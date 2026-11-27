//! First-release discovery and regeneration. Only the newest published release of a tag stream is
//! read, and only the `<mod>-<version>.release.json` that the trusted release workflow signed into
//! the package's attestation names its package path. Tags and file names are hints.
use std::collections::BTreeMap;

use serde::Deserialize;

use super::*;
use crate::schema::{CalendarDate, Login, Project, Registrant};

struct FirstCandidate<'a> {
    namespace: Option<ModId>,
    candidate: Candidate<'a>,
}

/// The newest published release of each tag stream (the root, or one subfolder namespace) that no
/// registered project claims, newest publication first. A stream whose newest release carries no
/// metadata is not admitted, and nothing older of it is considered.
fn first_candidates<'a>(
    registry: &Registry,
    repository: RepositoryId,
    releases: &'a [github::Release],
) -> Vec<FirstCandidate<'a>> {
    let claimed = registry
        .projects
        .values()
        .filter(|entry| entry.project.repository == repository)
        .map(|entry| match entry.project.path {
            PackagePath::Root(_) => None,
            PackagePath::Folder(_) => Some(entry.project.mod_id.clone()),
        })
        .collect::<BTreeSet<_>>();
    let mut newest = releases
        .iter()
        .filter_map(|release| {
            let published = release.published()?;
            let (namespace, version) = tag_parts(&release.tag_name)?;
            let namespace = namespace
                .map(|id| ModId::parse(id.to_owned()))
                .transpose()
                .ok()?;
            (!claimed.contains(&namespace)).then_some(FirstCandidate {
                namespace,
                candidate: Candidate {
                    release,
                    published,
                    version,
                },
            })
        })
        .collect::<Vec<_>>();
    newest.sort_by(|a, b| {
        (b.candidate.published, &b.candidate.version, &b.namespace).cmp(&(
            a.candidate.published,
            &a.candidate.version,
            &a.namespace,
        ))
    });
    let mut streams = BTreeSet::new();
    newest.retain(|first| streams.insert(first.namespace.clone()));
    newest
}

fn asset_mod_id(first: &FirstCandidate<'_>) -> Result<ModId, Observation> {
    if let Some(id) = &first.namespace {
        return Ok(id.clone());
    }
    let suffix = format!("-{}.smod", first.candidate.version);
    let mut packages = first
        .candidate
        .release
        .assets
        .iter()
        .filter_map(|asset| asset.name.strip_suffix(&suffix));
    match (packages.next(), packages.next()) {
        (Some(id), None) => {
            ModId::parse(id.to_owned()).map_err(|e| Observation::Ineligible(e.into()))
        }
        _ => Err(Observation::Ineligible(
            "expected exactly one <mod>-<version>.smod asset for the root tag".into(),
        )),
    }
}

/// Called for the newest candidate of a stream. The package must authenticate, and the release
/// must carry its `<mod>-<version>.release.json` as an asset whose name and SHA-256 are a subject
/// of the same verified statement as the package. The path comes from that file alone. Two streams
/// that sign one path derive one project key, so the open-PR, publish and collision checks admit
/// only one project for it.
pub(super) fn newest_path(
    request: &Request<'_>,
    namespace: Option<&ModId>,
    dir: &Path,
    claim: Option<&Release>,
) -> Result<(PackagePath, SignedPackage), Observation> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ReleaseJson {
        path: String,
    }
    // The document holds one package path: Linux PATH_MAX (`limits.h`) is the longest a checkout can name.
    const MAX_RELEASE_JSON: u64 = 4096;
    let name = format!(
        "{}-{}.release.json",
        request.mod_id.as_str(),
        request.version
    );
    let live = request
        .release
        .asset(&name)
        .map_err(Observation::Ineligible)?;
    if live.size > MAX_RELEASE_JSON {
        return Err(Observation::Ineligible(format!(
            "{name} is larger than {MAX_RELEASE_JSON} bytes"
        )));
    }
    let url = release_url(request, live)?;
    let package = signed_package(request, dir, claim)?;
    let file = dir.join("release.json");
    let sha256 = download_asset(request.github, url, &file, live)?;
    if !package
        .attestation
        .subjects
        .contains(&(name.clone(), sha256))
    {
        return Err(Observation::Ineligible(format!(
            "{name} is not a subject of the verified package attestation"
        )));
    }
    let bytes = fs::read(&file).map_err(|e| Observation::Unknown(e.to_string()))?;
    let ReleaseJson { path } = serde_json::from_slice(&bytes)
        .map_err(|e| Observation::Ineligible(format!("invalid {name}: {e}")))?;
    let path = PackagePath::parse(path).map_err(|e| Observation::Ineligible(e.into()))?;
    if matches!(path, PackagePath::Root(_)) != namespace.is_none() {
        return Err(Observation::Ineligible(format!(
            "{name} path does not match the release tag namespace"
        )));
    }
    Ok((path, package))
}

