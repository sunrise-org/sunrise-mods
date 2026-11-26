use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

use percent_encoding::percent_decode_str;
use semver::Version;

use crate::{
    builds::{self, Offered},
    schema::*,
    tree::{Entry, Tree},
};
use sunrise_mod_package::{
    icon::MAX_ICON,
    index::v2::collection::{Collection, ICON, Part, Registered, TOML, check_collection},
};

/// The site's own pages under `/mods/` (sunny `frontend/site/src/pages/mods/`: `mine.astro`, `policy.astro`,
/// `registry-page.astro`, `submit.astro`). A project's page is `/mods/<slug>`, so no project takes one of these.
const SITE_PAGES: [&str; 4] = ["mine", "policy", "registry-page", "submit"];

pub struct Registry {
    pub repositories: BTreeMap<RepositoryId, Repository>,
    pub release_workflow: WorkflowUse,
    pub categories: Vec<Category>,
    pub projects: BTreeMap<ProjectId, ProjectEntry>,
    pub collections: BTreeMap<ProjectId, CollectionEntry>,
    pub fits: BTreeMap<ProjectId, BTreeMap<Version, FitRelease>>,
    pub builds: Vec<Offered>,
    /// The SHA-512 of each original jar Modrinth does not list, as the judging job stored them.
    pub unlisted_jars: BTreeSet<String>,
    pub revoked: Vec<Revocation>,
}

pub struct ProjectEntry {
    pub project: Project,
    pub repository_name: RepositoryName,
    pub page: Option<Page>,
    pub releases: BTreeMap<Version, Release>,
}

pub struct CollectionEntry {
    pub collection: Collection,
    /// The one GitHub account that owns every member's repository.
    pub publisher: String,
}

pub fn load(checkout: &Path) -> Result<Registry, Vec<String>> {
    load_inner(checkout, false)
}

pub fn load_generated(checkout: &Path) -> Result<Registry, Vec<String>> {
    load_inner(checkout, true)
}

