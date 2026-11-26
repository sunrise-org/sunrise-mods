use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sunrise_mod_package::index::stable_tag;
pub use sunrise_mod_package::manifest::JarRef;
use sunrise_release_sign::{Channel, is_build};
use time::{
    Date, OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description,
};
use url::Url;

macro_rules! text_type {
    ($name:ident, $expected:literal, $valid:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        #[allow(dead_code)] // Only identifiers used for paths and lookups need view access.
        impl $name {
            pub fn parse(value: String) -> Result<Self, &'static str> {
                if ($valid)(&value) {
                    Ok(Self(value))
                } else {
                    Err($expected)
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
            }
        }
    };
}

text_type!(
    ProjectId,
    "expected an 8-character base62 project ID",
    |s: &str| {
        // Modrinth: apps/labrinth/src/database/models/ids.rs:29–30, 236.
        s.len() == 8 && s.bytes().all(|b| b.is_ascii_alphanumeric())
    }
);
text_type!(
    Slug,
    "expected a slug using a-z, 0-9, '-' or '_'",
    |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
    }
);
text_type!(
    ModId,
    "expected an in-game ID valid in sunrise-mod.toml",
    sunrise_mod_package::manifest::valid_mod_id
);
text_type!(
    Sha256,
    "expected a 64-digit lowercase hexadecimal SHA-256",
    |s: &str| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
);
text_type!(
    RelativePath,
    "expected a relative path with forward slashes and no dot segments",
    |s: &str| {
        s.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.contains(['\\', ':'])
                && !part.chars().any(char::is_control)
        })
    }
);
text_type!(
    CalendarDate,
    "expected a calendar date in YYYY-MM-DD form",
    |s: &str| {
        Date::parse(
            s,
            format_description!("[year repr:full sign:automatic]-[month]-[day]"),
        )
        .is_ok_and(|date| date.to_string() == s && !s.starts_with('-'))
    }
);
text_type!(
    PublishedAt,
    "expected a UTC time in YYYY-MM-DDTHH:MM:SSZ form",
    |s: &str| OffsetDateTime::parse(s, &Rfc3339).is_ok_and(|time| PublishedAt::text(time) == s)
);

impl PublishedAt {
    fn text(time: OffsetDateTime) -> String {
        time.to_offset(time::UtcOffset::UTC)
            .format(format_description!(
                "[year repr:full]-[month]-[day]T[hour]:[minute]:[second]Z"
            ))
            .unwrap_or_default()
    }

    /// GitHub's publication time, to the second.
    pub fn of(time: OffsetDateTime) -> Result<Self, &'static str> {
        Self::parse(Self::text(time))
    }

    pub fn time(&self) -> OffsetDateTime {
        OffsetDateTime::parse(&self.0, &Rfc3339).expect("validated when parsed")
    }

    pub fn date(&self) -> &str {
        &self.0[..10]
    }
}
text_type!(
    RepositoryName,
    "expected a GitHub owner/repository name",
    github_repository
);

pub fn github_repository(s: &str) -> bool {
    s.split_once('/').is_some_and(|(owner, repo)| {
        github_login(owner)
            && !repo.is_empty()
            && repo != "."
            && repo != ".."
            && repo
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    })
}

