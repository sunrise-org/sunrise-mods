use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;
use tempfile::TempDir;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const REPOSITORY: &str = "registry/repositories/123456789.toml";
const REPOSITORY_2: &str = "registry/repositories/987654321.toml";
const WORKFLOW: &str = "registry/release-workflow.toml";
const CATEGORIES: &str = "registry/categories.toml";
const REVOKED: &str = "registry/revoked.toml";
const FIT: &str = "registry/fit/Ab3dE9xQ.toml";
const PROJECT: &str = "registry/projects/Ab3dE9xQ/project.toml";
const PROJECT_2: &str = "registry/projects/Zz91kLmN/project.toml";
const PAGE: &str = "registry/projects/Ab3dE9xQ/page/page.toml";
const ICON: &str = "registry/projects/Ab3dE9xQ/page/icon.png";
const SCREENSHOT: &str = "registry/projects/Ab3dE9xQ/page/map.png";
const RELEASE: &str = "registry/projects/Ab3dE9xQ/releases/1.2.0.toml";
const RELEASE_2: &str = "registry/projects/Zz91kLmN/releases/2.0.0-beta.1.toml";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-ports")
}

fn copy(source: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn checkout() -> TempDir {
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target");
    fs::create_dir_all(&target).unwrap();
    let checkout = tempfile::tempdir_in(target).unwrap();
    copy(
        &fixture().join("registry"),
        &checkout.path().join("registry"),
    )
    .unwrap();
    checkout
}

fn fit_checkout() -> TempDir {
    let checkout = checkout();
    copy(
        &fixture().join("generated-fit"),
        &checkout.path().join("registry/fit"),
    )
    .unwrap();
    checkout
}

fn run(args: &[&str], checkout: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_registry-bot"))
        .current_dir(checkout)
        .args(args)
        .output()
        .unwrap()
}

fn rejected(checkout: &Path, path: &str, expected: &str) {
    let generated = path.starts_with("registry/fit/");
    let result = if generated {
        run(
            &[
                "index",
                ".",
                "--serial",
                "41",
                "--out",
                "output",
                "--generated-fit",
            ],
            checkout,
        )
    } else {
        run(&["check", "."], checkout)
    };
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(result.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.lines().any(|line| (generated && line.contains(path)
            || line.starts_with(&format!("{path}: ")))
            && line.contains(expected)),
        "expected {path}: ... {expected}, got:\n{stderr}"
    );
}

fn replace(checkout: &Path, file: &str, old: &str, new: &str) {
    let path = checkout.join(file);
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(old),
        "fixture {file} does not contain {old:?}"
    );
    fs::write(path, text.replacen(old, new, 1)).unwrap();
}

macro_rules! change {
    ($name:ident, $file:expr, $old:expr, $new:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let temp = if $file == FIT {
                fit_checkout()
            } else {
                checkout()
            };
            replace(temp.path(), $file, $old, $new);
            rejected(temp.path(), $file, $expected);
        }
    };
}

change!(
    unknown_repository_field,
    REPOSITORY,
    "name =",
    "extra = true\nname =",
    "unknown field"
);
change!(
    unknown_package_field,
    REPOSITORY,
    "path = \".\"",
    "extra = true\npath = \".\"",
    "unknown field"
);
change!(
    unknown_category_field,
    CATEGORIES,
    "id =",
    "extra = true\nid =",
    "unknown field"
);
change!(
    unknown_project_field,
    PROJECT,
    "id =",
    "extra = true\nid =",
    "unknown field"
);
change!(
    unknown_release_field,
    RELEASE,
    "version =",
    "extra = true\nversion =",
    "unknown field"
);
change!(
    unknown_asset_field,
    RELEASE,
    "[page]",
    "extra = true\n[page]",
    "unknown field"
);
change!(
    unknown_dependency_field,
    RELEASE,
    "required =",
    "extra = true\nrequired =",
    "unknown field"
);
change!(
    unknown_page_field,
    PAGE,
    "name =",
    "tags = []\nname =",
    "unknown field"
);
change!(
    unknown_screenshot_field,
    PAGE,
    "caption =",
    "extra = true\ncaption =",
    "unknown field"
);
change!(
    unknown_fit_container_field,
    FIT,
    "[[release]]",
    "extra = true\n[[release]]",
    "unknown field"
);
change!(
    unknown_revocations_container_field,
    REVOKED,
    "[[revoked]]",
    "extra = true\n[[revoked]]",
    "unknown field"
);
change!(
    unknown_fit_field,
    FIT,
    "rules =",
    "extra = true\nrules =",
    "unknown field"
);
change!(
    unknown_revocation_field,
    REVOKED,
    "reason =",
    "extra = true\nreason =",
    "unknown field"
);
change!(
    unknown_workflow_field,
    WORKFLOW,
    "uses =",
    "extra = true\nuses =",
    "unknown field"
);