fn load_inner(checkout: &Path, generated: bool) -> Result<Registry, Vec<String>> {
    let mut tree = Tree::new(checkout);
    let root = Path::new("registry");
    let mut top = match tree.entry(root) {
        Some(Entry::Directory) => tree.list(root),
        Some(Entry::File) => return Err(vec!["registry: expected a directory".to_owned()]),
        None => return Err(tree.finish()),
    };
    let repositories_root = root.join("repositories");
    let mut repository_files = match top.remove(OsStr::new("repositories")) {
        Some(Entry::Directory) => tree.list(&repositories_root),
        _ => {
            tree.problem(&repositories_root, "expected a repositories directory");
            Default::default()
        }
    };
    if repository_files.remove(OsStr::new(".gitkeep")) == Some(Entry::Directory) {
        tree.problem(
            &repositories_root.join(".gitkeep"),
            "expected a regular file",
        );
    }
    let categories = tree
        .toml::<Categories>(
            &root.join("categories.toml"),
            top.remove(OsStr::new("categories.toml")),
        )
        .map_or_else(Vec::new, |value| value.category);
    let revoked_path = root.join("revoked.toml");
    let revoked = tree
        .toml::<Revocations>(&revoked_path, top.remove(OsStr::new("revoked.toml")))
        .map_or_else(Vec::new, |value| value.revoked);
    let release_workflow = tree.toml::<ReleaseWorkflow>(
        &root.join("release-workflow.toml"),
        top.remove(OsStr::new("release-workflow.toml")),
    );
    let projects_root = root.join("projects");
    let mut project_dirs = match top.remove(OsStr::new("projects")) {
        Some(Entry::Directory) => tree.list(&projects_root),
        _ => {
            tree.problem(&projects_root, "expected a projects directory");
            Default::default()
        }
    };
    if project_dirs.remove(OsStr::new(".gitkeep")) == Some(Entry::Directory) {
        tree.problem(&projects_root.join(".gitkeep"), "expected a regular file");
    }
    if let Some(entry) = top.remove(OsStr::new("index-key.pub"))
        && entry != Entry::File
    {
        tree.problem(
            &root.join("index-key.pub"),
            "expected a regular public key file",
        );
    }
    let collections_root = root.join("collections");
    let collections_dir = top.remove(OsStr::new("collections"));
    let fit_path = root.join("fit");
    let fit_dir = top.remove(OsStr::new("fit"));
    if !generated && fit_dir.is_some() {
        tree.problem(
            &fit_path,
            "fit is generated during publishing and must not be committed",
        );
    }
    tree.unknown(root, top);

    let mut by_category = BTreeMap::new();
    for category in &categories {
        if by_category.insert(&category.id, category).is_some() {
            tree.problem(
                &root.join("categories.toml"),
                format!("duplicate category {}", category.id.as_str()),
            );
        }
    }
    let mut repositories = BTreeMap::new();
    for (name, entry) in repository_files {
        let path = repositories_root.join(&name);
        let Some(id) = name.to_str().and_then(RepositoryId::of_file) else {
            tree.problem(&path, "expected a file named <numeric repository ID>.toml");
            continue;
        };
        if let Some(repository) = tree.toml::<Repository>(&path, Some(entry)) {
            repositories.insert(id, (path, repository));
        }
    }
    let mut registrations = BTreeMap::new();
    for (id, (path, repo)) in &repositories {
        let mut package_paths = BTreeSet::new();
        for package in &repo.package {
            if !package_paths.insert(&package.path) {
                tree.problem(
                    path,
                    format!(
                        "duplicate package path in {} for project {}",
                        repo.name.as_str(),
                        package.project.as_str()
                    ),
                );
            }
            if registrations
                .insert(&package.project, (*id, path, repo, package))
                .is_some()
            {
                tree.problem(
                    path,
                    format!(
                        "duplicate registered project ID {}",
                        package.project.as_str()
                    ),
                );
            }
        }
    }

    let mut projects = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    let mut subfolder_mods = BTreeSet::new();
    for (name, entry) in project_dirs {
        let dir = projects_root.join(&name);
        let Ok(id) = ProjectId::parse(name.to_string_lossy().into_owned()) else {
            tree.problem(
                &dir,
                "expected a directory named by an 8-character base62 project ID",
            );
            continue;
        };
        if entry != Entry::Directory {
            tree.problem(&dir, "expected a project directory");
            continue;
        }
        let project_path = dir.join("project.toml");
        let mut entries = tree.list(&dir);
        let project =
            tree.toml::<Project>(&project_path, entries.remove(OsStr::new("project.toml")));
        let releases_dir = dir.join("releases");
        let releases = match entries.remove(OsStr::new("releases")) {
            Some(Entry::Directory) => read_releases(&mut tree, &releases_dir, project.as_ref()),
            Some(Entry::File) => {
                tree.problem(&releases_dir, "expected a releases directory");
                BTreeMap::new()
            }
            None => BTreeMap::new(),
        };
        let page_dir = dir.join("page");
        let page = match entries.remove(OsStr::new("page")) {
            Some(Entry::Directory) => read_page(
                &mut tree,
                &page_dir,
                &by_category,
                project.as_ref().map(|p| p.kind),
            ),
            entry => {
                if entry.is_some() || !releases.is_empty() {
                    tree.problem(
                        &page_dir,
                        "expected a page directory with page.toml and images",
                    );
                }
                None
            }
        };
        tree.unknown(&dir, entries);
        let registration = registrations.remove(&id);
        if let Some(project) = project {
            if project.id != id {
                tree.problem(&project_path, "project ID must match the directory name");
            }
            if SITE_PAGES.contains(&project.slug.as_str()) {
                tree.problem(
                    &project_path,
                    format!(
                        "reserved slug {}: the site has its own page at /mods/{}",
                        project.slug.as_str(),
                        project.slug.as_str()
                    ),
                );
            }
            if !slugs.insert(project.slug.clone()) {
                tree.problem(
                    &project_path,
                    format!("duplicate slug {}", project.slug.as_str()),
                );
            }
            if matches!(project.path, PackagePath::Folder(_))
                && !subfolder_mods.insert((project.repository, project.mod_id.clone()))
            {
                tree.problem(&project_path, "another subfolder package in this repository has the same in-game ID (shared tag and caller workflow names)");
            }
            if project.workflow.as_str() != RELEASE_WORKFLOW {
                tree.problem(
                    &project_path,
                    "workflow must match the path in release-workflow.toml before '@'",
                );
            }
            match registration {
                Some((repository, _, repo, package)) => {
                    if repository != project.repository {
                        tree.problem(
                            &project_path,
                            "repository ID does not match the repository file that lists this project",
                        );
                    }
                    if package.path != project.path {
                        tree.problem(
                            &project_path,
                            "package path does not match its repository file",
                        );
                    }
                    projects.insert(
                        id,
                        ProjectEntry {
                            project,
                            repository_name: repo.name.clone(),
                            page,
                            releases,
                        },
                    );
                }
                None => tree.problem(
                    &project_path,
                    "project is not listed in a registry/repositories file",
                ),
            }
        }
    }
    for (id, (_, path, _, _)) in &registrations {
        tree.problem(path, format!("project {} has no project.toml", id.as_str()));
    }
    for (id, entry) in &projects {
        for release in entry.releases.values() {
            let path = projects_root
                .join(id.as_str())
                .join("releases")
                .join(format!("{}.toml", release.version));
            if release.kind != entry.project.kind {
                tree.problem(&path, "release kind does not match the project kind");
            }
            for dependency in &release.dependencies {
                if let Some(target) = &dependency.project {
                    match projects.get(target) {
                        None => tree.problem(
                            &path,
                            format!("dependency names unregistered project {}", target.as_str()),
                        ),
                        Some(provider) if provider.project.mod_id != dependency.mod_id => tree
                            .problem(
                                &path,
                                format!(
                                    "dependency project {} does not provide {}",
                                    target.as_str(),
                                    dependency.mod_id.as_str()
                                ),
                            ),
                        Some(_) => {}
                    }
                }
            }
        }
    }
    let collections = match collections_dir {
        Some(Entry::Directory) => read_collections(&mut tree, &collections_root, &projects),
        Some(Entry::File) => {
            tree.problem(&collections_root, "expected a collections directory");
            BTreeMap::new()
        }
        None => BTreeMap::new(),
    };
    let mut revocations = BTreeSet::new();
    for row in &revoked {
        if !revocations.insert((&row.project, &row.sha256)) {
            tree.problem(
                &revoked_path,
                format!(
                    "duplicate takedown for project {} and hash",
                    row.project.as_str()
                ),
            );
        }
    }
    let mut fits = BTreeMap::new();
    let mut builds = Vec::new();
    let mut unlisted_jars = BTreeSet::new();
    if generated {
        match fit_dir {
            Some(Entry::Directory) => {
                let mut contents = tree.list(&fit_path);
                let builds_path = fit_path.join("builds.json");
                match contents.remove(OsStr::new("builds.json")) {
                    Some(Entry::File) => {
                        match builds::read(&checkout.join(&builds_path), |build: &Offered| {
                            (&build.id, &build.commit)
                        }) {
                            Ok(rows) => builds = rows,
                            Err(message) => tree.problem(&builds_path, message),
                        }
                    }
                    _ => tree.problem(&builds_path, "expected the generated offered-build list"),
                }
                // The judging job's own record of the Sunrise releases it has looked at.
                if contents.remove(OsStr::new(crate::fit::CONSIDERED)) == Some(Entry::Directory) {
                    tree.problem(
                        &fit_path.join(crate::fit::CONSIDERED),
                        "expected a regular file",
                    );
                }
                // Absent until the judging job has first asked Modrinth.
                if contents.remove(OsStr::new(crate::jar::UNLISTED)).is_some() {
                    let path = fit_path.join(crate::jar::UNLISTED);
                    match std::fs::read(checkout.join(&path))
                        .map_err(|e| e.to_string())
                        .and_then(|bytes: Vec<u8>| {
                            serde_json::from_slice(&bytes).map_err(|e| e.to_string())
                        }) {
                        Ok(jars) => unlisted_jars = jars,
                        Err(message) => tree.problem(&path, message),
                    }
                }
                fits = read_fits(&mut tree, &fit_path, contents);
            }
            _ => tree.problem(&fit_path, "expected the generated fit directory"),
        }
    }
    let problems = tree.finish();
    if !problems.is_empty() {
        return Err(problems);
    }
    let Some(release_workflow) = release_workflow else {
        return Err(vec!["registry: missing release-workflow.toml".to_owned()]);
    };
    Ok(Registry {
        repositories: repositories
            .into_iter()
            .map(|(id, (_, repository))| (id, repository))
            .collect(),
        release_workflow: release_workflow.uses,
        categories,
        projects,
        collections,
        fits,
        builds,
        unlisted_jars,
        revoked,
    })
}

