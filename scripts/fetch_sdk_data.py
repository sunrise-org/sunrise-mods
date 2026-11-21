"""Fetch the signed SDK metadata needed to judge registry versions against a game build."""
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

REPO = "sunrise-org/sunrise"
SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"[0-9a-f]{64}")
PLAIN = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}")
ASSETS = {"surface.json": ("surface", "none"), "build-data.tar.xz": ("build-data", "xz")}
BUILD_DATA = {"functions.json", "internals.json", "loops.json"}
MAX_BUILD_DATA = 1 << 30  # The SDK tool's unpacked build-data cap.
MAX_GAME_LOCK = 16 << 20  # The registry tool's cap for the game's Cargo.lock.


def download(tag: str, folder: Path, names) -> None:
    subprocess.run(["gh", "release", "download", tag, "-R", REPO, "-D", str(folder),
                    *(arg for name in names for arg in ("-p", name))], check=True)


def unpack(archive: Path, target: Path) -> None:
    target.mkdir()
    seen = set()
    size = 0
    with tarfile.open(archive, "r:xz") as tar:
        for member in tar:
            if member.name == "." and member.isdir() and member.size == 0:
                continue
            name = member.name.removeprefix("./")
            if (not PLAIN.fullmatch(name) or name in seen or member.sparse is not None
                    or member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE)):
                raise ValueError(f"unexpected archive entry {member.name!r}")
            seen.add(name)
            size += member.size
            if member.size < 0 or size > MAX_BUILD_DATA:
                raise ValueError("build data exceeds its unpacked size limit")
            if name in BUILD_DATA:
                # Keep only what the registry reads, never archive paths, modes or links.
                with tar.extractfile(member) as source, (target / name).open("xb") as dest:
                    shutil.copyfileobj(source, dest)
    missing = BUILD_DATA - seen
    if missing:
        raise ValueError(f"missing build data: {', '.join(sorted(missing))}")


def fetch(sha: str, into: Path) -> None:
    tag = f"sdk-{sha}"
    with tempfile.TemporaryDirectory(dir=into) as folder:
        root = Path(folder)
        ready = root / "ready"
        ready.mkdir()
        download(tag, ready, ("sdk.json", "SHA256SUMS", "SHA256SUMS.sig"))
        subprocess.run(["sunrise-release-sign", "verify", "--repository", REPO, "--tag", tag,
                        "--only", "sdk.json", str(ready)], check=True)
        index = json.loads((ready / "sdk.json").read_text())
        if type(index["schema"]) is not int or index["schema"] != 1:
            raise ValueError("unknown sdk.json schema")
        if index["build"]["commit"] != sha:
            raise ValueError("sdk.json names a different build")
        assets = {}
        for name, (kind, codec) in ASSETS.items():
            asset = index["assets"][name]
            size, digest = asset["size"], asset["sha256"]
            if (asset["kind"] != kind or asset["codec"] != codec
                    or type(size) is not int or not 0 < size <= MAX_BUILD_DATA
                    or not isinstance(digest, str) or not DIGEST.fullmatch(digest)):
                raise ValueError(f"sdk.json has invalid metadata for {name}")
            assets[name] = (size, digest)
        download(tag, ready, assets)
        for name, (size, digest) in assets.items():
            file = ready / name
            if file.stat().st_size != size:
                raise ValueError(f"{name} does not match its sdk.json size")
            with file.open("rb") as data:
                if hashlib.file_digest(data, "sha256").hexdigest() != digest:
                    raise ValueError(f"{name} does not match its sdk.json SHA-256")
        (ready / "SHA256SUMS").unlink()
        (ready / "SHA256SUMS.sig").unlink()
        archive = ready / "build-data.tar.xz"
        unpack(archive, ready / "build-data")
        archive.unlink()
        # A commit directory is only put in the cache after every check passes.
        ready.rename(into / sha)


def fetch_game_lock(sha: str, folder: Path) -> None:
    """Keep the game's own lock, which review compares a native mod's requirements with.

    No SDK asset carries all of it, so it is read from the build's commit, which names its content.
    Without it the SDK data stays usable and review says the comparison was skipped.
    """
    result = subprocess.run(["gh", "api", "-H", "Accept: application/vnd.github.raw",
                             f"repos/{REPO}/contents/Cargo.lock?ref={sha}"], capture_output=True)
    if result.returncode != 0 or not 0 < len(result.stdout) <= MAX_GAME_LOCK:
        print(f"::warning::sdk-{sha}: the game's Cargo.lock could not be fetched; "
              "native dependency requirements are not compared for this build")
        return
    partial = folder / "Cargo.lock.partial"
    partial.write_bytes(result.stdout)
    partial.rename(folder / "Cargo.lock")


def main(inputs: Path, into: Path) -> None:
    releases = json.loads((inputs / "releases.json").read_text())
    # Absent while Sunrise has no `nightly` release (fetch_releases.py --nightly-optional).
    nightly = inputs / "nightly-build.json"
    commits = {row["commit"] for row in [*releases, *([json.loads(nightly.read_text())] if nightly.exists() else [])]}
    if not all(isinstance(sha, str) and SHA.fullmatch(sha) for sha in commits):
        sys.exit("registry inputs contain an invalid build commit")
    into.mkdir(parents=True, exist_ok=True)
    # Reuse the verified commit directories from the last run, not Nightlies no longer listed.
    for path in into.iterdir():
        if path.is_dir() and SHA.fullmatch(path.name) and path.name not in commits:
            shutil.rmtree(path)
    for sha in sorted(commits):
        if not (into / sha).is_dir():
            try:
                fetch(sha, into)
            except (subprocess.CalledProcessError, OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
                sys.exit(f"sdk-{sha}: {error}")
        if not (into / sha / "Cargo.lock").is_file():
            fetch_game_lock(sha, into / sha)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: fetch_sdk_data.py <input-dir> <sdk-data-dir>")
    try:
        main(Path(sys.argv[1]), Path(sys.argv[2]))
    except (OSError, ValueError, KeyError, TypeError) as error:
        sys.exit(f"could not read SDK inputs: {error}")