change!(
    bad_project_id_length,
    PROJECT,
    "Ab3dE9xQ",
    "short",
    "8-character base62"
);
change!(
    bad_project_id_character,
    PROJECT,
    "Ab3dE9xQ",
    "Ab3dE9_Q",
    "8-character base62"
);
change!(
    bad_slug,
    PROJECT,
    "slug = \"wayfinder\"",
    "slug = \"UPPER\"",
    "expected a slug"
);
change!(
    bad_mod_id,
    PROJECT,
    "mod = \"wayfinder\"",
    "mod = \"Way-Finder\"",
    "expected an in-game ID"
);
change!(
    bad_kind,
    PROJECT,
    "native-mod",
    "collection",
    "unknown variant"
);
change!(bad_repo_id, PROJECT, "123456789", "0", "nonzero");
change!(
    bad_repository_name,
    REPOSITORY,
    "example/wayfinder",
    "https://github.com/example/wayfinder",
    "owner/repository"
);
change!(
    bad_package_path,
    REPOSITORY_2,
    "mods/wayfinder",
    "mods/../wayfinder",
    "PackagePath"
);
change!(
    absolute_package_path,
    REPOSITORY_2,
    "mods/wayfinder",
    "/wayfinder",
    "PackagePath"
);
change!(
    bad_registered_date,
    PROJECT,
    "2026-11-22",
    "2026-02-30",
    "calendar date"
);
change!(
    bad_published_time,
    RELEASE,
    "2026-11-23T12:00:00Z",
    "2026-11-23",
    "UTC time"
);
change!(
    bad_revocation_date,
    REVOKED,
    "2026-11-25",
    "today",
    "calendar date"
);
change!(
    bad_version,
    RELEASE,
    "version = \"1.2.0\"",
    "version = \"latest\"",
    "version"
);
change!(
    bad_workflow_tag,
    WORKFLOW,
    "release-v1",
    "main",
    "release-v<number>"
);
change!(
    bad_workflow_repository,
    WORKFLOW,
    "sunrise-org/sunrise-mods",
    "other/sunrise-mods",
    "registry release workflow"
);
change!(
    bad_workflow_file,
    WORKFLOW,
    "release.yml",
    "different.yml",
    "registry release workflow"
);
change!(
    project_workflow_mismatch,
    PROJECT,
    "release.yml",
    "different.yml",
    "workflow must match"
);
change!(
    bad_root_tag,
    RELEASE,
    "tag = \"v1.2.0\"",
    "tag = \"wayfinder/v1.2.0\"",
    "tag must be v1.2.0"
);
change!(
    bad_subfolder_tag,
    RELEASE_2,
    "tag = \"wayfinder/v2.0.0-beta.1\"",
    "tag = \"v2.0.0-beta.1\"",
    "tag must be wayfinder/v2.0.0-beta.1"
);
#[test]
fn release_requires_its_exact_versioned_signer_workflow() {
    const SIGNER: &str =
        "sunrise-org/sunrise-mods/.github/workflows/release.yml@refs/tags/release-v1";
    for (signer, expected) in [
        (None, "missing field `workflow`"),
        (
            Some("sunrise-org/sunrise-mods/.github/workflows/release.yml@refs/heads/release-v1"),
            "expected a signer workflow path",
        ),
        (
            Some("sunrise-org/sunrise-mods/.github/workflows/release.yml@release-v1"),
            "expected a signer workflow path",
        ),
        (
            Some("sunrise-org/sunrise-mods/.github/workflows/other.yml@refs/tags/release-v1"),
            "signer workflow path must match",
        ),
        (
            Some("sunrise-org/sunrise-mods/.github/workflows/release.yml@refs/tags/release-v"),
            "expected a signer workflow path",
        ),
    ] {
        let temp = checkout();
        replace(
            temp.path(),
            RELEASE,
            &format!("workflow = \"{SIGNER}\"\n"),
            &signer.map_or_else(String::new, |s| format!("workflow = \"{s}\"\n")),
        );
        rejected(temp.path(), RELEASE, expected);
    }
    let temp = checkout();
    replace(temp.path(), WORKFLOW, "release-v1", "release-v2");
    assert_eq!(
        index(temp.path())["projects"][0]["releases"][0]["workflow"],
        SIGNER
    );
}

#[test]
fn release_row_names_the_repository_it_was_signed_from_whatever_the_project_names_now() {
    const ROW: &str = "repository = 123456789\n";
    // The project has since moved: no repository file lists 555 any more.
    let temp = checkout();
    replace(temp.path(), RELEASE, ROW, "repository = 555\n");
    assert_eq!(
        index(temp.path())["projects"][0]["releases"][0]["repository"],
        555
    );
    for (row, expected) in [
        ("", "missing field `repository`"),
        ("repository = 0\n", "invalid value"),
        ("repository = \"555\"\n", "invalid type"),
    ] {
        let temp = checkout();
        replace(temp.path(), RELEASE, ROW, row);
        rejected(temp.path(), RELEASE, expected);
    }
}

#[test]
fn release_assets_match_github_tag_and_asset_names_even_after_rename() {
    for (old, new, field) in [
        (
            "https://github.com/example",
            "https://github.com.example.org/example",
            "file.url",
        ),
        ("/download/v1.2.0/", "/download/v1.2.1/", "file.url"),
        (
            "wayfinder-1.2.0.smod.sigstore.json",
            "wayfinder-1.2.0.sigstore.json",
            "file.attestation",
        ),
        (
            "wayfinder-1.2.0.page.tar\"",
            "other-1.2.0.page.tar\"",
            "page.url",
        ),
        (
            "wayfinder-1.2.0.page.tar.sigstore.json\"",
            "wayfinder-1.2.0.page.tar.sigstore.json?q=x\"",
            "page.attestation",
        ),
    ] {
        let temp = checkout();
        replace(temp.path(), RELEASE, old, new);
        rejected(
            temp.path(),
            RELEASE,
            &format!("{field} must be a GitHub release download HTTPS URL"),
        );
    }
    let temp = checkout();
    replace(
        temp.path(),
        REPOSITORY,
        "example/wayfinder",
        "example/renamed",
    );
    assert_eq!(
        index(temp.path())["projects"][0]["repository"]["name"],
        "example/renamed"
    );
}

#[test]
fn release_and_fit_require_exact_sunrise_build_names() {
    for (file, from, to) in [
        (RELEASE, "26.3-0.1.0", "latest"),
        (RELEASE, "26.3-0.1.0", "26.3-0.1.0-beta.1"),
        (RELEASE_2, "nightly-2026-11-23-abcdef0", "nightly"),
        (FIT, "26.3-0.1.1", "not-a-build"),
        (FIT, "26.3-0.1.1", "dev-2026-11-23-notasha"),
    ] {
        let temp = if file == FIT {
            fit_checkout()
        } else {
            checkout()
        };
        replace(temp.path(), file, from, to);
        rejected(
            temp.path(),
            file,
            "expected a stable Sunrise tag or an exact Nightly/Dev build ID",
        );
    }
}

#[test]
fn mod_ids_use_the_manifest_predicate() {
    for (file, from, id) in [
        (PROJECT, "wayfinder", "a".repeat(65)),
        (RELEASE, "map_markers", "con".into()),
    ] {
        let temp = checkout();
        replace(
            temp.path(),
            file,
            &format!("mod = \"{from}\""),
            &format!("mod = \"{id}\""),
        );
        rejected(temp.path(), file, "expected an in-game ID");
    }
}