fn read_collections(
    tree: &mut Tree<'_>,
    root: &Path,
    projects: &BTreeMap<ProjectId, ProjectEntry>,
) -> BTreeMap<ProjectId, CollectionEntry> {
    let mut collections = BTreeMap::new();
    for (name, entry) in tree.list(root) {
        let dir = root.join(&name);
        if entry != Entry::Directory {
            tree.problem(&dir, "expected a collection directory");
            continue;
        }
        let folder = name.to_string_lossy().into_owned();
        let file = dir.join(TOML);
        let icon_path = dir.join(ICON);
        let mut entries = tree.list(&dir);
        let collection = tree.toml::<Collection>(&file, entries.remove(OsStr::new(TOML)));
        let icon = match entries.get(OsStr::new(ICON)) {
            Some(Entry::File) => tree.bytes(&icon_path, MAX_ICON as u64 + 1),
            _ => None,
        };
        let names: Vec<String> = entries
            .keys()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let lookup = |project: &str| {
            let entry = projects.get(&ProjectId::parse(project.to_owned()).ok()?)?;
            Some(Registered {
                publisher: entry
                    .repository_name
                    .as_str()
                    .split_once('/')
                    .expect("validated when parsed")
                    .0
                    .to_owned(),
                owners: entry
                    .project
                    .owners
                    .iter()
                    .map(|login| login.as_str().to_owned())
                    .collect(),
            })
        };
        let report = check_collection(
            &folder,
            &names,
            collection.as_ref(),
            icon.as_deref(),
            &lookup,
        );
        for problem in report.problems {
            let path = match problem.part {
                Part::Folder => dir.clone(),
                Part::Toml => file.clone(),
                Part::Icon => icon_path.clone(),
                Part::Entry(name) => dir.join(name),
            };
            tree.problem(&path, problem.message);
        }
        if let (Some(collection), Some(publisher), Ok(id)) =
            (collection, report.publisher, ProjectId::parse(folder))
        {
            collections.insert(
                id,
                CollectionEntry {
                    collection,
                    publisher,
                },
            );
        }
    }
    collections
}

