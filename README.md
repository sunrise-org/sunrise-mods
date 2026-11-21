# sunrise-mods

Sunrise's mod registry. Each project has an entry here, and every change to an entry is a pull request that a person reviews.
After a merge, CI publishes the index that the launcher and [playsunrise.net/mods](https://playsunrise.net/mods) read:

**<https://sunrise-org.github.io/sunrise-mods/index.json>**

Beside it, CI publishes the Nightly source list: native mods a player's launcher builds from their GitHub source
([Source mods](#source-mods)):

**<https://sunrise-org.github.io/sunrise-mods/sources.json>**

The format of the index and the rules for an entry are set in Sunrise's own code and docs:
[registry-index.md](https://github.com/sunrise-org/sunrise/blob/main/docs/modding/src/reference/registry-index.md)
and `sunrise_mod_package::index::registry`. CI runs that code (`cargo sunrise-mod registry`) and doesn't
reimplement it. What a listed project agrees to is in [MOD-POLICY.md](MOD-POLICY.md).

## Layout

| Path | What it is |
| --- | --- |
| `mods/<id>.toml` | A project's entry: its page, its owners and every version's file, pinned by SHA-256 |
| `icons/<id>.png` | The project's icon, or a collection's: a square PNG, 64–1024 px, at most 256 KiB |
| `collections/<id>.toml` | A collection: a named list of mods already listed here, shown as one card |
| `screenshots/<id>/<name>.png\|.jpg` | 1–8 images of the project in game. A new project needs at least one, and so does a listed project whose pull request adds a version. Collections need none; a mod in the `library` category needs none |
| `revoked.toml` | Files taken down. The launcher refuses to install them and turns installed copies off |
| `sources/<id>.toml` | A source mod's listing for the Nightly list: its repository, where its manifest is, and the branch or release to follow |

The `mods/`, `collections/` and `sources/` folders hold only `<id>.toml` files.

## Submitting a project

1. Check your package: `cargo sunrise-mod check my_mod.smod --key <key> --signature <file>`. Every new
   version is signed.
2. Fork this repository and add `mods/<id>.toml`, `icons/<id>.png` and at least one screenshot in
   `screenshots/<id>/`, listed in the entry's `screenshots` (a mod in the `library` category needs none). A mod's `id` is the `id` in its `sunrise-mod.toml`.
3. Open a pull request. Every new version names the stable Sunrise release it was made for (`sunrise`) and
   your own release asset as its `file`, named `<id>-<version>` with your kind's extension: the `check` workflow downloads it from your repository and tests it.
   Nothing is staged by hand. The entry needs your project's `key` and the version its `signature`.
   - A row without `sunrise` is from before stable releases. It stays listed as it is, and `check` warns
     about it; it can't be changed, and a new row without `sunrise` is refused.
   - A `key` means every listed version carries a signature. A project that still lists unsigned rows from
     before stable releases removes them in the pull request that adds its `key` and its first stable version.
     Removing such a row needs no line in `revoked.toml`.
4. A reviewer reads the change and merges it. The `publish` workflow then publishes the file as the release
   `<id>-<version>` and adds the project to the index.

The submission form at playsunrise.net/mods/submit will open these pull requests for you once its service runs.
Until then, submit by hand as described above.

```toml
id = "coords_hud"
kind = "mod"                  # mod, native-mod, modpack, resourcepack, shader, shaderpack, datapack
name = "Coordinates HUD"
summary = "Shows where you are."                          # at most 160 characters
description = "Markdown for the project's page, 50 to 10,000 characters."
authors = ["Your Name"]
owners = ["your-github-login"]  # who may publish updates
license = "MIT"                 # an SPDX expression: your project's own licence
source = "https://github.com/you/coords_hud"
icon = "icons/coords_hud.png"
screenshots = ["screenshots/coords_hud/1-hud.png"]   # 1 to 8
categories = ["utility"]        # 1 to 3 from your kind's list
tags = ["hud"]                  # up to 10, lowercase
key = "RWT..."                  # minisign public key: every listed version carries a signature

[[versions]]                    # newest first
version = "0.1.0"
published = "2026-11-16"
game = "26.3"
sunrise = "26.3-0.1.0"          # the stable Sunrise release it was made for
file = "https://github.com/you/coords_hud/releases/download/v0.1.0/coords_hud-0.1.0.smod"
sha256 = "<lowercase hex>"      # of that file
signature = """<the .minisig text>"""
changelog = ""
```

The tool reads a mod's API, side, capabilities and dependencies from the file itself, so you don't list them. A mod
that requires another mod can be listed only once that mod is. Unknown keys
are refused. The category lists are in
[`index/page.rs`](https://github.com/sunrise-org/sunrise/blob/main/crates/sunrise-mod-package/src/index/page.rs).

**Updates.** To publish an update, add a new `[[versions]]` table at the top of the list. A published file
never changes: to take one down, add it to `revoked.toml`. Only a row without `sunrise`, from before stable
releases, may leave an entry without that. A project's kind can't change.

**Code mods.** A `mods/` entry doesn't follow a repository's releases (`repo = ...`). Every version is listed as its
own file, as shown above. To list a native mod by its source instead, see [Source mods](#source-mods).

## Collections

A collection groups mods that are already listed, such as a library and the mods built on it, into one card a
player can install together. It is not a package: it has no file and no versions, and each member keeps its own
entry, page and updates.

To add one, open a pull request with `collections/<id>.toml` and `icons/<id>.png`:

```toml
id = "my_family"                # not the id of any project
name = "My Family"              # may be the same as a project's name
summary = "One line about the set."                       # at most 160 characters
description = "Optional Markdown for the collection's page."
owners = ["your-github-login"]  # who may change the collection
icon = "icons/my_family.png"
members = [
  { id = "my_library" },
  { id = "my_first_mod" },
  { id = "my_extra_mod", default = false },   # listed, but not ticked until the player ticks it
]
```

- Every member is a `mod` or `native-mod` listed in `mods/`. A collection has at least two members, and at least
  one that is installed by default. A member is installed by default unless it says `default = false`.
- A member's required dependencies are installed with it whether or not they are members.
- A project cannot be removed from `mods/` while a collection lists it.

**Adding a mod to your collection.** List the mod first, as any project. Then open a pull request that adds its id
to `members`. When you are an owner of the collection and of the mod, and the pull request changes nothing else,
the review marks it routine. A new collection, a change to its name, summary, description, icon, owners, member
order or defaults, and a member you do not own all wait for a reviewer.

## Source mods

The Nightly list names native mods by their source on GitHub. A player's launcher downloads the source at the
branch or release you name and builds it on their computer, after telling them the code hasn't been reviewed and
asking once for that mod. A listing is reviewed for its format and checked against GitHub when it is added or
changed: the repository is public, its name and id match, the branch or release exists, and the mod's
`sunrise-mod.toml` and its `Cargo.toml` parse at that commit. **The code is never reviewed or built here.** A
listing can sit beside a `mods/` entry of the same mod; the launcher then also offers the reviewed file.

To list one, open a pull request that adds `sources/<id>.toml`:

```toml
id = "trail_atlas"                              # the id in the mod's sunrise-mod.toml; the file is sources/<id>.toml
name = "Trail Atlas"                            # plain text, 1 to 80 characters
summary = "A field atlas for marking trails and camps."   # plain text, 1 to 160 characters
repo = "driftwood/trail-atlas"                  # GitHub owner/name exactly as GitHub spells it, no URL
repo_id = 123456789                             # the repository's GitHub id: gh api repos/driftwood/trail-atlas --jq .id
manifest = "mods/trail_atlas/sunrise-mod.toml"  # where the manifest is in the repository; "sunrise-mod.toml" at its root
track = { branch = "main" }                     # or { release = true }: the latest published release, not pre-releases
```

- Every key is required and no others are allowed. The `Cargo.toml` beside the manifest is the mod's crate, and the
  manifest needs a `[native]` table: a content-only mod is listed in `mods/` as a file instead.
- One repository may hold several mods, each with its own listing and manifest path.
- A listing has no URLs: the launcher builds every address from `repo` and the commit it resolves.
- Removing a listing takes the mod off the list; players who built it keep their copy. Rename a moved repository
  in the listing (GitHub's new `owner/name`; its id stays the same).

## How it works

Both workflows build `cargo-sunrise-mod` and the release verifier from the `sunrise-org/sunrise` commit named
in `.github/sunrise-rev` and cache the tools per commit. They look up the stable release tags and the current
Nightly's build, then download and verify the small SDK files needed to judge which builds each mod fits.
They cache those files per build. The Nightly's commit and date come from its release's title and
its signed `sunrise-release.json` and `sdk.json`, verified with the signature for exactly that build. Sunrise takes its `nightly` release away for a moment while it
publishes the next one: `check` then warns and judges a pull request against the stable releases only, and
`publish` stops before it deploys, so the last good index stays live.

- **`check`** runs on every pull request. It uses the tool pin and scripts from the base checkout and reads
  the proposed entries only as data. Its steps are:
  - `scripts/fetch_files.py`: downloads every listed file and checks its hash. A version that names a stable
    Sunrise release lists the author's own release asset, which is fetched from the author's repository until
    the registry holds its copy; an older version's file comes from `staging`. A stable native build's signed
    descriptor is fetched the same way, then the archives the verified descriptor names. A download stops at
    the registry tool's size limit, or at the size the descriptor gives an archive;
  - `registry review`: compares the pull request with `main`, and says which mods a takedown would leave without
    a dependency;
  - `registry check`: tests each new file, and refuses one whose required dependencies the registry cannot supply;
  - `registry build`: builds the whole index the merge would publish.
  Before those, `scripts/build_sources.py check` checks every `sources/` listing and asks GitHub about the new or
  changed ones. A separate job runs the tests of the registry's own scripts.
- **`publish`** runs on every push to `main`, and every three hours to refresh the release and Nightly
  metadata and counts. It downloads the listed files as `check` does and builds the index, which checks them;
  only then does it create the `<id>-<version>` release for each file still in `staging` or only in its
  author's release, under the file's own name, with a stable native build's descriptor and archives beside it.
  It then reads GitHub's
  download count of each listed file's release asset and the stars of each project's source repository
  (`scripts/fetch_counts.py`), builds `index.json` again with them, writes `sources.json` from `sources/`
  (`scripts/build_sources.py build`, without asking GitHub), and deploys both to GitHub Pages together with
  `icons/` and `screenshots/`. A count GitHub fails to answer (other than a repository that is gone) stops the
  run before it deploys, so the last good index stays live.
  Both workflows keep the downloaded files in an Actions cache, so our own runs fetch only files they haven't
  seen and stay out of the download counts.
- **`dco`**: every commit carries a `Signed-off-by:` line for its author.

To update the tool, change `.github/sunrise-rev` in a pull request.

## Sign-off

This repository uses the [Developer Certificate of Origin](https://developercertificate.org/). Sign off every
commit with `git commit -s`. A missing sign-off fails the `dco` check. To fix it, run
`git rebase --signoff origin/main` and push again.

## Licence

This repository's tooling and docs are under [PolyForm Shield 1.0.0](LICENSE). Each listed project keeps its own
licence, the one its entry names. Its files belong to their authors.