change!(bad_asset_hash, RELEASE, &"a".repeat(64), "f", "SHA-256");
change!(
    missing_asset_attestation,
    RELEASE,
    "attestation = \"https://github.com/example/wayfinder/releases/download/v1.2.0/wayfinder-1.2.0.smod.sigstore.json\"",
    "",
    "missing field `attestation`"
);
change!(
    missing_page_attestation,
    RELEASE,
    "attestation = \"https://github.com/example/wayfinder/releases/download/v1.2.0/wayfinder-1.2.0.page.tar.sigstore.json\"",
    "",
    "missing field `attestation`"
);
change!(
    bad_attestation_url,
    RELEASE,
    "attestation = \"https://",
    "attestation = \"http://",
    "HTTPS URL"
);
change!(
    bad_asset_url,
    RELEASE,
    "url = \"https://",
    "url = \"file://",
    "HTTPS URL"
);
change!(
    bad_page_link,
    PAGE,
    "https://example.org/wayfinder",
    "file:///wayfinder",
    "HTTP(S) URL"
);

#[test]
fn repository_file_is_named_by_its_one_decimal_id() {
    for name in [
        "0987654321.toml",
        "987654321",
        "port-collection.toml",
        "0.toml",
    ] {
        let temp = checkout();
        let renamed = format!("registry/repositories/{name}");
        fs::rename(temp.path().join(REPOSITORY_2), temp.path().join(&renamed)).unwrap();
        rejected(temp.path(), &renamed, "<numeric repository ID>.toml");
    }
}

change!(
    project_repository_must_be_the_file_that_lists_it,
    PROJECT_2,
    "987654321",
    "123456789",
    "repository ID does not match"
);
change!(
    duplicate_package_path,
    REPOSITORY,
    "project = \"Ab3dE9xQ\"",
    "project = \"Ab3dE9xQ\"\n\n[[package]]\npath = \".\"\nproject = \"Zz91kLmN\"",
    "duplicate package path"
);
change!(
    duplicate_registered_project,
    REPOSITORY_2,
    "Zz91kLmN",
    "Ab3dE9xQ",
    "duplicate registered project ID"
);
change!(
    reserved_slug,
    PROJECT,
    "slug = \"wayfinder\"",
    "slug = \"registry-page\"",
    "reserved slug registry-page"
);
change!(
    duplicate_slug,
    PROJECT_2,
    "wayfinder-alternate",
    "wayfinder",
    "duplicate slug"
);
change!(
    wrong_project_folder,
    PROJECT,
    "Ab3dE9xQ",
    "Ab3dE9xZ",
    "match the directory name"
);
change!(
    project_repo_mismatch,
    PROJECT,
    "123456789",
    "123456788",
    "repository ID does not match"
);
change!(
    project_path_mismatch,
    PROJECT,
    "path = \".\"",
    "path = \"subfolder\"",
    "package path does not match"
);
change!(
    release_filename_mismatch,
    RELEASE,
    "version = \"1.2.0\"",
    "version = \"1.2.1\"",
    "version must match its filename"
);
change!(
    unknown_dependency_project,
    RELEASE_2,
    "Ab3dE9xQ",
    "Unknown0",
    "dependency names unregistered project"
);
change!(
    wrong_dependency_project,
    RELEASE_2,
    "mod = \"wayfinder\"",
    "mod = \"other\"",
    "does not provide other"
);
change!(
    duplicate_category,
    CATEGORIES,
    "id = \"utility\"",
    "id = \"adventure\"",
    "duplicate category adventure"
);
change!(
    unknown_page_category,
    PAGE,
    "\"utility\"",
    "\"missing\"",
    "unknown category missing"
);
change!(
    duplicate_page_category,
    PAGE,
    "\"utility\"",
    "\"adventure\"",
    "duplicate category adventure"
);
change!(
    contradictory_fit_verdicts,
    FIT,
    "dev-2026-11-23-abcdef0",
    "\"26.3-0.1.1\"",
    "both fits and does not fit"
);
change!(
    duplicate_fit_release,
    FIT,
    "[[release]]",
    "[[release]]\nversion = \"1.2.0\"\npackage = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\nrules = 2\n\n[[release]]",
    "duplicate fit row"
);
change!(
    duplicate_takedown,
    REVOKED,
    "[[revoked]]",
    "[[revoked]]\nproject = \"Retired0\"\nreason = \"duplicate\"\ndate = \"2026-11-25\"\n\n[[revoked]]",
    "duplicate takedown"
);
change!(
    bad_revocation_hash,
    REVOKED,
    &"c".repeat(64),
    &"C".repeat(64),
    "SHA-256"
);
change!(
    too_many_categories,
    PAGE,
    "[\"adventure\", \"utility\"]",
    "[\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\", \"h\", \"i\"]",
    "at most 8 categories"
);
change!(
    short_summary,
    PAGE,
    "Find your way using the new map display.",
    &format!("  {}  ", "🗺".repeat(24)),
    "at least 25 characters"
);
change!(
    short_description,
    PAGE,
    "Wayfinder adds a map display that follows you as you explore. Mark landmarks, name spots you want to revisit, and hide the display when you no longer need it.",
    &format!("  {}  ", "é".repeat(124)),
    "at least 125 characters"
);
change!(
    missing_page_name,
    PAGE,
    "name = \"Wayfinder\"\n",
    "",
    "missing field `name`"
);
change!(
    missing_page_authors,
    PAGE,
    "authors = [\"Example\"]\n",
    "",
    "missing field `authors`"
);
change!(
    missing_page_license,
    PAGE,
    "license = \"MIT\"\n",
    "",
    "missing field `license`"
);
change!(
    missing_page_icon,
    PAGE,
    "icon = \"icon.png\"\n",
    "",
    "missing field `icon`"
);
change!(
    missing_page_summary,
    PAGE,
    "summary = \"Find your way using the new map display.\"\n",
    "",
    "missing field `summary`"
);
change!(
    missing_page_description,
    PAGE,
    "description = \"Wayfinder adds a map display that follows you as you explore. Mark landmarks, name spots you want to revisit, and hide the display when you no longer need it.\"\n",
    "",
    "missing field `description`"
);
change!(
    empty_screenshots,
    PAGE,
    "[[screenshots]]\npath = \"map.png\"\ncaption = \"The map on the title screen.\"",
    "screenshots = []",
    "at least one screenshot"
);
change!(
    missing_caption,
    PAGE,
    "caption = \"The map on the title screen.\"",
    "",
    "missing field `caption`"
);
change!(
    parent_image_path,
    PAGE,
    "map.png",
    "../map.png",
    "relative path"
);
change!(
    absolute_image_path,
    PAGE,
    "map.png",
    "/map.png",
    "relative path"
);
change!(
    windows_image_path,
    PAGE,
    "map.png",
    "C:/map.png",
    "relative path"
);
change!(
    backslash_image_path,
    PAGE,
    "map.png",
    "..\\\\map.png",
    "relative path"
);
change!(
    missing_screenshot,
    PAGE,
    "map.png",
    "missing.png",
    "not a regular file"
);
change!(
    missing_icon_file,
    PAGE,
    "icon.png",
    "missing.png",
    "not a regular file"
);