/// A pull request never takes a listed project off the registry by accident: a project with
/// releases on `base` still has one on `head`, unless `head` pauses it. Of `base` only the names
/// under `registry/projects/` are read, so a pull request that changes what a file holds is
/// still compared.
pub fn stay_listed(base: &Path, head: &Path) -> Result<(), String> {
    let after = load(head).map_err(|e| e.join("\n"))?;
    let projects = base.join("registry/projects");
    let mut unlisted = Vec::new();
    for entry in std::fs::read_dir(&projects).map_err(|e| format!("{}: {e}", projects.display()))? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| ProjectId::parse(name.into()).ok())
        else {
            continue;
        };
        let was_listed = std::fs::read_dir(projects.join(&name).join("releases"))
            .is_ok_and(|mut releases| releases.next().is_some());
        let gone = after
            .projects
            .get(&id)
            .is_none_or(|now| now.releases.is_empty() && !now.project.paused);
        if was_listed && gone {
            unlisted.push(format!(
                "registry/projects/{}: unlisted project: this change removes its last listed release. \
                 A release of another kind replaces the earlier ones in its own release pull request; \
                 to take the project off the registry, set `paused = true` in the same change",
                id.as_str()
            ));
        }
    }
    unlisted.sort();
    if unlisted.is_empty() {
        Ok(())
    } else {
        Err(unlisted.join("\n"))
    }
}