fn first_request<'a>(
    live: &'a github::Repository,
    candidate: &'a Candidate<'_>,
    mod_id: &'a ModId,
    workflow: &'a str,
    github: &'a dyn GitHub,
) -> Request<'a> {
    Request {
        github,
        mod_id,
        registered_kind: None,
        repository_id: live.id,
        repository: &live.name,
        workflow,
        release: candidate.release,
        version: &candidate.version,
    }
}

pub(super) struct Watch<'a, 'gh> {
    pub registry: &'a Registry,
    pub branches: &'a github::ReleaseBranches,
    pub out: &'a Path,
    pub github: &'gh dyn GitHub,
    pub workflow: &'a str,
    pub categories: &'a page::Categories,
    pub builds: &'a mut Builds<'gh>,
}

impl Watch<'_, '_> {
    pub(super) fn observe(
        self,
        live: &github::Repository,
        releases: &[github::Release],
        summary: &mut String,
    ) {
        // A repository row from before registrants were recorded cannot name an owner. Its old-format
        // streams (no `release.json` asset) stay unreported, so they cannot flood the summary; a stream
        // that carries one is a signed first release, and is reported if it cannot be taken.
        let legacy = registrants(self.registry, live.id).is_err();
        for first in first_candidates(self.registry, live.id, releases) {
            let candidate = &first.candidate;
            let label = &candidate.release.tag_name;
            let result = (|| {
                if legacy
                    && !candidate
                        .release
                        .assets
                        .iter()
                        .any(|asset| asset.name.ends_with(".release.json"))
                {
                    return Ok(false);
                }
                let mod_id = asset_mod_id(&first).map_err(|e| e.to_string())?;
                // An open or refused PR already holds this exact release: the live release names
                // the same repository ID, tag and package SHA-256 as its row.
                let package_name = format!("{}-{}.smod", mod_id.as_str(), candidate.version);
                if let Ok(asset) = candidate.release.asset(&package_name)
                    && let Some(digest) = asset
                        .digest
                        .as_deref()
                        .and_then(|d| d.strip_prefix("sha256:"))
                    && let Ok(sha256) = Sha256::parse(digest.to_owned())
                    && self
                        .branches
                        .examined
                        .contains(&(live.id, label.clone(), sha256))
                {
                    return Ok(false);
                }
                let request = first_request(live, candidate, &mod_id, self.workflow, self.github);
                let work = tempfile::tempdir().map_err(|e| e.to_string())?;
                let (path, package) =
                    newest_path(&request, first.namespace.as_ref(), work.path(), None)
                        .map_err(|e| e.to_string())?;
                if self.registry.has_package_path(live.id, &path) {
                    return Ok(false);
                }
                let id = registry::first_project_id(live.id, &path)?;
                let branch = format!("release/{}/{}", id.as_str(), candidate.version);
                if has_open_release(&self.branches.waiting, &id)
                    || self.branches.handled.contains(&branch)
                {
                    return Ok(false);
                }
                registrants(self.registry, live.id).map_err(|e| e.to_string())?;
                let assets =
                    signed_page(&request, work.path(), None, package).map_err(|e| e.to_string())?;
                let (row, payload) = prepared(
                    &request,
                    candidate.published,
                    assets,
                    work.path(),
                    self.categories,
                    self.builds,
                    None,
                )?;
                if let Some(missing) = unsupplied(self.registry, &id, &row) {
                    return Err(missing);
                }
                let project = first_project(self.registry, &request, id, path, &row)?;
                release_payload::source(
                    &work.path().join("package.smod"),
                    &work.path().join("source"),
                    payload.package.is_native(),
                )?;
                let root = self.out.join("first-releases").join(project.id.as_str());
                fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                let stage = tempfile::tempdir_in(&root).map_err(|e| e.to_string())?;
                write_first(&project, &row, &payload, stage.path())?;
                fs::rename(stage.path(), root.join(row.version.to_string()))
                    .map_err(|e| e.to_string())?;
                Ok(true)
            })();
            match result {
                Ok(true) => note(summary, label, "verified; ready to open a first-release PR"),
                Ok(false) => (),
                Err(reason) => note(summary, label, &reason),
            }
        }
    }
}