#[test]
fn category_must_apply_to_kind() {
    let temp = checkout();
    replace(
        temp.path(),
        PROJECT,
        "kind = \"native-mod\"",
        "kind = \"shader\"",
    );
    rejected(temp.path(), PAGE, "does not apply to this project kind");
}

#[test]
fn only_subfolder_packages_in_one_repo_collide_on_the_same_in_game_id() {
    let temp = checkout();
    fs::remove_file(temp.path().join(REPOSITORY_2)).unwrap();
    replace(
        temp.path(),
        REPOSITORY,
        "project = \"Ab3dE9xQ\"",
        "project = \"Ab3dE9xQ\"\n\n[[package]]\npath = \"mods/wayfinder\"\nproject = \"Zz91kLmN\"",
    );
    replace(temp.path(), PROJECT_2, "987654321", "123456789");
    let count = index(temp.path())["projects"].as_array().unwrap().len();
    assert_eq!(count, 2);

    for file in [REPOSITORY, PROJECT] {
        replace(
            temp.path(),
            file,
            "path = \".\"",
            "path = \"mods/root-wayfinder\"",
        );
    }
    let file = temp.path().join(RELEASE);
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        file,
        text.replace("tag = \"v1.2.0\"", "tag = \"wayfinder/v1.2.0\"")
            .replace("/download/v1.2.0/", "/download/wayfinder%2Fv1.2.0/"),
    )
    .unwrap();
    rejected(temp.path(), PROJECT_2, "same in-game ID");
}

#[test]
fn listed_project_must_have_metadata() {
    let temp = checkout();
    fs::remove_dir_all(temp.path().join(PROJECT).parent().unwrap()).unwrap();
    rejected(temp.path(), REPOSITORY, "Ab3dE9xQ has no project.toml");
}

#[test]
fn project_must_be_listed_by_a_repository_file() {
    let temp = checkout();
    fs::remove_file(temp.path().join(REPOSITORY)).unwrap();
    rejected(
        temp.path(),
        PROJECT,
        "not listed in a registry/repositories file",
    );
}

#[test]
fn every_release_has_its_projects_kind() {
    let temp = checkout();
    replace(
        temp.path(),
        "registry/projects/Zz91kLmN/project.toml",
        "kind = \"mod\"",
        "kind = \"native-mod\"",
    );
    rejected(
        temp.path(),
        "registry/projects/Zz91kLmN/releases/2.0.0-beta.1.toml",
        "release kind does not match the project kind",
    );
}

#[test]
fn release_must_have_page() {
    let temp = checkout();
    fs::remove_dir_all(temp.path().join(PAGE).parent().unwrap()).unwrap();
    rejected(
        temp.path(),
        "registry/projects/Ab3dE9xQ/page",
        "expected a page directory",
    );
}

#[test]
fn unexpected_entries_are_rejected() {
    let temp = checkout();
    fs::write(temp.path().join("registry/extra.toml"), "").unwrap();
    rejected(
        temp.path(),
        "registry/extra.toml",
        "unexpected registry entry",
    );
}

/// A verdict counts only for the package, surface and rules it was judged from. A release
/// without a current one on an offered build is published with that build in neither list.
#[test]
fn a_cell_with_no_current_verdict_is_left_out_of_the_index_not_refused_or_guessed() {
    let judged = |checkout: &Path| {
        let release = &index(checkout)["projects"][0]["releases"][0];
        ["fits", "does_not_fit"]
            .iter()
            .flat_map(|list| release[list].as_array().into_iter().flatten().cloned())
            .collect::<Vec<_>>()
    };
    let every = judged(fit_checkout().path());
    assert_eq!(every.len(), 3);
    for (file, old, new) in [
        (FIT, "rules = 3", "rules = 2"),
        (
            FIT,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        ),
        (
            FIT,
            "\"26.3-0.1.1\" = \"2222222222222222222222222222222222222222222222222222222222222222\"",
            "\"26.3-0.1.1\" = \"9999999999999999999999999999999999999999999999999999999999999999\"",
        ),
        (FIT, "dev-2026-11-23-abcdef0", "dev-2026-11-22-abcdef0"),
        (
            RELEASE,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        ),
    ] {
        let temp = fit_checkout();
        replace(temp.path(), file, old, new);
        let left = judged(temp.path());
        assert!(left.len() < every.len(), "{file}: {old}");
        assert!(left.iter().all(|build| every.contains(build)));
    }
}

#[test]
fn verdicts_for_a_build_no_longer_offered_are_ignored() {
    let temp = fit_checkout();
    let builds = temp.path().join("registry/fit/builds.json");
    let mut offered: Value = serde_json::from_slice(&fs::read(&builds).unwrap()).unwrap();
    offered.as_array_mut().unwrap().remove(1);
    fs::write(builds, offered.to_string()).unwrap();
    let release = &index(temp.path())["projects"][0]["releases"][0];
    assert_eq!(release["fits"], serde_json::json!(["26.3-0.1.0"]));
    assert_eq!(
        release["does_not_fit"],
        serde_json::json!(["dev-2026-11-23-abcdef0"])
    );
}

#[test]
fn release_filename_must_be_a_version() {
    let temp = checkout();
    let dest = "registry/projects/Ab3dE9xQ/releases/latest.toml";
    fs::rename(temp.path().join(RELEASE), temp.path().join(dest)).unwrap();
    rejected(temp.path(), dest, "named by a semantic version");
}