/// A registration adds one or more projects of one repository, with their packages, and nothing
/// else. Under `registry/`, `head` differs from `base` only in the new projects' `project.toml`
/// and in that repository's file, which is new with exactly those packages or keeps its name
/// and its packages in their order and gains exactly those. Where the new packages stand in the
/// file does not matter. Every other file keeps its bytes and its executable bit, and no empty
/// directory (what a submodule checks out as) comes or goes; of `base` only the repository's
/// file is parsed.
pub fn registration(base: &Path, head: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let after = load(head).map_err(|e| e.join("\n"))?;
    // Each file with whether it is executable, and each empty directory as `None`.
    let entries = |checkout: &Path| {
        let mut tree = Tree::new(checkout);
        let mut found = BTreeMap::new();
        let mut dirs = Vec::new();
        match tree.entry(Path::new("registry")) {
            Some(Entry::Directory) => dirs.push(PathBuf::from("registry")),
            Some(Entry::File) => tree.problem(Path::new("registry"), "expected a directory"),
            None => (),
        }
        while let Some(dir) = dirs.pop() {
            let listed = tree.list(&dir);
            if listed.is_empty() {
                found.insert(dir.clone(), None);
            }
            for (name, entry) in listed {
                let path = dir.join(name);
                match entry {
                    Entry::Directory => dirs.push(path),
                    Entry::File => match std::fs::metadata(checkout.join(&path)) {
                        Ok(meta) => {
                            found.insert(path, Some(meta.permissions().mode() & 0o111 != 0));
                        }
                        Err(error) => tree.problem(&path, error),
                    },
                }
            }
        }
        let problems = tree.finish();
        if problems.is_empty() {
            Ok(found)
        } else {
            Err(problems.join("\n"))
        }
    };
    let (before_files, after_files) = (entries(base)?, entries(head)?);
    let project_file = |id: &ProjectId| {
        Path::new("registry/projects")
            .join(id.as_str())
            .join("project.toml")
    };
    let mut changed = BTreeSet::new();
    let paths: BTreeSet<_> = before_files.keys().chain(after_files.keys()).collect();
    for path in paths {
        let (was, is) = (before_files.get(path), after_files.get(path));
        let read = |checkout: &Path| {
            let file = checkout.join(path);
            std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))
        };
        if was != is || (matches!(is, Some(Some(_))) && read(base)? != read(head)?) {
            changed.insert(path.clone());
        }
    }
    let added: BTreeMap<_, _> = after
        .projects
        .iter()
        .filter(|(id, _)| !before_files.contains_key(&project_file(id)))
        .collect();
    let Some(repository) = added.values().next().map(|new| new.project.repository) else {
        return Err("a registration adds at least one project; this change adds none".to_owned());
    };
    if let Some((id, _)) = added
        .iter()
        .find(|(_, new)| new.project.repository != repository)
    {
        return Err(format!(
            "{}: a registration adds projects of one repository; this one is not in repository {}",
            project_file(id).display(),
            repository.get()
        ));
    }
    let repository_file =
        Path::new("registry/repositories").join(format!("{}.toml", repository.get()));
    let permitted: BTreeSet<_> = added
        .keys()
        .map(|id| project_file(id))
        .chain([repository_file.clone()])
        .collect();
    if let Some(other) = changed.difference(&permitted).next() {
        return Err(format!(
            "{}: a registration changes only its new projects' project.toml and {}",
            other.display(),
            repository_file.display()
        ));
    }
    if let Some(file) = permitted
        .iter()
        .find(|file| after_files.get(*file) != Some(&Some(false)))
    {
        return Err(format!(
            "{}: expected a file that is not executable",
            file.display()
        ));
    }
    let now = &after.repositories[&repository];
    let kept = now
        .package
        .iter()
        .filter(|package| !added.contains_key(&package.project));
    let unchanged = if before_files.contains_key(&repository_file) {
        let file = base.join(&repository_file);
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let before = toml::from_str::<Repository>(&text)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        before.name == now.name && kept.eq(&before.package)
    } else {
        kept.count() == 0
    };
    if unchanged {
        Ok(())
    } else {
        Err(format!(
            "{}: a registration adds its new packages and leaves the repository's name and other packages as they were",
            repository_file.display()
        ))
    }
}

