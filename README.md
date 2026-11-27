# sunrise-mods

Sunrise's mod registry. It lists the projects whose releases the launcher's Discover and
[playsunrise.net/mods](https://playsunrise.net/mods) show, and publishes them as one signed file:

**<https://sunrise-org.github.io/sunrise-mods/v2/index.signed>**

## Listing your mod

1. **Prepare your repository.** In `page.toml`, write a short summary and a long description and
   list at least one screenshot under `page/`. Categories are optional. The page uses the mod's
   icon from `sunrise-mod.toml`. `cargo sunrise-mod new` writes the caller workflow; for an existing
   mod, `cargo sunrise-mod release setup` writes it without tagging a release. Commit and push these
   files before submitting.
2. **Register once.** Open a pull request on this repository that adds your public repository to
   `registry/repositories/<repository ID>.toml` and one `registry/projects/<project ID>/project.toml`
   for each package, as [the reference](docs/registry-v2.md) describes. Use GitHub's numeric repository
   ID (`https://api.github.com/repos/<owner>/<name>`, field `id`) and pick a new project ID of eight
   characters from `0-9`, `A-Z` and `a-z`. That ID is permanent; the project appears in Discover after
   its first release is approved. Or open a
   [Register a repository](https://github.com/sunrise-org/sunrise-mods/issues/new?template=register.yml)
   issue with the link to your repository: the registry checks it and opens that pull request for you,
   answering on the issue. An issue can register only a repository of the personal account that opens
   it; for an organisation's repository, an admin of it uses the form at
   [Submit a mod](https://playsunrise.net/mods/submit), which does the same after a GitHub sign-in.
3. **Release from your package folder.** After registration merges, run `cargo sunrise-mod release <version>`.
   The command updates the caller workflow and bumps the manifest version, then tags and pushes.
   The shared workflow publishes a signed `.smod` and a separate signed page bundle. A native mod
   ships source and `Cargo.lock`, not prebuilt binaries; the launcher builds it on the player's computer.
   The registry bot checks the release and opens the release PR for you.
4. **Group your mods into a collection (optional).** Once several of your projects are registered, they can
   show as one card in Discover. In a pull request on this repository, add `registry/collections/<collection ID>/`
   with `collection.toml` (`id`, `name`, `summary`, an optional `description`, and one `[[member]] project =
   "<project ID>"` per mod, `default = false` to start one unticked) and a square PNG `icon.png` of at most
   512×512 pixels and 256 KiB. The collection ID is a new, permanent eight characters from `0-9`, `A-Z` and `a-z`.
   Each project's ID is its `project` in `registry/repositories/<repository ID>.toml`. Every member must be in a
   repository owned by the same GitHub account, and the pull request's author must be listed in `owners` of every
   member. Discover shows the collection once two of its members have a reviewed release, one of them ticked by
   default; it then shows the members only inside the collection, and a search still finds each by name.
   [Group your mods into a collection](https://github.com/sunrise-org/sunrise/blob/main/docs/modding/src/how-to/collections.md)
   walks through it.

Things to get right, since each one otherwise costs a round trip:

- **Several packages from one repository, one pull request.** They share that repository's file, so
  separate pull requests for them conflict. Different repositories never do.
- **The repository ID is your repository's own.** A fork has a different ID from the repository it
  was forked from. The watcher follows the ID, not the name.
- **The slug and the project ID are new.** The check refuses a slug or project ID another project has.
- **Actions are on.** GitHub turns Actions off in a new fork until its owner enables them, and the
  release workflow runs in your repository. The repository must be public.
- **The release tag is new.** A fork starts with every tag of the repository it was forked from.
  Release a version whose tag (`v<version>`, or `<mod>/v<version>` in a subfolder) does not exist yet.
- **The first release proposed is your newest one.** For a project with no approved release the
  watcher looks only at the newest published release. It looks every 10 minutes.
- **One release pull request per project at a time.** A later release is proposed once the open one
  is merged or closed.

Discover lists only signed, reviewed releases on Stable, Nightly and Dev. Nightly and Dev releases
name an exact build, not a branch. Unreviewed mods stay out of Discover; the launcher offers them
only through the manual **Add from source** action, with a warning.

See [the registry v2 reference](docs/registry-v2.md) for the file layout and
[the review guide](REVIEWING.md) for what reviewers check.

What a listed project agrees to is in [MOD-POLICY.md](MOD-POLICY.md).

## Layout

| Path | What it is |
| --- | --- |
| `registry/repositories/<repository ID>.toml` | A registered GitHub repository and the packages in it |
| `registry/projects/<project ID>/` | A project: `project.toml`, one file per approved release under `releases/`, and its page |
| `registry/collections/<collection ID>/` | A collection: mods of one publisher, shown as one card |
| `registry/categories.toml` | The categories a page may name |
| `registry/release-workflow.toml` | The shared release workflow every release must be signed by |
| `registry/revoked.toml` | Releases and projects taken down |
| `registry/index-key.pub` | The public key the published index is signed for |
| `bot/` | The registry bot: checks, the release watcher, the index builder |
| `scanner/` | The first-pass scanner whose flags a release review reads |

[The reference](docs/registry-v2.md) describes every file and field.

## How it works

- **`Release a Sunrise mod`** (`release.yml`) is the shared workflow an author's repository calls: it builds,
  signs and publishes a release there.
- **`Watch registered releases`**, started every ten minutes by `registry timer`, finds new releases in the
  registered repositories and opens one pull request per release.
- **`Check registry release PR`** verifies a release pull request: hashes, attestations, the scanner and a
  build. A person or agent then reviews it ([REVIEWING.md](REVIEWING.md)).
- **`registry-v2`** checks every pull request that changes the registry or the bot.
- **`judge fits`** runs on every push to `main`, and by hand after an SDK is published. It works out which
  game builds each release fits, stores the verdicts on the `registry-fit` branch and starts `publish`.
- **`publish`** assembles, signs and deploys the index.
- **`register-issue`** answers a Register issue with a registration pull request. When the registry could not
  answer, the timer starts it again, up to five more times ([Registration retries](docs/registry-v2.md#registration-retries)).
- **`dco`**: every commit carries a `Signed-off-by:` line for its author.

## Sign-off

This repository uses the [Developer Certificate of Origin](https://developercertificate.org/). Sign off every
commit with `git commit -s`. A missing sign-off fails the `dco` check. To fix it, run
`git rebase --signoff origin/main` and push again.

## Licence

This repository's tooling and docs are under [PolyForm Shield 1.0.0](LICENSE). Each listed project keeps its own
licence, the one its entry names. Its files belong to their authors.