fn github_login(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

text_type!(Login, "expected a GitHub login", github_login);
pub const RELEASE_WORKFLOW: &str = "sunrise-org/sunrise-mods/.github/workflows/release.yml";

text_type!(
    WorkflowUse,
    "expected the registry release workflow at a release-v<number> tag",
    |s: &str| {
        s.split_once('@')
            .is_some_and(|(path, tag)| path == RELEASE_WORKFLOW && workflow_tag(tag))
    }
);
text_type!(
    SignerWorkflowRef,
    "expected a signer workflow path at refs/tags/release-v<number>",
    |s: &str| {
        s.split_once('@').is_some_and(|(path, r)| {
            !path.is_empty() && r.strip_prefix("refs/tags/").is_some_and(workflow_tag)
        })
    }
);

fn workflow_tag(tag: &str) -> bool {
    tag.strip_prefix("release-v")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

text_type!(
    Commit,
    "expected a full lowercase game commit SHA",
    sunrise_mod_package::manifest::revision
);

text_type!(
    BuildName,
    "expected a stable Sunrise tag or an exact Nightly/Dev build ID",
    |s: &str| { stable_tag(s).is_some() || Channel::of(s).is_some_and(|ch| is_build(ch, s)) }
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseWorkflow {
    #[serde(rename = "uses")]
    pub uses: WorkflowUse,
}
text_type!(
    HttpsUrl,
    "expected an absolute HTTPS URL without credentials",
    |s: &str| { s.starts_with("https://") && web_url(s) }
);
text_type!(
    Link,
    "expected an absolute HTTP(S) URL without credentials",
    web_url
);
text_type!(Text, "expected nonblank text", |s: &str| !s
    .trim()
    .is_empty());

fn web_url(s: &str) -> bool {
    (s.starts_with("https://") || s.starts_with("http://"))
        && !s.contains('\\')
        && !s.chars().any(|c| c.is_control() || c.is_whitespace())
        && Url::parse(s).is_ok_and(|url| {
            url.has_host() && url.username().is_empty() && url.password().is_none()
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RepositoryId(std::num::NonZeroU64);

impl RepositoryId {
    pub fn get(self) -> u64 {
        self.0.get()
    }

    /// The ID a `registry/repositories/<id>.toml` file is named for, in its one decimal form.
    pub fn of_file(name: &str) -> Option<Self> {
        let id = name.strip_suffix(".toml")?.parse().ok().map(Self)?;
        (format!("{}.toml", id.get()) == name).then_some(id)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PackagePath {
    Root(RootPath),
    Folder(RelativePath),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum RootPath {
    #[serde(rename = ".")]
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Mod,
    NativeMod,
    Modpack,
    Resourcepack,
    Shader,
    Shaderpack,
    Datapack,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub name: RepositoryName,
    pub package: Vec<Package>,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub path: PackagePath,
    pub project: ProjectId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Categories {
    pub category: Vec<Category>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: Slug,
    pub icon: Slug,
    pub kinds: Vec<Kind>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: ProjectId,
    pub slug: Slug,
    #[serde(rename = "mod")]
    pub mod_id: ModId,
    pub kind: Kind,
    pub owners: Vec<Login>,
    pub repository: RepositoryId,
    pub path: PackagePath,
    pub workflow: Text,
    #[serde(rename = "registered")]
    pub _registered: CalendarDate,
    /// The watcher proposes no release of a paused project. Its registration, ID and publisher
    /// pin stay, and releases already in the registry stay listed.
    #[serde(default)]
    pub paused: bool,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub version: Version,
    /// What the release's package is. Every release of a project has the project's `kind`.
    pub kind: Kind,
    pub tag: Text,
    pub published: PublishedAt,
    pub sunrise: BuildName,
    pub workflow: SignerWorkflowRef,
    /// The repository the release was signed from: the project's when the release was proposed.
    /// A later move of the project to another repository leaves it as it is.
    pub repository: RepositoryId,
    pub file: Asset,
    pub page: Asset,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// The original Java jars the package's manifest lists as `[[jar]]`, in manifest order. The
    /// registry stores only these references, never a jar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jars: Vec<JarRef>,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub url: HttpsUrl,
    pub size: u64,
    pub sha256: Sha256,
    pub attestation: HttpsUrl,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    #[serde(rename = "mod")]
    pub mod_id: ModId,
    /// The manifest's requirement on the provider's version.
    pub version: VersionReq,
    pub project: Option<ProjectId>,
    pub required: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub name: Text,
    pub summary: Text,
    pub description: Text,
    pub authors: Vec<Text>,
    pub license: Text,
    pub homepage: Option<Link>,
    pub issues: Option<Link>,
    pub source: Option<Link>,
    pub icon: RelativePath,
    #[serde(default)]
    pub categories: Vec<Slug>,
    pub screenshots: Vec<Screenshot>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Screenshot {
    pub path: RelativePath,
    pub caption: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fit {
    #[serde(default)]
    pub release: Vec<FitRelease>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FitRelease {
    pub version: Version,
    /// The release's package the verdicts were judged from.
    pub package: Sha256,
    /// [`crate::fit::RULES`] when they were judged.
    pub rules: u32,
    /// Each build the release fits, with the digest of the surface it was judged against.
    #[serde(default)]
    pub fits: BTreeMap<BuildName, Sha256>,
    #[serde(default)]
    pub does_not_fit: BTreeMap<BuildName, Sha256>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revocations {
    #[serde(default)]
    pub revoked: Vec<Revocation>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Revocation {
    pub project: ProjectId,
    pub sha256: Option<Sha256>,
    pub reason: Text,
    pub date: CalendarDate,
}