/// Refuses a change to a collection by anyone but an owner of every project it lists, before
/// and after the change. Ownership is read from `base`, so a pull request cannot grant it to
/// itself. Of `base` only the collection files and the members' `project.toml` are read: the
/// base commit was checked by its own checker, and a pull request may change what a row holds.
pub fn collection_author(base: &Path, head: &Path, login: &str) -> Result<(), String> {
    let after = load(head).map_err(|e| e.join("\n"))?;
    let read = |path: PathBuf| match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    };
    let collections = Path::new("registry/collections");
    let mut ids: BTreeSet<ProjectId> = after.collections.keys().cloned().collect();
    if let Ok(entries) = std::fs::read_dir(base.join(collections)) {
        for entry in entries {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|name| ProjectId::parse(name.into()).ok())
            {
                ids.insert(id);
            }
        }
    }
    let files = |checkout: &Path, id: &ProjectId| {
        let dir = checkout.join(collections).join(id.as_str());
        (
            std::fs::read(dir.join("collection.toml")).ok(),
            std::fs::read(dir.join("icon.png")).ok(),
        )
    };
    for id in &ids {
        if files(base, id) == files(head, id) {
            continue;
        }
        let file = base
            .join(collections)
            .join(id.as_str())
            .join("collection.toml");
        let before = read(file.clone())?
            .map(|text| toml::from_str::<Collection>(&text))
            .transpose()
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let members = before
            .iter()
            .chain(after.collections.get(id).map(|entry| &entry.collection))
            .flat_map(|collection| &collection.member);
        for member in members {
            let file = base
                .join("registry/projects")
                .join(member.project.as_str())
                .join("project.toml");
            let project = read(file.clone())?
                .map(|text| toml::from_str::<Project>(&text))
                .transpose()
                .map_err(|error| format!("{}: {error}", file.display()))?;
            let owns = project.is_some_and(|project| {
                project
                    .owners
                    .iter()
                    .any(|owner| owner.as_str().eq_ignore_ascii_case(login))
            });
            if !owns {
                return Err(format!(
                    "collection {} lists project {}, which {login} does not own on the base commit: only the owner of its mods curates a collection",
                    id.as_str(),
                    member.project.as_str()
                ));
            }
        }
    }
    Ok(())
}

fn read_releases(
    tree: &mut Tree<'_>,
    dir: &Path,
    project: Option<&Project>,
) -> BTreeMap<Version, Release> {
    let mut releases = BTreeMap::new();
    for (name, entry) in tree.list(dir) {
        let file = dir.join(name);
        let version = file
            .file_stem()
            .and_then(OsStr::to_str)
            .and_then(|stem| Version::parse(stem).ok());
        let Some(version) = version.filter(|_| file.extension() == Some(OsStr::new("toml"))) else {
            tree.problem(
                &file,
                "expected a <version>.toml release file named by a semantic version",
            );
            continue;
        };
        let Some(row) = tree.toml::<Release>(&file, Some(entry)) else {
            continue;
        };
        if version != row.version {
            tree.problem(&file, "release version must match its filename");
        }
        if let Some(project) = project {
            let expected = match project.path {
                PackagePath::Root(_) => format!("v{}", row.version),
                PackagePath::Folder(_) => format!("{}/v{}", project.mod_id.as_str(), row.version),
            };
            if row.tag.as_str() != expected {
                tree.problem(&file, format!("tag must be {expected}"));
            }
            if row.workflow.as_str().split_once('@').map(|(path, _)| path)
                != Some(project.workflow.as_str())
            {
                tree.problem(&file, "signer workflow path must match project.toml");
            }
            for (field, asset, suffix) in
                [("file", &row.file, "smod"), ("page", &row.page, "page.tar")]
            {
                let name = format!("{}-{}.{suffix}", project.mod_id.as_str(), row.version);
                let bundle = format!("{name}.sigstore.json");
                for (key, url, name) in [
                    ("url", &asset.url, &name),
                    ("attestation", &asset.attestation, &bundle),
                ] {
                    if release_download_repository(url.as_str(), row.tag.as_str(), name).is_none() {
                        tree.problem(&file, format!("{field}.{key} must be a GitHub release download HTTPS URL for {name} with the release tag"));
                    }
                }
            }
        }
        releases.insert(version, row);
    }
    releases
}

pub(crate) fn release_download_repository<'a>(
    url: &'a str,
    tag: &str,
    asset: &str,
) -> Option<&'a str> {
    // Check the raw authority and repository path, then decode only the release tag and asset.
    // GitHub's browser_download_url can percent-encode a tag such as wayfinder/v1.2.0.
    let (repository, download) = url
        .strip_prefix("https://github.com/")?
        .split_once("/releases/download/")?;
    let (tag_part, asset_part) = download.rsplit_once('/')?;
    (github_repository(repository)
        && !download.contains(['?', '#'])
        && percent_decode_str(tag_part).eq(tag.bytes())
        && percent_decode_str(asset_part).eq(asset.bytes()))
    .then_some(repository)
}

