# Sunrise mod registry v2

The registry keeps its source files in `registry/`. The launcher and site read the single file
`v2/index.signed` on the registry site, not the TOML files. The author tool reads
its repository's file under `repositories/`, `categories.toml`, and `release-workflow.toml` from raw `main`.

## Repository layout

All TOML tables reject unknown keys. Text fields described as dates use quoted `YYYY-MM-DD` strings.
`project` and project `id` values are eight case-sensitive base62 characters (`0–9`, `A–Z`, `a–z`).
For a new first release, the watcher derives the project ID from the numeric repository ID and
canonical package path. Published keys do not change. `mod` is the in-game ID
from `sunrise-mod.toml`. It follows Sunrise's mod-ID rules, including the length and reserved
Windows device names.

Paths in the layout use forward slashes. They may not be absolute, contain backslashes or drive
prefixes, or contain empty, `.` or `..` segments. A package path may be exactly `.` for the root
of the author's repository. The checker does not follow symlinks in the registry tree.

### `registry/repositories/<repository ID>.toml`

One file per registered repository, named by its positive numeric GitHub ID in plain decimal, so
registrations of different repositories never edit the same file. The file holds the current
`name` (`owner/repo`); the watcher refreshes it on a rename, and the ID in the file name is the
identity. A repository-only registration adds this file and nothing else. It carries one `[[registrant]]`
with the authenticated registrant's numeric GitHub account `id` and `login`. An existing repository
row is not changed by another registration. The repository is tracked, not listed as a project.

```toml
name = "example/wayfinder"

[[registrant]]
id = 42
login = "example"
```

During cutover, the checker still reads legacy `[[package]]` lists and checks every link against
its project. An existing project can refer directly to its repository row and path, without an entry
in a package list. A reviewed migration can record several registrants for a legacy repository.
The registry app's repository-only registration check expects exactly one registrant and no package
list. Until the website's writer switches, its repository add or append with new project rows
still goes through the ordinary layout, DCO and non-release checks, not the repository-only exemption.
The cutover removes this old-writer pass-through from the `bot` check after the legacy proposals
are resolved.

### `registry/release-workflow.toml`

Maintainers set `uses` to `sunrise-org/sunrise-mods/.github/workflows/release.yml@release-v1`.
The file has only this key. The suffix is a `release-v<number>` tag, not a moving branch. The
author tool copies `uses` into the caller workflow. Change the pointer to a new tag when that
version of the shared workflow is available.

### `registry/categories.toml`

Maintainers own the list. Each `[[category]]` has a unique `id`, a Lucide `icon` name, and `kinds`,
the package kinds that may use it. The initial names come from Modrinth's mod categories. No
Modrinth artwork is used, and neither readers nor the checker need to query Modrinth.

### `registry/projects/<project>/project.toml`

For a new project, the watcher writes this file in its first-release PR after verifying the
package, page and signed package path. The project key is not a manifest field. The watcher does not edit
the repository row.

| Field | Meaning |
| --- | --- |
| `id` | Permanent project ID. It matches the directory name. |
| `slug` | The in-game ID if available, otherwise the in-game ID plus `-` and the lowercased project key. Unique site page address, `/mods/<slug>`. It uses `a–z`, `0–9`, `-`, and `_` and may change in a reviewed PR. It is never one of the site's own pages under `/mods/`: `mine`, `policy`, `registry-page` or `submit`. |
| `mod` | In-game ID. Only two subfolder packages in the same repository cannot share it, because both use it in release tags and caller workflow names. A root package has its own tag and caller names. |
| `kind` | `mod`, `native-mod`, `modpack`, `resourcepack`, `shader`, `shaderpack` or `datapack`. Pack-specific fields are not defined here. |
| `owners` | GitHub logins of the registrants for a new project, resolved by numeric account ID at first release. Existing project owners are retained; neither page credits nor tag pushers grant ownership. |
| `repository`, `path` | Numeric repository ID and package path, anchored to the existing `repositories/` file for that numeric ID. |
| `workflow` | Signer workflow path, equal to the part of `uses` before `@`. |
| `registered` | For a first-release PR, the UTC publication date of the verified GitHub release. |
| `paused` | Optional; `true` stops the watcher from proposing releases of this project. The registration, the ID and the publisher pin stay, and releases already in the registry stay listed. Set and cleared in a reviewed PR. |

### `registry/projects/<project>/releases/<version>.toml`

The watcher adds one file per approved release. A merged release file is not edited except by a
takedown. When a release's package is of another kind than the project (a content mod that gained
code, or the reverse), the same pull request changes the project's `kind` and removes the project's
earlier release files, so every listed release has the project's kind. The earlier releases stay listed until that pull request merges: changing the `kind` and removing them ahead of it would take the project off the registry while the new release is in review. The pull request check refuses a change that removes a project's last release unless the same change sets `paused = true`. The checker checks a checkout, not the history of that file.