#[test]
fn screenshot_requires_a_png_or_jpeg_signature() {
    let temp = checkout();
    fs::write(temp.path().join(SCREENSHOT), b"not an image").unwrap();
    rejected(temp.path(), SCREENSHOT, "PNG or JPEG file signature");
}

#[test]
fn referenced_image_extension_must_match_its_format() {
    for (image, from, to) in [
        (SCREENSHOT, "map.png", "map.jpg"),
        (SCREENSHOT, "map.png", "map.html"),
        (ICON, "icon.png", "icon.jpg"),
    ] {
        let temp = checkout();
        let renamed = image.replace(from, to);
        fs::rename(temp.path().join(image), temp.path().join(&renamed)).unwrap();
        replace(temp.path(), PAGE, from, to);
        rejected(temp.path(), &renamed, "extension must match");
    }
}

#[test]
fn nested_unreferenced_page_files_are_rejected() {
    let temp = checkout();
    let file = "registry/projects/Ab3dE9xQ/page/gallery/extra.png";
    fs::create_dir_all(temp.path().join(file).parent().unwrap()).unwrap();
    fs::copy(temp.path().join(SCREENSHOT), temp.path().join(file)).unwrap();
    rejected(
        temp.path(),
        file,
        "page file is not referenced by page.toml",
    );
}

#[test]
fn screenshot_gallery_size_does_not_replace_the_package_icon_limit() {
    let temp = checkout();
    let f = fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join(SCREENSHOT))
        .unwrap();
    f.set_len(5 * 1024 * 1024).unwrap();
    assert!(run(&["check", "."], temp.path()).status.success());
    f.set_len(5 * 1024 * 1024 + 1).unwrap();
    rejected(temp.path(), SCREENSHOT, "screenshot exceeds 5 MiB");
    f.set_len(5 * 1024 * 1024).unwrap();

    fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join(ICON))
        .unwrap()
        .set_len(256 * 1024 + 1)
        .unwrap();
    rejected(temp.path(), ICON, "icon.png exceeds 256 KiB");
}

#[cfg(unix)]
#[test]
fn registry_root_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let temp = checkout();
    let other = checkout();
    let registry = temp.path().join("registry");
    fs::remove_dir_all(&registry).unwrap();
    symlink(other.path().join("registry"), registry).unwrap();
    rejected(temp.path(), "registry", "not a symlink");
}

#[cfg(unix)]
#[test]
fn symlink_toml_is_not_followed() {
    use std::os::unix::fs::symlink;
    let temp = checkout();
    let other = checkout();
    let repository = temp.path().join(REPOSITORY);
    fs::remove_file(&repository).unwrap();
    symlink(other.path().join(REPOSITORY), repository).unwrap();
    rejected(temp.path(), REPOSITORY, "not a symlink");
}

#[cfg(unix)]
#[test]
fn image_directory_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = "registry/projects/Zz91kLmN/page/.github";
    let temp = checkout();
    let other = checkout();
    fs::remove_dir_all(temp.path().join(dir)).unwrap();
    symlink(other.path().join(dir), temp.path().join(dir)).unwrap();
    rejected(temp.path(), dir, "not a symlink");
}

#[cfg(unix)]
#[test]
fn image_file_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let temp = checkout();
    let other = checkout();
    fs::remove_file(temp.path().join(SCREENSHOT)).unwrap();
    symlink(other.path().join(SCREENSHOT), temp.path().join(SCREENSHOT)).unwrap();
    rejected(temp.path(), SCREENSHOT, "not a symlink");
}

#[test]
fn multiple_problems_are_each_reported_on_one_line() {
    let temp = checkout();
    replace(temp.path(), PAGE, "name =", "test = true\nname =");
    replace(temp.path(), WORKFLOW, "uses =", "test = true\nuses =");
    let output = run(&["check", "."], temp.path());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr.lines().count(), 2, "{stderr}");
    assert!(
        stderr
            .lines()
            .all(|line| line.starts_with("registry/") && line.contains("unknown field"))
    );
}

#[test]
fn diagnostics_escape_both_workflow_command_markers() {
    let temp = checkout();
    replace(
        temp.path(),
        PAGE,
        "name =",
        "\"##[warning]::warning:::\" = true\nname =",
    );
    let output = run(&["check", "."], temp.path());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.contains("%23%23%5Bwarning]%3A%3Awarning%3A%3A:"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("##[") && !stderr.contains("::"),
        "{stderr}"
    );
}

fn index(checkout: &Path) -> Value {
    let mut args = vec!["index", ".", "--serial", "41", "--out", "output"];
    if checkout.join("registry/fit").exists() {
        args.push("--generated-fit");
    }
    let result = run(&args, checkout);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&fs::read(checkout.join("output/index.json")).unwrap()).unwrap()
}

#[test]
fn full_index_matches_golden_with_both_providers() {
    let temp = fit_checkout();
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture().join("index.json")).unwrap()).unwrap();
    let mut actual = index(temp.path());
    let generated = OffsetDateTime::parse(actual["generated"].as_str().unwrap(), &Rfc3339).unwrap();
    assert!(generated.offset().is_utc());
    actual["generated"] = expected["generated"].clone();
    assert_eq!(actual, expected);
}

#[test]
fn published_image_extension_is_canonical_for_jpeg_content() {
    let temp = checkout();
    let source = temp.path().join(SCREENSHOT);
    let jpeg = [0xff, 0xd8, 0xff, 0xd9];
    fs::remove_file(&source).unwrap();
    fs::write(source.with_extension("JPEG"), jpeg).unwrap();
    replace(temp.path(), PAGE, "map.png", "map.JPEG");
    let catalogue = index(temp.path());
    let hash = sunrise_release_sign::sha256_hex(&jpeg);
    let file = format!("pages/Ab3dE9xQ/{hash}.jpg");
    assert_eq!(catalogue["projects"][0]["screenshots"][0]["path"], file);
    assert_eq!(
        fs::read(temp.path().join("output").join(file)).unwrap(),
        jpeg
    );
}