fn read_page(
    tree: &mut Tree<'_>,
    dir: &Path,
    categories: &BTreeMap<&Slug, &Category>,
    kind: Option<Kind>,
) -> Option<Page> {
    let mut entries = tree.list(dir);
    let file = dir.join("page.toml");
    let page = tree.toml::<Page>(&file, entries.remove(OsStr::new("page.toml")));
    let mut assets = BTreeSet::<PathBuf>::new();
    let mut pending = vec![(dir.to_owned(), entries)];
    while let Some((dir, entries)) = pending.pop() {
        for (name, entry) in entries {
            let path = dir.join(name);
            match entry {
                Entry::Directory => pending.push((path.clone(), tree.list(&path))),
                Entry::File => {
                    assets.insert(path);
                }
            }
        }
    }
    let page = page?;
    // The thresholds are from Modrinth validate/project/{summary.rs:13,description.rs:17}.
    // Counting trimmed Unicode scalar values matches the author-side release check.
    if page.summary.as_str().trim().chars().count() < 25 {
        tree.problem(&file, "summary needs at least 25 characters");
    }
    if page.description.as_str().trim().chars().count() < 125 {
        tree.problem(&file, "description needs at least 125 characters");
    }
    // Modrinth validate/project/tags.rs:10.
    if page.categories.len() > 8 {
        tree.problem(&file, "at most 8 categories are allowed");
    }
    let mut selected = BTreeSet::new();
    for id in &page.categories {
        if !selected.insert(id) {
            tree.problem(&file, format!("duplicate category {}", id.as_str()));
        }
        match categories.get(id) {
            None => tree.problem(&file, format!("unknown category {}", id.as_str())),
            Some(category) if kind.is_some_and(|kind| !category.kinds.contains(&kind)) => {
                tree.problem(
                    &file,
                    format!(
                        "category {} does not apply to this project kind",
                        id.as_str()
                    ),
                );
            }
            Some(_) => {}
        }
    }
    if page.screenshots.is_empty() {
        tree.problem(&file, "at least one screenshot is required");
    }
    let images: BTreeSet<_> = std::iter::once(&page.icon)
        .chain(page.screenshots.iter().map(|s| &s.path))
        .collect();
    for image in images {
        let path = dir.join(image.as_str());
        if assets.remove(&path) {
            if image == &page.icon {
                tree.icon(&path);
            } else {
                tree.screenshot(&path);
            }
        } else {
            tree.problem(
                &file,
                format!(
                    "image {:?} is not a regular file in the page directory",
                    image.as_str()
                ),
            );
        }
    }
    for asset in assets {
        tree.problem(&asset, "page file is not referenced by page.toml");
    }
    Some(page)
}

/// Reads the stored verdicts as written. A row for a release, project or build that is gone is
/// not a problem here: the judging run drops it. A release with no current verdict on an
/// offered build is not one either: the index leaves that cell out until it is judged.
fn read_fits(
    tree: &mut Tree<'_>,
    dir: &Path,
    contents: crate::tree::Entries,
) -> BTreeMap<ProjectId, BTreeMap<Version, FitRelease>> {
    let mut fits = BTreeMap::new();
    for (name, entry) in contents {
        let file = dir.join(name);
        let id = file
            .file_stem()
            .and_then(OsStr::to_str)
            .and_then(|stem| ProjectId::parse(stem.to_owned()).ok());
        let Some(id) = id.filter(|_| file.extension() == Some(OsStr::new("toml"))) else {
            tree.problem(
                &file,
                "expected a <project>.toml fit file named by a base62 project ID",
            );
            continue;
        };
        let Some(fit) = tree.toml::<Fit>(&file, Some(entry)) else {
            continue;
        };
        let mut releases = BTreeMap::new();
        for row in fit.release {
            if let Some(build) = row.fits.keys().find(|b| row.does_not_fit.contains_key(*b)) {
                tree.problem(
                    &file,
                    format!(
                        "{} both fits and does not fit {}",
                        row.version,
                        build.as_str()
                    ),
                );
            }
            let version = row.version.clone();
            if releases.insert(version.clone(), row).is_some() {
                tree.problem(&file, format!("duplicate fit row for {version}"));
            }
        }
        fits.insert(id, releases);
    }
    fits
}