| Field | Meaning |
| --- | --- |
| `version` | Package semantic version. It matches the filename without `.toml`. |
| `kind` | `mod` or `native-mod`: what the package is, read from its manifest. It equals the project's `kind`. |
| `tag` | Git tag. Use `v<version>` at the repository root and `<mod>/v<version>` in a subfolder. |
| `published` | GitHub's publication time of the release, in UTC to the second (`2026-11-23T06:55:18Z`). The index publishes only its date. |
| `sunrise` | The stable tag (for example `26.3-0.1.0`) or exact Nightly/Dev build ID (`<channel>-<YYYY-MM-DD>-<commit>`) the package was made for. A rolling channel name is not a build ID. |
| `workflow` | Exact signer workflow ref recorded by the attestation: the project's workflow path followed by `@refs/tags/release-v<number>`. Existing releases keep their ref when `release-workflow.toml` selects a new tag. |
| `repository` | Numeric ID of the repository the release was signed from. The bot writes the project's repository when it proposes the release; a later move of the project leaves the row as it is. |
| `[file]` | The `.smod` asset. Native mods ship source and a lockfile, not a binary. |
| `[page]` | The separate page bundle. It is not part of the install or the published index. |
| `[[dependencies]]` | Optional list. Each entry names a `mod` slot, the `version` requirement on its provider (semver, as written in the mod's manifest), and whether it is `required`. An optional `project` pins a specific registered provider of that in-game ID. |
| `[[jars]]` | Optional list, in the order of the package manifest's `[[jar]]` tables: the `id`, `version` and `sha512` of each original Java jar the mod layers its assets and data on. The bot writes it from the package and the release check refuses a row that differs. The registry stores these references only, never a jar. |

Both asset tables give the HTTPS `url`, `size` in bytes, `sha256` (64 lowercase hexadecimal
characters), and HTTPS `attestation` URL. Both URLs use
`https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>` with the release row's tag.
The named assets are `<mod>-<version>.smod` and `<mod>-<version>.page.tar`; each bundle adds
`.sigstore.json` to its asset's name. The owner/repo in a URL may reflect a repository's earlier
name. The layout checker does not download the assets, compare hashes, or verify that each
asset's GitHub artifact attestation (Sigstore) matches the registered repository and release ref.

A release made by the shared workflow also carries `<mod>-<version>.release.json`, a fifth asset
with no bundle of its own. The workflow's trusted `publish` job writes it from its `path` input,
passed as data, without reading the author's tree: `{"path":"<package path>"}`, with `.` for the
repository root. It is a second named subject of the same attestation statement as the `.smod`, so
the package's bundle covers it. It has one key and no version field. Releases made before the
workflow wrote it have no such file; those, and every release that is not a first release,
are not affected.

### `registry/projects/<project>/page/`

The watcher copies the newest release's page assets here. It writes a resolved `page.toml` after
applying defaults from the package's `sunrise-mod.toml`. A project does not need a page before its
first release. After that, it needs all of the following:

- `name`, `summary`, `description`, `authors` (list), and `license` from the resolved page.
  The summary needs at least 25 Unicode scalar values and the description at least 125, after
  trimming surrounding whitespace. Markdown is not parsed or normalized for this count.
- `icon`, pointing to the mod's own icon copied from the package.
- At least one `[[screenshots]]` row with `path` and `caption`. The watcher writes an empty string
  if the author omitted a caption.

The page can also give `homepage`, `issues`, and `source` as HTTP(S) URLs. `categories` is an
optional list of 0–8 distinct category IDs that apply to the project's `kind`. There are no
free-form tags.

All image paths are relative to the resolved `page.toml`: use `gallery/recipe.png`, not
`page/gallery/recipe.png`. Each image must exist inside this folder. Screenshots are checked for
a PNG or JPEG signature, a matching `.png`, `.jpg` or `.jpeg` extension, and the 5 MiB gallery
limit. The icon follows the mod package's own PNG icon rules and must have a `.png` extension.
Apart from `page.toml`, files not named in the page are rejected. The publisher serves only the
validated images.

### `registry/collections/<collection>/`

A collection is one publisher's set of mods that a player installs together, each member switched on
or off. It is a folder named by a permanent ID of the same form as a project ID, chosen at random by
whoever opens it; the ID never changes and may not be a project's. The folder holds
`collection.toml` and `icon.png`, which follows the project icon rules.

```toml
id = "Qm4tZ81c"
name = "Wayfinding"
summary = "The map and its markers."
description = "Optional longer text."

[[member]]
project = "Ab3dE9xQ"

[[member]]
project = "Zz91kLmN"
default = false
```

`name` and `summary` are required. Each `[[member]]` names a registered `project`; `default` says
whether it is selected when a player opens the collection and is `true` when omitted. A collection
has at least two different members and at least one selected by default.

**Every member belongs to one publisher.** A project's publisher is the GitHub account that owns the
repository it is registered to. The checker refuses a collection whose members' repositories are
owned by different accounts, so the file stores no publisher. The publisher curates it, and so may
the registry's maintainers: a pull request that adds, changes or deletes a collection must be opened
by a login listed in `owners` of every member, before and after the change, as `owners` stands on
the base commit, or by a person with write access to this repository. A maintainer's collection is
merged by hand after review, and the publisher may still edit or remove it.

### Fit verdicts: `registry/fit/`

Whether a release fits a game build is judged by one job, `fit.yml`, and stored on the
`registry-fit` branch, which holds `registry/fit/` and nothing else. `main` never contains that
directory: the checkout checker rejects it there, and release pull requests carry no fit verdicts. `.github/registry-v2/fit_store.py` is the only code that knows where the verdicts are
kept; it refuses a branch that holds any other file or a commit the job did not make.

`builds.json` lists the offered builds as `{ id, commit, surface }`, where `surface` is the SHA-256 of
the build's signed `surface.json`. Each `<project>.toml` has one `[[release]]` row per release:
`version`, `package` (the SHA-256 of the package the row was judged from), `rules` (the version of the
judging rules, `RULES` in `bot/src/fit.rs`), and the tables `fits` and `does_not_fit`, each mapping a
build ID to the surface digest it was judged against. A verdict is current only when all three inputs
match the release row on `main`, the offered build and the tool; anything else is judged again, so a
change to the rules raises `RULES`. Rows for a build that is no longer offered are ignored.

`fit.yml` runs on every push to `main` and by dispatch. A push is judged on the builds already
offered. A dispatch also looks for builds to offer: every published
`26.3-<semver>` stable release and up to the ten newest builds with a published SDK surface from each
of Nightly and Dev, named by exact build ID. A build whose surface this tool cannot read is not
offered. The job verifies each package's size and SHA-256 and the SDK's signed metadata and surface; it
does not build or run packages.

A native package is judged on a build by that build's own mod tool, the one a launcher on that build
runs: the job takes `cargo-sunrise-mod` from the build's signed SDK release, checks it against the
digest `sdk.json` lists, stages the package's files as the launcher does and asks `native-check`. The
release fits when the tool's verdict is `made-for` or `fits`. Any other verdict, a refusal, or a
package the tool cannot read is "does not fit", and the job's log has the tool's words. So a newer
package format an older build's tool refuses is never offered to that build. A build whose SDK has no
tool for the job's computer (Linux x86_64), or a tool from before `native-check`, gets no verdict for
native packages, and a launcher on it checks them itself. The tool runs with an empty environment, in
a step of `fit.yml` that holds no token. A content-only package fits every build with its content API.

A cell that cannot be judged (a download that fails, a tool that gives no answer) fails the run and
stores nothing: an unknown is never recorded as "does not fit". A run that
changed what is stored starts `publish.yml`, so the new verdicts are published.

Nobody has to dispatch it when Sunrise publishes a build. A run that looks for builds also stores
`registry/fit/considered.json`: every published Sunrise release that carries `sdk.json` (game
builds and SDKs), by tag and target commit, with the build it is offered as or the reason it is
not. Each `timer.yml` run lists Sunrise's releases and dispatches `fit.yml` when one of them is
not in that list. An SDK is published before its game build, so the SDK is considered and declined
first and the game release dispatches again when it appears. The timer holds no build policy and
never fails on this look: an API error is a warning and the next run looks again. Running
`timer.yml` by hand with `force` dispatches `fit.yml` regardless.

The site also serves a copy of Sunrise's own release list, at
`github/repos/sunrise-org/sunrise/releases`: every published release as GitHub's API returns it,
in one array, never a draft. A launcher reads it only when GitHub's API is rate-limiting its
network. `publish.yml` writes it on every publish, and `timer.yml` starts a publish when the last
one is about half an hour old, so the copy is rarely much older than that. A publish that cannot
read the list deploys nothing, the index included, and the site keeps what it has until the next run.

The release check judges a proposed release the same way on the builds offered at that moment
(`registry-bot release-fit`) and shows the result in its report, so a reviewer sees where the release
will appear before merging. It stores nothing: the job above judges the release again after the
merge. A release that cannot be judged on an offered build fails its check; a build that gets no
verdict is in neither list.

### `registry/revoked.toml`

Maintainers record `[[revoked]]` rows with `project`, `reason`, `date`, and optional `sha256`. Without
`sha256`, the entire project is taken down; otherwise only the file with that digest is taken down.
A given project/hash pair may appear only once. A row may outlive a listing or a release. The empty
list is `revoked = []`.

### Sources for the registry limits

In Modrinth `modrinth/code` at `7f6963c`, `apps/labrinth/fixtures/labrinth-seed-data-202508052143.sql:3–80`
contains the category names for project type `1` (`mod`, from
`apps/labrinth/migrations/20210509010206_project_types.sql:3–9,20–22`). The project ID length and base62
alphabet are from `apps/labrinth/src/database/models/ids.rs:29–30,236` and
`apps/labrinth/migrations/20240115052708_base62-helper-functions.sql:5`. In `apps/labrinth/src/validate/project/`,
`summary.rs:13`, `description.rs:17`, and `tags.rs:10` set the minimums and category cap. The screenshot
limit is `max_gallery_image_size` in `apps/labrinth/src/routes/internal/globals.rs:57`, shared by the
Sunrise page reader. The icon rules come from `sunrise_mod_package::icon::Icon`.

## Published index

The top-level object contains:

| Field | Meaning |
| --- | --- |
| `schema` | Integer `2`. |
| `serial` | Opaque unsigned 64-bit sequence number. A new index must exceed the live serial. Carrying the live index over keeps its exact bytes and serial. |
| `generated` | UTC RFC 3339 timestamp. Informational only; it does not expire the index. |
| `builds` | Array of offered `{ id, commit }` pairs. `commit` is the full SDK commit SHA. |
| `categories` | Array of `{ id, icon, kinds }` rows, as above. |
| `projects` | Projects with at least one release. |
| `collections` | Collections with at least two listed members, described below. |
| `revoked` | Takedown rows as above. A `null` `sha256` takes down the whole project. |

Each project carries `id`, `slug`, `mod`, `kind`, `owners`, and `workflow` from the project record.
Its `repository` object has `id`, `name`, and `path`. `name`, `summary`, `description`, `authors`,
`license`, `homepage`, `issues`, `source`, `categories`, `icon`, and `screenshots` come from the
resolved page. Missing links are `null`; absent categories are `[]`. Each screenshot is
`{ path, caption }`. Resolve image paths relative to the directory of `index.json`. They have the
form `pages/<project>/<sha256>.png` or `pages/<project>/<sha256>.jpg`: the digest is the SHA-256 of
the exact served bytes, in lowercase hexadecimal, and the extension is from the image format.
The author's original filename is not published.

Each project's `releases` array contains rows with `version`, `published`, `sunrise`, `workflow`,
`repository`,
`fits`, `file`, `dependencies` and, when the release lists any, `jars` (as in the release row). The release's `workflow` is an exact signer ref, not the project's
bare workflow path. `fits` is an array of exact build IDs and `does_not_fit` (left out when empty) those judged not to
fit. An offered build in neither list has not been judged for that release yet: a release is
published as soon as it is merged, and its verdicts follow within the judging job's next run.
Readers filter using `fits` only.

`dependency_unavailable` (left out when empty) lists `{ build, mod }` pairs: a build the release
fits on which no listed release can satisfy its required dependency `mod`, so it cannot be installed
there. A provider counts on a build when it is another project's listed release of that in-game ID
(the pinned `project` when the dependency pins one), inside the dependency's `version` range, not
revoked, fits the build, and can have its own required dependencies satisfied in turn; releases that
need each other count as satisfying each other. A provider not judged on the build yet counts as
fitting, so a pair appears only when no verdict still to come could change it. Optional
dependencies are never considered. It is derived from the rows and stored verdicts each time the
index is assembled and is stored nowhere. The registry bot and the launcher's install planner
decide "satisfies" with the same code, the `sunrise-dependency-rule` crate.

`jar_unavailable` (left out when empty) lists the release's `jars`, each as `{ id, version, sha512 }`,
that Modrinth no longer lists: the release cannot be newly installed, and an installed copy whose
jar is already on the player's machine is unaffected. The judging job asks Modrinth for every jar
the release rows name in one request, without downloading any, and stores the SHA-512 of each one
Modrinth's answer leaves out in `registry/fit/jars.json`; the index copies from there. Only that
answer marks a jar or clears a mark: when Modrinth cannot be asked (a server error, a rate limit, a
broken connection) the stored list stands. While any release names a jar, the registry timer starts
the judging job on every tick, so a jar that disappears is marked, and one that returns is cleared,
at the next tick.
`file` contains
`url`, `size`, `sha256`, and `attestation`. Each dependency carries `mod`, `version`, `project` and `required`.
`version` is the semver requirement the provider's release must satisfy; a `null` `project` means
the dependency does not pin a provider. The index omits the Git tag and
page bundle.

The top-level `collections` array has one row per collection with `id`, `name`, `summary`,
`description` (empty when the file has none), `publisher` (the account login), `icon`
(`pages/<collection>/<sha256>.png`) and `members`, each with `project` and `default`. `members`
lists only projects present in `projects`, in file order; a collection appears once two of its
members are listed and one of those is selected by default. Takedowns and fit are not repeated:
a reader applies `revoked` and each member's `fits` as it does for a project on its own. The launcher does not fetch the bundle to show the page or install the mod.

### Reader rules

1. Fetch `v2/index.signed` and nothing else for the catalogue. Its first four lines, each ending in
   a line feed, are the minisign signature file; every byte after the fourth line feed is
   `index.json`. Verify the signature with the registry public key over those **exact bytes**.
   The site also carries `index.json` and `index.json.minisig` as the publisher's own record. Do not
   read that pair: the site replaces files one at a time behind a cache, so the two can briefly come
   from different publishes and fail to verify.
   Do not parse and reserialize those bytes for verification.
2. Accept only schema 2 at this address. Ignore unknown fields under schema 2. A breaking change
   raises `schema` and uses a new address.
3. Refuse a serial lower than the last accepted serial. Do not use `generated` for rollback checks.
4. Apply project and digest takedowns even if the listing or release is still present.
5. Use the release's exact fit list for the selected build. Do not infer compatibility from a version
   number or from the `sunrise` field alone.
6. Treat `id` as the project identity, `slug` as the page address, and `mod` as a slot. No array order
   chooses a provider. Enable at most one provider per slot in a profile and never switch it silently.
7. Check each file's hash and verify its attestation against that release's own `repository` ID
   and exact `workflow` ref; an index from before the field has none on a row, and the project's
   repository ID stands in. On install the launcher does this for the `.smod`. Remember
   the registered repository and workflow path per project; use the approved ref for each release.
   Verifying the catalogue is not a substitute for verifying the package.
8. Verify each icon and screenshot against the SHA-256 in its filename before displaying it.

## Local checks and example

- `registry-bot check <checkout>` validates a local tree, reports file and reason on each diagnostic
  line, and exits nonzero for an invalid tree.
- `registry-bot builds --out <file>` lists the Sunrise releases that carry `sdk.json` from GitHub,
  each with the build it is offered as or why it is not. For authenticated API access, pass the
  usual GitHub CLI token in its environment.
- `registry-bot fit <checkout> [--builds <file>]` brings `registry/fit/` in the checkout up to date,
  judging only what has no current verdict. With `--builds` the offered builds become the readable
  ones in that list, which is stored as `considered.json`; without it both stay as they are.
- `registry-bot release-fit <builds.json> <release.toml> --out <file>` judges one release row on each
  build in a stored `builds.json` and writes the build IDs it fits and does not fit, storing nothing.
- `registry-bot index <checkout> --serial <n> --out <dir>` validates the same tree and stages
  `index.json` and the exact referenced images under `pages/`. Use `--generated-fit` to read the
  verdicts in `registry/fit/`; a release with no current verdict on an offered build is written with
  that build in neither list.
  The command uses the current UTC time for `generated` and
  does not sign or deploy anything.
- `registry-bot verify-index <index> <sig> --key <pub>` checks the exact bytes and the index schema.

Every workflow gets `registry-bot` through `.github/actions/registry-tool`. `registry-tools.yml`
builds the tool from `main` on every change to its source and publishes it as
`registry-tool-<digest>`, where the digest is
`git ls-tree <commit> -- bot/Cargo.toml bot/Cargo.lock bot/src | sha256sum`. The release carries the
binary and `PIN`, one line naming that digest and the binary's SHA-256, with an attestation that
`registry-tools.yml` wrote it on `main`. A run computes the digest of its own checkout, downloads that
release, verifies the attestation and both values in `PIN`, and fails if any of them is wrong. When
the release does not exist (a pull request that changes the tool, or the minutes after such a merge
while the release is built) the run builds the tool from its checkout, so it never runs a tool that
disagrees with the code.

The independent index verifier (`bot/verifier`) is released the same way, as
`registry-verifier-<digest>` with the digest
`git ls-tree <commit> -- bot/verifier bot/src/index_signature.rs | sha256sum`, and publish's `stage`
and `confirm-live` jobs get it through the same action with `name: registry-verifier`. It stays its
own build and release, so a tool that does not build cannot keep a publish from recovering the last
signed snapshot.

The publish workflow uses the current UTC epoch second as the next serial. Signing and deployment
both require a commit of `main` that is not older than the deployed one, so a merge during a publish
does not stop it and a rerun of an old publish cannot bring back old content under a new serial. Immediately before publishing, deployment compares the
candidate's signed serial with the verified live index and the snapshot used by the last successful
Pages deployment. A valid live response can lag that deployment, so it replaces a missing or
unverifiable snapshot from a keyed deployment only when its serial and index digest are exactly
what that deployment recorded: after each keyed deploy, the workflow writes both in a
`registry-v2/deployed` commit status that names the run. Assembly verifies the index and images and
checks that the Pages archive will include them. If the new catalogue fails, the workflow uses the
last complete signed snapshot, from Pages first or from the last successful deployment's retained
artifact. It deploys that snapshot again and reports the failure. If neither source can be
recovered it reports `NO_AUTHENTICATED_V2_SNAPSHOT`, and if the deployed serial cannot be
established it reports `V2_DEPLOYED_SERIAL_UNKNOWN`; either way the entire site is left unchanged. Without a committed public key, the publish does not contact
the v2 services and publishes no index. Publishing downloads and judges
nothing: it runs on every push to `main`, and again when `fit.yml` has stored new verdicts, and
assembles the index from `main` and the verdicts stored at that moment. It never waits for `main`
to stop moving.

The [two-provider fixture](../bot/tests/fixtures/two-ports/registry/) and
[its golden index](../bot/tests/fixtures/two-ports/index.json) show two projects for a single in-game ID.

## The watcher and release PR checks

`watch.yml` polls every 10 minutes and can also be started manually on `main`. GitHub's own schedule
events rarely arrive, so `timer.yml` starts it: each timer run starts the watcher, waits ten minutes and
starts the next timer run, and a push to `main` restarts a chain that has stopped. It looks up each
repository by its numeric GitHub ID. A rename opens a separate maintenance PR for that repository's file;
it does not change registrants, project owners or IDs, or rewrite old release URLs. The watcher reads
existing project rows directly, including paused projects and reviewed moves. It also discovers
first releases in every tracked repository. A repository row without registrants (one from before
they were recorded) cannot name an owner and has none invented: a stream of it with no `release.json`
asset stays silent, and a stream that carries one, once it is not claimed or already handled, is
reported in `watch.md` as a signed first release that cannot be taken, until a reviewed change
records a registrant. Of each tag stream (one namespace, or the
root) that no registered project claims, only the newest publication by GitHub's timestamp is
a candidate, even if it has no metadata; then that stream is not admitted and no older release of it
is read or downloaded, so a tracked repository whose only tags predate `release.json` publishes one
more release. The candidate's package must authenticate and its `<mod>-<version>.release.json`
asset must be a subject of that same verified package statement. The file is the only source of the
package path. A candidate that cannot be verified, or whose metadata is missing or invalid, does not
stop another stream. Two streams that sign one path derive one project key from the repository ID
and the path, so one project per path is admitted by the open-PR check, the publish-time skip of a
project that has an open PR, and the ID and path collision check. After an approval, only releases
published later than the latest approved release are eligible. That order comes from the publication
times in the committed release rows, not from what the author's repository shows now, so taking a
release down does not stop the watcher. The same order selects the previous approved package for the
scanner. When its tag, package or attestation is no longer published, a fully verified new release
still goes to review: the scan has nothing to compare against and the release check's comment says so.
Taking a package down never lifts a takedown in `revoked.toml`. An open
release PR holds its project's later releases back until it is merged or closed, unless its release
check failed: the author's fix for a failed check is a later release, which gets its own PR while
the failed one is still open. Close the failed one once it is superseded. An existing bot PR makes
that version handled: an open one, a merged
one, and a closed, unmerged one while its `release/<project>/<version>` branch still exists. The watcher
does not recreate it. Drafts never count; signed prereleases can count.
Observation and publication failures appear in the watch summary.

The release check regenerates a first release's `project.toml` on `main`, with its owners resolved
from the registrants of the repository's row on `main`, and compares it byte for byte with the PR's.
It refuses an occupied key (project or collection), a reserved or occupied slug, an occupied
repository/path or subfolder in-game namespace, and owners that differ from the current logins of
the repository's registrants. After a merge-time refusal the PR is closed and its branch deleted, so
the watcher proposes the release again.

### Moving a project to another repository

A project moves by an ordinary reviewed pull request: it changes `repository` and `path` in
`project.toml`, lists the package in the new repository's file and takes it out of the old one's.
Release rows are left as they are, each naming the repository it was signed from.

For every project whose `repository` changes, the pull request's check looks both repositories up
by ID and writes into its summary whether they belong to the same GitHub account, compared by the
account's numeric ID. A move to another account is allowed and is marked **DIFFERENT OWNER**, so
the reviewer sees it; players who have the project installed are then asked to confirm the new
owner. The check fails only when a repository cannot be looked up.

### Refusing or withdrawing a release

Closing a release PR without merging does one of two things, decided by its branch:

- **Refuse the release:** close the PR and keep its `release/<project>/<version>` branch. The watcher
  never opens that version again. The author's next release opens a new PR.
- **Withdraw the PR:** close it and delete its branch, for example when the PR must be generated again
  after the registry's format changes. The next watch run opens the same release in a new PR, as long
  as it is still published and newer than the project's latest approved release.

Deleting the branch of a refused release later withdraws the refusal the same way.

### Proving a change to the shared release workflow

`release-v1` moves to a workflow change only after one real publish of it from a branch ref shows
the change working and an ordinary release unaffected. The bot's trust is not relaxed for this:
`SignerWorkflowRef` still accepts only `refs/tags/release-v<number>`, so the bot refuses such a
release, and the proof does not go through the bot or any registered repository.

1. Push the workflow change to a branch of this repository, for example `proof/release-json`. No tag
   is created or moved.
2. In a scratch public repository that is not in `registry/repositories`, call
   `sunrise-org/sunrise-mods/.github/workflows/release.yml@proof/release-json` from a tag push, once
   for a root package (`v0.0.1`, `path: .`) and once for a subfolder (`<id>/v0.0.1`, `path: <folder>`).
3. The release has five assets: `.smod`, `.page.tar`, `.release.json` and the two `.sigstore.json`
   bundles. `.release.json` is `{"path":"."}` or `{"path":"<folder>"}`.
4. Fetch GitHub's trusted root once (`gh attestation trusted-root > trusted_root.jsonl`), then verify
   both bundles offline against the exact branch signer, not a prefix:

   ```text
   gh attestation verify <stem>.smod --bundle <stem>.smod.sigstore.json \
     --custom-trusted-root trusted_root.jsonl --repo <scratch-owner>/<scratch-repo> \
     --cert-oidc-issuer https://token.actions.githubusercontent.com \
     --cert-identity https://github.com/sunrise-org/sunrise-mods/.github/workflows/release.yml@refs/heads/proof/release-json \
     --source-ref refs/tags/<tag> --source-digest <tag commit> --format json > verified.json
   ```

   Check the flags against the installed `gh attestation verify --help` first. Run it again for the
   page archive.
5. In `verified.json`, `buildSignerURI` equals that exact identity, and
   `verificationResult.statement.subject` holds exactly two `sha256` entries for the package result:
   `<stem>.smod` and `<stem>.release.json`, each equal to `sha256sum` of the downloaded asset. The page result
   keeps its one subject. The existing consumers still read the same `.smod` and bundles.
6. Only then is `release-v1` moved, as a separate reviewed act.

### Libraries

A library is an ordinary project: it registers, tags and releases like any other mod, and a mod that
needs it names it in the `[dependencies]` of its `sunrise-mod.toml`. A release whose required
dependency has no release in the registry is refused before its build: another project on `main`
must provide that in-game ID with a release that satisfies the `version` requirement and is not
taken down (the project the dependency pins, when it pins one). So a library's release PR merges and
publishes first; the dependent's check is then run again. The build takes each library from the
signed index, checked against its attestation, and a library's crates are pinned by the library's
own lockfile, which its own release check scanned.

The read-only job downloads the `.smod`, page tar and both bundles. It checks GitHub's sizes and
available hashes, then uses `gh attestation verify` to check each artifact against the release
workflow and the same tagged source commit, resolved once. It checks the **verified certificate**
for the registered numeric repository ID and exact `release-v<number>` workflow ref, and requires
both bundles to agree. For a first release it also reads the `verificationResult.statement.subject`
list of the verified package result (`gh attestation verify --format json`; the same statement the
certificate checks above belong to) and requires the exact `<mod>-<version>.release.json` name with
the SHA-256 of the downloaded asset. It reads that asset only from the live release, through the same
URL, size and hash checks as the other assets and under a 4096-byte limit. Its `path` must be
canonical and match the root or subfolder tag, and the package's manifest must agree with the asset
name and tag. Native packages must pin a commit that a signed Sunrise build names. When a
content-only package has no SDK pin, the watcher records the current signed Nightly's exact build,
not the moving alias. The checker resolves the same exact build when it checks the PR. The existing
registry package download cap is 64 MiB (`sunrise_mod_package::index::MAX_FILE`); page entries use
Sunrise's manifest and screenshot limits. The write job gets only generated registry files. It
opens `release/<project>/<version>` and dispatches `release-check.yml` on `main` with the PR number
and exact head SHA. The registry's own GitHub Actions token does this; authors do not need one.

The release check reads the PR's Git tree as data, without checking it out. Only a new release row
and that project's page are ordinarily allowed to change; an existing change of kind also replaces the
project kind and removes older release rows. A first release may only add a project row, one release
row and the resolved page as regular files. It must not replace an existing project directory or
change its repository row. Owners come only from the registrants' numeric account IDs on `main`;
display names, stored logins and old package links are not authority. The release check
resolves current GitHub logins for those accounts. The
release check takes all four download URLs from the live release
of the registered repository, verifies the downloaded hashes and attestations, and requires the
release tree to match what it regenerated. GitHub API digests may be absent; the local SHA-256
checks still apply. An old URL for a previously approved source is accepted after a rename only if
it still reaches the same asset; the stored SHA-256 must still match.

The scanner receives the unpacked source and the last approved package when there is one. For the
native template's API path dependency, the scanner fetches the signed SDK pinned by the mod into a
scratch copy before passing the lockfile to Cargo; it never builds the mod. A separate hosted job
builds on Linux; the macOS and Windows compile checks are switched off for now, because waiting for
a macOS runner made each release slow. The job uses the signed
`cargo-sunrise-mod` tool, and its signed `tool.json` must declare `release_contract >= 1`. Publish
such a tool before enabling the watcher. Jobs that handle the package have no write permissions,
secrets, persisted checkout token or Actions cache access.

The release checks are static checks plus a compile: no job starts the game or loads the mod in
it. There is no cackle report: the mod tool generates its own Cargo work tree and installs its own
rustc wrapper for the SDK-native build; it does not expose a cackle entry point into that build.

The check sets `registry-v2/release-check` to pending on the checked **PR head**, not on the
workflow's `main` commit. A final job in the same run posts the current scanner summary and job
outcomes in one PR comment and updates that status. Checks for one PR are serialized, and only the
newest dispatch and attempt for the head may post. It passes only when every required job succeeds
and the trusted scanner's report verifies for that attempt. A skipped, cancelled, timed-out or
incomplete check is not green; if cancellation prevents the final job from starting, the status
stays pending. Use **Rerun all jobs**; do not rerun only failed jobs. Ordinary scanner flags guide
review without failing the release.

A release whose manifest lists original Java jars (`[[jar]]`) is checked against them too. The
check asks Modrinth for each jar by its SHA-512, downloads it from Modrinth's CDN and compares the
bytes, then runs the mod tool's reference check (`cargo sunrise-mod check`) on the mod's files with
the jars beneath them. A jar Modrinth does not list, or a file neither the mod nor its jars hold,
fails the check with the jar's `id`, `version` and `sha512` or the file named. The jars stay in the
job's working directory: none reaches an artifact, a cache or the index.

A download that fails with a server error (HTTP 5xx), a timeout or a broken connection is tried
again three times, after one, two and four seconds; a rate limit (HTTP 429) is not tried again;
any other failure, another 4xx included, is final at once. A check that still could not download
something, was rate limited, or whose GitHub lookup or attestation verification met a server error
or a failed connection, says nothing about the release, and its status says so: `failure` with the
description `Not checked: GitHub or Modrinth failed. Not a verdict on the release`. The watcher dispatches the check for that head once more on its next pass; if that
one ends the same way, use **Rerun all jobs**.

Enable **Allow GitHub Actions to create and approve pull requests** before turning on the watcher.
Require `registry-v2/release-check` on `main` for every PR. A no-checkout workflow reports success
with `not a release` for PRs that change no files under `registry/projects/*/releases/` or
`registry/projects/*/page/`. Only the dispatched check can pass a PR that changes those paths or a release branch opened by the watcher.
GitHub starts no pull request workflow for a PR the bot opens, so the watcher asks that same
workflow by number to look at its own maintenance PRs, such as a repository rename.
A maintainer must approve the ordinary `dco` workflow runs for PRs the bot opens with
`GITHUB_TOKEN`, and the `registry-v2` run of a PR that moves the tool's Sunrise pin
(`bot-pin.yml`).

## Registration retries

`register-issue.yml` asks the registry service about a `Register ` issue on the opener's own open, edit
or reopen, and on `workflow_dispatch` with the issue's number. A run's title carries the number
(`Registration of issue N`, `Registration retry for issue N`). The service reads the issue's live opener, so a
dispatch registers that account, never the dispatcher, and refuses an issue that is closed, not titled
`Register `, or whose opener is suspended. A run makes one call. It never retries by itself, and an Actions
"re-run" is refused.

A run writes the issue's one status comment of the bot's. Its first line is a marker
(`<!-- registry-v2-registration run=<run id> state=retry|unanswered|done|exhausted -->`); only the bot's own
comment counts. A registration, `already_registered`, `already_pending` or another definite answer is `done`. A
typed `{"retry": …}` answer (rate, busy, GitHub, registry head) is `retry` and the run stays green. No JSON, an
edge's page, a bare 403, a connection failure or JSON of another shape is `unanswered`. The first unanswered run
of a cycle is **red**; a dispatch that again gets no answer, right after one that got none, writes the comment
and stays green. The comment asks the author to edit the issue only for a problem the author can fix.

Each `tick` of `timer.yml` has one step, `registration_retry.py retry`, that looks at the open `Register ` issues.
It reads the whole history of `register-issue.yml` runs, unfiltered and paginated, and stops with an error
if the list is shorter than GitHub's own total, so a history it could not read is never an empty one. Runs are
identified and ordered by run id. For each issue, a cycle is the opener's newest own `issues` run and the
`workflow_dispatch` runs after it (an edit or reopening by the opener starts a new cycle). The opener's run is the
run whose `actor.id`, the account that caused its first attempt, is the open issue's `user.id`; `triggering_actor`,
who last re-ran it, is never read. Only runs on `main` count. A maintainer's edit (and any re-run of it), a dispatch from
another branch and a run Actions skipped on its first attempt start no cycle and use no attempt. The step:

- waits while the newest run of the cycle has not finished, however long it has been running;
- stops when that run's comment is `done` or `exhausted`, or the run ended green with no comment of its own (the
  registry's silent answer for an issue that is closed or not a registration);
- starts one more run when the run ended any other way: a typed `retry`, an `unanswered` comment, or red, cancelled
  or timed out before it commented (the step then writes the `unanswered` comment itself first);
- when five dispatches of the cycle have run, writes `exhausted` into the comment and goes red once.

An issue with no run at all, because the event never started one, is dispatched unless the bot has already
commented on it. One dispatch per issue per tick is the pace of the serialized `tick` job: it holds its concurrency
group for the ten minutes between ticks. Nothing here has a clock; the registry's own rate check counts each dispatch
against the opener like any other try.

A failure of the step turns `timer.yml`'s run red, and every later step of `tick` still runs. These workflows send
no notification of their own: a red run is the signal.

A maintainer recovers an exhausted or stuck issue with
`gh workflow run register-issue.yml -R sunrise-org/sunrise-mods --ref main -f issue=N`. It counts as an attempt, and
the timer starts none after the fifth.