fn first_project(
    registry: &Registry,
    request: &Request<'_>,
    id: ProjectId,
    path: PackagePath,
    row: &Release,
) -> Result<Project, String> {
    let project = Project {
        slug: registry.first_slug(&id, request.mod_id)?,
        id,
        mod_id: request.mod_id.clone(),
        kind: row.kind,
        owners: owners(registry, request.repository_id, request.github)
            .map_err(|e| e.to_string())?,
        repository: request.repository_id,
        path,
        workflow: Text::parse(request.workflow.to_owned())?,
        registered: CalendarDate::parse(row.published.date().to_owned())?,
        paused: false,
    };
    registry.check_new_project(&project)?;
    Ok(project)
}

/// The accounts that registered the repository: the only owners a new project can have. A row
/// without any is not given a fallback owner; a reviewed change to the row has to record one.
fn registrants(
    registry: &Registry,
    repository: RepositoryId,
) -> Result<&[Registrant], &'static str> {
    let Some(repo) = registry.repositories.get(&repository) else {
        return Err("first-release repository is not registered");
    };
    if repo.registrant.is_empty() {
        return Err(
            "signed first release cannot be taken: the repository's registry row records no registrant to own the new project; a reviewed registry change must record one",
        );
    }
    Ok(&repo.registrant)
}

fn owners(
    registry: &Registry,
    repository: RepositoryId,
    github: &dyn GitHub,
) -> Result<Vec<Login>, Observation> {
    let registrants =
        registrants(registry, repository).map_err(|e| Observation::Ineligible(e.into()))?;
    let mut logins = BTreeMap::new();
    for registrant in registrants {
        let account = github.account(registrant.id)?;
        if account.id != registrant.id {
            return Err(Observation::Ineligible(
                "GitHub returned the wrong account ID for a registrant".into(),
            ));
        }
        logins.insert(account.id, account.login);
    }
    Ok(logins.into_values().collect())
}