#[test]
fn project_without_a_release_does_not_need_a_page_and_is_not_listed() {
    let temp = fit_checkout();
    fs::remove_file(temp.path().join(RELEASE)).unwrap();
    fs::remove_file(temp.path().join(FIT)).unwrap();
    fs::remove_dir_all(temp.path().join(PAGE).parent().unwrap()).unwrap();
    let value = index(temp.path());
    assert_eq!(value["projects"].as_array().unwrap().len(), 1);
    assert_eq!(value["projects"][0]["id"], "Zz91kLmN");
}

#[test]
fn invalid_checkout_produces_no_index() {
    let temp = checkout();
    fs::remove_file(temp.path().join(ICON)).unwrap();
    let output = run(
        &["index", ".", "--serial", "41", "--out", "output"],
        temp.path(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!temp.path().join("output").exists());
}

#[test]
fn malformed_serial_is_a_usage_error() {
    let temp = checkout();
    let output = run(
        &["index", ".", "--serial", "-1", "--out", "output"],
        temp.path(),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(!temp.path().join("output").exists());
}

#[test]
fn shared_two_provider_fixture_passes_checkout_checker_without_generated_fit() {
    let result = run(&["check", "."], &fixture());
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn committed_fit_directory_is_refused_by_checkout_checker() {
    let temp = fit_checkout();
    rejected(
        temp.path(),
        "registry/fit",
        "fit is generated during publishing and must not be committed",
    );
}

const COLLECTION: &str = "registry/collections/Cc77aaQ1/collection.toml";

/// The fixture with both projects under one account and a collection of the two.
fn collection_checkout() -> TempDir {
    let temp = fit_checkout();
    replace(temp.path(), REPOSITORY_2, "other/", "example/");
    let dir = temp.path().join(COLLECTION).parent().unwrap().to_owned();
    fs::create_dir_all(&dir).unwrap();
    fs::copy(temp.path().join(ICON), dir.join("icon.png")).unwrap();
    fs::write(
        temp.path().join(COLLECTION),
        "id = \"Cc77aaQ1\"\nname = \"Wayfinding\"\nsummary = \"Both ports.\"\n\n[[member]]\nproject = \"Ab3dE9xQ\"\n\n[[member]]\nproject = \"Zz91kLmN\"\ndefault = false\n",
    )
    .unwrap();
    temp
}

macro_rules! collection_change {
    ($name:ident, $file:expr, $old:expr, $new:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let temp = collection_checkout();
            fs::remove_dir_all(temp.path().join("registry/fit")).unwrap();
            replace(temp.path(), $file, $old, $new);
            rejected(temp.path(), COLLECTION, $expected);
        }
    };
}

collection_change!(
    collection_of_two_publishers,
    REPOSITORY_2,
    "example/",
    "other/",
    "member Zz91kLmN is published by other, not by example"
);
collection_change!(
    collection_id_differs_from_directory,
    COLLECTION,
    "Cc77aaQ1",
    "Cc77aaQ2",
    "collection ID must match the directory name"
);
collection_change!(
    collection_with_nothing_selected,
    COLLECTION,
    "project = \"Ab3dE9xQ\"",
    "project = \"Ab3dE9xQ\"\ndefault = false",
    "at least one member must be selected by default"
);
collection_change!(
    collection_member_twice,
    COLLECTION,
    "Zz91kLmN",
    "Ab3dE9xQ",
    "duplicate member Ab3dE9xQ"
);
collection_change!(
    collection_member_unregistered,
    COLLECTION,
    "Zz91kLmN",
    "Nope0000",
    "member Nope0000 is not a registered project"
);
collection_change!(
    collection_of_one,
    COLLECTION,
    "[[member]]\nproject = \"Zz91kLmN\"\ndefault = false\n",
    "",
    "a collection needs at least two members"
);
collection_change!(
    collection_unknown_field,
    COLLECTION,
    "name =",
    "owners = []\nname =",
    "unknown field"
);

#[test]
fn collection_is_published_with_its_publisher_icon_and_listed_members() {
    let temp = collection_checkout();
    let value = index(temp.path());
    let icon = fs::read(temp.path().join(ICON)).unwrap();
    let path = format!(
        "pages/Cc77aaQ1/{}.png",
        sunrise_release_sign::sha256_hex(&icon)
    );
    assert_eq!(
        value["collections"],
        serde_json::json!([{
            "id": "Cc77aaQ1",
            "name": "Wayfinding",
            "summary": "Both ports.",
            "description": "",
            "publisher": "example",
            "icon": path,
            "members": [
                { "project": "Ab3dE9xQ", "default": true },
                { "project": "Zz91kLmN", "default": false },
            ],
        }])
    );
    assert_eq!(
        fs::read(temp.path().join("output").join(path)).unwrap(),
        icon
    );
}

#[test]
fn collection_waits_for_two_released_members() {
    let temp = collection_checkout();
    fs::remove_file(temp.path().join(RELEASE)).unwrap();
    fs::remove_file(temp.path().join(FIT)).unwrap();
    fs::remove_dir_all(temp.path().join(PAGE).parent().unwrap()).unwrap();
    assert_eq!(index(temp.path())["collections"], serde_json::json!([]));
}

#[test]
fn only_an_owner_of_every_member_on_the_base_commit_changes_a_collection() {
    let author = |base: &Path, head: &Path, login: &str| {
        let result = run(
            &[
                "collection-author",
                base.to_str().unwrap(),
                head.to_str().unwrap(),
                login,
            ],
            base,
        );
        (
            result.status.success(),
            String::from_utf8(result.stderr).unwrap(),
        )
    };
    let shared = |temp: &TempDir| {
        fs::remove_dir_all(temp.path().join("registry/fit")).unwrap();
        replace(
            temp.path(),
            PROJECT_2,
            "\"Other\"",
            "\"Other\", \"Example\"",
        );
    };
    // The base has no collection; the head adds one.
    let base = checkout();
    replace(base.path(), REPOSITORY_2, "other/", "example/");
    let head = collection_checkout();
    fs::remove_dir_all(head.path().join("registry/fit")).unwrap();
    let (ok, error) = author(base.path(), head.path(), "example");
    assert!(!ok);
    assert!(
        error.contains("lists project Zz91kLmN, which example does not own on the base commit"),
        "{error}"
    );
    // Ownership granted by the same pull request does not count.
    replace(
        head.path(),
        PROJECT_2,
        "\"Other\"",
        "\"Other\", \"Example\"",
    );
    assert!(!author(base.path(), head.path(), "example").0);
    replace(
        base.path(),
        PROJECT_2,
        "\"Other\"",
        "\"Other\", \"Example\"",
    );
    assert!(author(base.path(), head.path(), "example").0);
    assert!(!author(base.path(), head.path(), "stranger").0);
    // Unchanged collections bind nobody; deleting or editing one binds like adding it.
    let same = collection_checkout();
    shared(&same);
    assert!(author(head.path(), same.path(), "stranger").0);
    assert!(!author(head.path(), base.path(), "stranger").0);
    replace(same.path(), COLLECTION, "Both ports.", "Two ports.");
    assert!(!author(head.path(), same.path(), "stranger").0);
    assert!(author(head.path(), same.path(), "Example").0);
    // The base commit is read only for collections and owners: a release row this checker
    // no longer accepts does not stop the pull request that changes the rows.
    replace(head.path(), RELEASE, "kind = \"native-mod\"\n", "");
    assert!(author(head.path(), same.path(), "Example").0);
    assert!(!author(head.path(), same.path(), "stranger").0);
}

#[test]
fn a_listed_project_keeps_a_release_unless_the_change_pauses_it() {
    let base = checkout();
    let head = checkout();
    let stay_listed = || {
        run(
            &["stay-listed", base.path().to_str().unwrap(), "."],
            head.path(),
        )
    };
    assert!(stay_listed().status.success());
    // The base is read by name only: a base file this checker no longer accepts still compares.
    fs::write(base.path().join(PROJECT), "kind = 7\n").unwrap();
    assert!(stay_listed().status.success());
    // The listed release is removed ahead of the pull request that brings its replacement.
    fs::remove_file(head.path().join(RELEASE)).unwrap();
    let refused = stay_listed();
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success());
    assert!(
        stderr.contains("registry/projects/Ab3dE9xQ: unlisted project"),
        "{stderr}"
    );
    assert!(!stderr.contains("Zz91kLmN"), "{stderr}");
    replace(
        head.path(),
        PROJECT,
        "registered = ",
        "paused = true\nregistered = ",
    );
    assert!(stay_listed().status.success());
}

#[test]
fn a_registration_adds_one_project_and_its_package_and_nothing_else() {
    const NEW: &str = "registry/projects/Nw7pQr2s/project.toml";
    const NEW_REPOSITORY: &str = "registry/repositories/555.toml";
    let base = checkout();
    let registration = |head: &Path| {
        let result = run(&["registration", base.path().to_str().unwrap(), "."], head);
        (
            result.status.success(),
            String::from_utf8(result.stderr).unwrap(),
        )
    };
    // A further package of a listed repository; `block` is its row in the repository's file.
    let further = |block: &str, at: &str| {
        let head = checkout();
        let project = fs::read_to_string(head.path().join(PROJECT_2))
            .unwrap()
            .replace("Zz91kLmN", "Nw7pQr2s")
            .replace("wayfinder-alternate", "lanterns")
            .replace("mod = \"wayfinder\"", "mod = \"lanterns\"")
            .replace("mods/wayfinder", "mods/lanterns");
        fs::create_dir_all(head.path().join(NEW).parent().unwrap()).unwrap();
        fs::write(head.path().join(NEW), project).unwrap();
        let row = "[[package]]\npath = \"mods/lanterns\"\nproject = \"Nw7pQr2s\"\n";
        replace(head.path(), REPOSITORY_2, at, &block.replace("{row}", row));
        head
    };
    let appended = || {
        further(
            "project = \"Zz91kLmN\"\n\n{row}",
            "project = \"Zz91kLmN\"\n",
        )
    };
    assert!(registration(appended().path()).0);
    // Where the row stands in the file is the registry service's business.
    let inserted = further(
        "{row}\n[[package]]\npath = \"mods/wayfinder\"",
        "[[package]]\npath = \"mods/wayfinder\"",
    );
    assert!(registration(inserted.path()).0);

    // The first package of a repository brings the repository's file.
    let first = appended();
    replace(
        first.path(),
        REPOSITORY_2,
        "\n[[package]]\npath = \"mods/lanterns\"\nproject = \"Nw7pQr2s\"\n",
        "",
    );
    replace(
        first.path(),
        NEW,
        "repository = 987654321",
        "repository = 555",
    );
    fs::write(
        first.path().join(NEW_REPOSITORY),
        "name = \"other/lanterns\"\n\n[[package]]\npath = \"mods/lanterns\"\nproject = \"Nw7pQr2s\"\n",
    )
    .unwrap();
    assert!(registration(first.path()).0);

    let refused = |head: &TempDir, expected: &str| {
        let (passed, stderr) = registration(head.path());
        assert!(
            !passed && stderr.contains(expected),
            "expected {expected}, got:\n{stderr}"
        );
    };
    refused(&checkout(), "adds exactly one project; this change adds 0");
    let renamed = appended();
    replace(
        renamed.path(),
        REPOSITORY_2,
        "other/port-collection",
        "other/elsewhere",
    );
    refused(
        &renamed,
        "registry/repositories/987654321.toml: a registration of Nw7pQr2s adds its one package",
    );
    // Another package of the same repository is pointed at another folder, with its project.
    let repointed = appended();
    replace(
        repointed.path(),
        REPOSITORY_2,
        "mods/wayfinder",
        "mods/moved",
    );
    replace(repointed.path(), PROJECT_2, "mods/wayfinder", "mods/moved");
    refused(
        &repointed,
        "registry/projects/Zz91kLmN/project.toml: a registration of Nw7pQr2s changes only",
    );
    let other_project = appended();
    replace(
        other_project.path(),
        PROJECT,
        "registered = ",
        "paused = true\nregistered = ",
    );
    refused(
        &other_project,
        "registry/projects/Ab3dE9xQ/project.toml: a registration of Nw7pQr2s changes only",
    );
    let other_file = appended();
    fs::remove_file(other_file.path().join(RELEASE)).unwrap();
    refused(
        &other_file,
        "registry/projects/Ab3dE9xQ/releases/1.2.0.toml: a registration of Nw7pQr2s changes only",
    );
    // A second registration in the same change.
    let two = appended();
    fs::create_dir_all(two.path().join("registry/projects/Tw0pQr2s")).unwrap();
    fs::write(
        two.path().join("registry/projects/Tw0pQr2s/project.toml"),
        fs::read_to_string(two.path().join(NEW))
            .unwrap()
            .replace("Nw7pQr2s", "Tw0pQr2s")
            .replace("lanterns", "candles"),
    )
    .unwrap();
    replace(
        two.path(),
        REPOSITORY_2,
        "project = \"Nw7pQr2s\"\n",
        "project = \"Nw7pQr2s\"\n\n[[package]]\npath = \"mods/candles\"\nproject = \"Tw0pQr2s\"\n",
    );
    refused(&two, "adds exactly one project; this change adds 2");
}

/// `curl` on the child's `PATH` is a script that fails every transfer with the HTTP status it
/// is given, and counts how often it is started.
#[test]
fn a_server_error_is_retried_and_ends_apart_from_a_refusal_while_a_client_error_is_final() {
    use std::os::unix::fs::PermissionsExt;
    let checkout = checkout();
    let bin = checkout.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let curl = bin.join("curl");
    fs::write(
        &curl,
        "#!/bin/sh\nread -r http < \"$0.http\"\nprintf %s \"$http\"\necho >> \"$0.calls\"\nexit 22\n",
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let builds = checkout.path().join("builds.json");
    fs::write(
        &builds,
        r#"[{"id":"26.3-0.1.0","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","surface":"1111111111111111111111111111111111111111111111111111111111111111"}]"#,
    )
    .unwrap();
    // Exit 75 is `EX_TEMPFAIL`, which the release check reads as "could not check".
    for (http, starts, exit, message) in [
        ("504", 4, 75, "Server unavailable: download failed 4 times"),
        ("403", 1, 1, "download failed"),
    ] {
        fs::write(bin.join("curl.http"), http).unwrap();
        let calls = bin.join("curl.calls");
        fs::write(&calls, "").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_registry-bot"))
            .current_dir(checkout.path())
            .env("PATH", &bin)
            .args(["release-fit", "builds.json", RELEASE, "--out", "fit.json"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(exit), "{http}: {stderr}");
        assert!(stderr.contains(message), "{http}: {stderr}");
        assert_eq!(fs::read_to_string(calls).unwrap().lines().count(), starts);
    }
}

/// `gh` on the child's `PATH` is a script that fails with the message it is given.
#[test]
fn a_github_server_error_ends_a_release_check_apart_from_a_refusal() {
    use std::os::unix::fs::PermissionsExt;
    let checkout = checkout();
    let bin = checkout.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/bin/sh\nread -r message < \"$0.message\"\necho \"$message\" >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    for (message, exit) in [
        ("gh: Bad Gateway (HTTP 502)", 75),
        ("gh: Not Found (HTTP 404)", 1),
        // Text in a refusal cannot make it look unchecked.
        ("gh: Server unavailable: no release tagged so (HTTP 404)", 1),
    ] {
        fs::write(bin.join("gh.message"), message).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_registry-bot"))
            .current_dir(checkout.path())
            .env("PATH", &bin)
            .args([
                "verify-release",
                ".",
                "sunrise-org/sunrise-mods",
                "Ab3dE9xQ",
                RELEASE,
                "out",
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(exit), "{stderr}");
        assert!(stderr.contains(message), "{stderr}");
    }
}

/// `curl` on the child's `PATH` is a script that answers with the status in `curl.http` and,
/// when that is 200, writes the one line of `curl.body` where `--output` names.
#[test]
fn a_jar_modrinth_does_not_list_is_marked_until_modrinth_lists_it_again() {
    use std::os::unix::fs::PermissionsExt;
    let checkout = fit_checkout();
    let bin = checkout.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let curl = bin.join("curl");
    fs::write(
        &curl,
        "#!/bin/sh\nread -r http < \"$0.http\"\nprintf %s \"$http\"\n[ \"$http\" = 200 ] || exit 22\n\
         while [ \"$1\" != --output ]; do shift; done\nread -r body < \"$0.body\"\nprintf %s \"$body\" > \"$2\"\n",
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let sha512 = "ab".repeat(64);
    let row = checkout.path().join(RELEASE);
    let text = fs::read_to_string(&row).unwrap();
    fs::write(
        &row,
        format!(
            "{text}\n[[jars]]\nid = \"original\"\nversion = \"3.1.2\"\nsha512 = \"{sha512}\"\n"
        ),
    )
    .unwrap();
    let listed = serde_json::json!({&sha512: {"files": [{
        "hashes": {"sha512": &sha512},
        "url": "https://cdn.modrinth.com/data/original/original-3.1.2.jar",
    }]}})
    .to_string();
    let marked = serde_json::json!([{"id": "original", "version": "3.1.2", "sha512": &sha512}]);
    // Modrinth lists nothing; then it cannot be asked (exit 75) and the mark stands; then it
    // lists the jar again.
    for (http, body, exit, mark) in [
        ("200", "{}", 0, Some(&marked)),
        ("429", "", 75, Some(&marked)),
        ("200", listed.as_str(), 0, None),
    ] {
        fs::write(bin.join("curl.http"), http).unwrap();
        fs::write(bin.join("curl.body"), body).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_registry-bot"))
            .current_dir(checkout.path())
            .env("PATH", &bin)
            .args(["jars", "."])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(exit), "{http}: {stderr}");
        let release = &index(checkout.path())["projects"][0]["releases"][0];
        assert_eq!(release.get("jar_unavailable"), mark, "{http}");
        fs::remove_dir_all(checkout.path().join("output")).unwrap();
    }
}

/// Every verdict of the fixture is current, so the judging downloads nothing.
#[test]
fn a_judging_run_keeps_the_stored_list_of_unlisted_jars() {
    let checkout = fit_checkout();
    let stored = checkout.path().join("registry/fit/jars.json");
    let text = format!("[\n  \"{}\"\n]\n", "ab".repeat(64));
    fs::write(&stored, &text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_registry-bot"))
        .current_dir(checkout.path())
        .env("PATH", "")
        .args(["fit", "."])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(stored).unwrap(), text);
}
