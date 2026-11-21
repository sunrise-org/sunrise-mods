"""The game's lock beside a build's SDK data: kept when it can be fetched, and never required."""
import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import fetch_sdk_data

SHA = "0123456789abcdef0123456789abcdef01234567"


def gh(returncode, stdout):
    def run(command, **_):
        assert command[:2] == ["gh", "api"] and command[-1].endswith(f"contents/Cargo.lock?ref={SHA}")
        return subprocess.CompletedProcess(command, returncode, stdout, b"")
    return run


class GameLock(unittest.TestCase):
    def fetch(self, returncode, stdout):
        with tempfile.TemporaryDirectory() as folder, contextlib.redirect_stdout(io.StringIO()) as out:
            with mock.patch.object(fetch_sdk_data.subprocess, "run", gh(returncode, stdout)):
                fetch_sdk_data.fetch_game_lock(SHA, Path(folder))
            return sorted(p.name for p in Path(folder).iterdir()), out.getvalue()

    def test_the_lock_is_kept_beside_the_sdk_data(self):
        self.assertEqual(self.fetch(0, b"version = 4\n"), (["Cargo.lock"], ""))

    def test_a_lock_that_cannot_be_fetched_is_a_warning(self):
        for returncode, stdout in ((1, b""), (0, b""), (0, b"x" * (fetch_sdk_data.MAX_GAME_LOCK + 1))):
            files, said = self.fetch(returncode, stdout)
            self.assertEqual(files, [])
            self.assertIn(f"::warning::sdk-{SHA}", said)


if __name__ == "__main__":
    unittest.main()