pub fn verify(
    main: &Path,
    id: &ProjectId,
    claim: &Path,
    out: &Path,
    github: &dyn GitHub,
) -> Result<(), String> {
    let registry = load(main)?;
    let claim: Release = toml::from_str(&fs::read_to_string(claim).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let workflow = workflow(&registry)?;
    let live = github.repository(Repo::Registered(claim.repository))?;
    if live.id != claim.repository || live.private {
        return Err("registered repository is no longer public or has the wrong ID".into());
    }
    let releases = github.releases(Repo::Registered(live.id))?;
    let first = first_candidates(&registry, live.id, &releases)
        .into_iter()
        .find(|first| {
            first.candidate.release.tag_name == claim.tag.as_str()
                && first.candidate.version == claim.version
        })
        .ok_or(
            "the proposed first release is not the newest publication of a new package stream",
        )?;
    let candidate = &first.candidate;
    let mod_id = asset_mod_id(&first).map_err(|e| e.to_string())?;
    let request = first_request(&live, candidate, &mod_id, workflow, github);
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (path, package) = newest_path(
        &request,
        first.namespace.as_ref(),
        work.path(),
        Some(&claim),
    )
    .map_err(|e| e.to_string())?;
    let key = registry::first_project_id(live.id, &path)?;
    if &key != id {
        return Err("first-release project ID does not match the release branch".into());
    }
    let assets =
        signed_page(&request, work.path(), Some(&claim), package).map_err(|e| e.to_string())?;
    let (row, payload) = prepared(
        &request,
        candidate.published,
        assets,
        work.path(),
        &categories(&registry)?,
        &mut Builds::new(github)?,
        Some(&claim),
    )?;
    if row != claim {
        return Err("release row does not match the attested assets and published metadata".into());
    }
    let project = first_project(&registry, &request, key, path, &row)?;
    check_payload(&registry, id, &row, &payload, work.path())?;
    write_first(&project, &row, &payload, &out.join("expected"))?;
    release_payload::source(
        &work.path().join("package.smod"),
        &out.join("source"),
        payload.package.is_native(),
    )
}

pub(super) fn write_first(
    project: &Project,
    row: &Release,
    payload: &Payload,
    out: &Path,
) -> Result<(), String> {
    let text = toml::to_string_pretty(project).map_err(|e| e.to_string())?;
    write_release(
        row,
        payload,
        &out.join(project_path(&project.id)),
        Some(&text),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two-ports");
    #[test]
    fn discovery_takes_only_the_newest_release_of_each_unclaimed_stream() {
        let registry = load(Path::new(FIXTURE)).unwrap();
        let repo = serde_json::from_str("123456789").unwrap();
        let releases: Vec<github::Release> = serde_json::from_value(serde_json::json!([
            {"draft":false,"tag_name":"other/v1.0.0","published_at":"2026-11-23T10:00:00Z","assets":[]},
            {"draft":false,"tag_name":"second/v2.0.0","published_at":"2026-11-23T11:00:00Z","assets":[]},
            {"draft":false,"tag_name":"v3.0.0","published_at":"2026-11-23T12:00:00Z","assets":[]},
            {"draft":false,"tag_name":"other/v0.9.0","published_at":"2026-11-23T12:30:00Z","assets":[]},
            {"draft":true,"tag_name":"second/v2.1.0","published_at":"2026-11-23T14:00:00Z","assets":[]},
            {"draft":false,"tag_name":"not-a-tag","published_at":"2026-11-23T14:00:00Z","assets":[]}
        ])).unwrap();
        // The root and wayfinder streams are claimed. In `other`, the newest publication is
        // chosen by time, not by version, and its older tag is never a candidate even though
        // neither release carries any metadata.
        let proposed = first_candidates(&registry, repo, &releases);
        assert_eq!(
            proposed
                .iter()
                .map(|first| first.candidate.release.tag_name.as_str())
                .collect::<Vec<_>>(),
            ["other/v0.9.0", "second/v2.0.0"]
        );
    }

    struct Accounts {
        wrong_id: bool,
        offline: bool,
    }

    impl GitHub for Accounts {
        fn repository(&self, _: Repo<'_>) -> Result<github::Repository, String> {
            unreachable!()
        }
        fn releases(&self, _: Repo<'_>) -> Result<Vec<github::Release>, String> {
            unreachable!()
        }
        fn release_branches(
            &self,
            _: &BTreeMap<ProjectId, ProjectEntry>,
        ) -> Result<github::ReleaseBranches, String> {
            unreachable!()
        }
        fn tag_commit(&self, _: Repo<'_>, _: &str) -> Result<String, String> {
            unreachable!()
        }
        fn account(&self, id: NonZeroU64) -> Result<github::Account, Observation> {
            assert_eq!(id.get(), 19);
            if self.offline {
                return Err(Observation::Unknown("GitHub unavailable".into()));
            }
            Ok(serde_json::from_value(serde_json::json!({"id": if self.wrong_id { 20 } else { 19 }, "login": "CurrentAdmin"})).unwrap())
        }
    }

    #[test]
    fn first_owners_are_resolved_by_registrant_account_not_stored_login_or_tag_publisher() {
        let mut registry = load(Path::new(FIXTURE)).unwrap();
        let repository = serde_json::from_str("123456789").unwrap();
        let row = registry.repositories.get_mut(&repository).unwrap();
        row.registrant = toml::from_str::<schema::Repository>(
            "name = 'example/wayfinder'\n[[registrant]]\nid = 19\nlogin = 'PreviousAdmin'\n",
        )
        .unwrap()
        .registrant;
        let current = owners(
            &registry,
            repository,
            &Accounts {
                wrong_id: false,
                offline: false,
            },
        )
        .unwrap();
        assert_eq!(current[0].as_str(), "CurrentAdmin");
        assert!(matches!(
            owners(
                &registry,
                repository,
                &Accounts {
                    wrong_id: true,
                    offline: false
                }
            ),
            Err(Observation::Ineligible(_))
        ));
        assert!(matches!(
            owners(
                &registry,
                repository,
                &Accounts {
                    wrong_id: false,
                    offline: true
                }
            ),
            Err(Observation::Unknown(_))
        ));
        assert_eq!(
            registry.projects.values().next().unwrap().project.owners[0].as_str(),
            "Example"
        );
    }
}
