import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
import zipfile

import build_release
import release_pr as release

SHA = "a" * 40
REPO = "sunrise-org/sunrise-mods"


def pull(branch="release/Ab3dE9xQ/1.0.0", number=10, state="open"):
    return {"number": number, "state": state, "merged_at": None, "user": {"login": release.BOT},
            "base": {"ref": "main"}, "head": {"repo": {"full_name": REPO}, "ref": branch, "sha": SHA}}


def listing(pulls, branches=()):
    """GitHub's lists as `rows` returns them: the release branches that exist, or the pull requests."""
    refs = [{"ref": f"refs/heads/{branch}"} for branch in branches]
    return Mock(side_effect=lambda endpoint: refs if endpoint.startswith("git/matching-refs/") else pulls)


class ReleasePRTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, GITHUB_REPOSITORY=REPO, GITHUB_REF=release.MAIN,
                                 PR_NUMBER="10", PR_HEAD=SHA, GITHUB_RUN_ID="1", GITHUB_RUN_ATTEMPT="1",
                                 REPORT_DOWNLOADED="true")
        environment.start()
        self.addCleanup(environment.stop)
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.folder = Path(directory.name)

    def test_dispatch_rejects_wrong_author_base_sha_or_path(self):
        good = pull()
        cases = (("user", "login", "author"), ("base", "ref", "dev"), ("head", "sha", "b" * 40),
                 ("head", "ref", "not-a-release"), ("head", "repo", {"full_name": "someone/fork"}))
        for group, key, value in cases:
            with self.subTest(key=key):
                pr = copy.deepcopy(good)
                pr[group][key] = value
                with patch.object(release, "api", return_value=pr), patch.object(release, "git") as git:
                    with self.assertRaisesRegex(ValueError, "open registry release bot PR"):
                        release.verify(self.folder / "out")
                    git.assert_not_called()
        with patch.object(release, "api", return_value=good), patch.object(release, "git") as git:
            git.side_effect = [b"", b"b" * 40,
                               b"registry/projects/Ab3dE9xQ/releases/1.0.0.toml\x00.github/workflows/evil.yml\x00"]
            with self.assertRaisesRegex(ValueError, "may only add its own release row"):
                release.verify(self.folder / "out")
            # The head is fetched as the commit the dispatch named: its refs/pull ref may not exist.
            self.assertEqual(git.call_args_list[0].args, ("fetch", "--no-tags", "origin", SHA))

    def test_a_kind_change_must_remove_every_earlier_release_and_touch_nothing_else(self):
        root = "registry/projects/Ab3dE9xQ"
        row, project = f"{root}/releases/1.0.0.toml", f"{root}/project.toml"
        entry = lambda name: f"100644 blob {'c' * 40}\t{name}".encode()
        cases = (
            # The head still lists an earlier release beside the new one.
            ([row, project], [row, f"{root}/releases/0.9.0.toml"], "must remove every earlier release"),
            # Another project's release is not this pull request's to delete.
            ([row, project, "registry/projects/Zz91kLmN/releases/2.0.0.toml"], None, "may only add its own release row"),
            # Without a kind change, deleting an earlier release is outside the pull request's scope.
            ([row, f"{root}/releases/0.9.0.toml"], None, "may only add its own release row"),
        )
        for changed, head_rows, message in cases:
            with self.subTest(message=message, changed=changed):
                calls = [b"", b"b" * 40, "".join(name + "\0" for name in changed).encode()]
                if head_rows is not None:
                    calls += [b"", b"".join(entry(name) + b"\0" for name in head_rows)]
                with patch.object(release, "api", return_value=pull()), patch.object(release, "git") as git:
                    git.side_effect = calls
                    with self.assertRaisesRegex(ValueError, message):
                        release.verify(self.folder / "out")

    def test_rejection_and_publish_error_do_not_block_another_project_or_dispatch(self):
        rejected = pull(state="closed")
        open_pr = pull("release/Zz91kLmN/1.0.0", number=11)
        for branch in (rejected["head"]["ref"], "release/Bb3dE9xQ/1.0.0", "release/Cb3dE9xQ/1.0.0"):
            (self.folder / "releases" / branch.removeprefix("release/")).mkdir(parents=True)
        summary = self.folder / "summary.md"
        api = Mock(side_effect=[{"object": {"sha": SHA}}, {"statuses": []}, [{"workflow_runs": []}]])
        with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(summary)), patch.object(release, "api", api), \
             patch.object(release, "rows", side_effect=[[rejected, open_pr], [{"ref": "refs/heads/" + rejected["head"]["ref"]}], [open_pr]]), \
             patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh") as gh, \
             patch.object(release, "open_pr", side_effect=[release.CommandError("::error\n##[warning]"), None]) as publish:
            release.publish(self.folder)
        self.assertEqual([call.args[1] for call in publish.call_args_list],
                         ["release/Bb3dE9xQ/1.0.0", "release/Cb3dE9xQ/1.0.0"])
        gh.assert_called_once_with("workflow", "run", release.WORKFLOW, "--repo", REPO,
                                   "--ref", "main", "-f", "pr=11", "-f", f"head={SHA}")
        text = summary.read_text()
        self.assertIn("Could not publish release/Bb3dE9xQ/1.0.0", text)
        self.assertNotIn("::error", text)
        self.assertNotIn("##[", text)

    def test_a_rename_refresh_only_rewrites_repository_files_already_on_main(self):
        generated, work = self.folder / "rename", self.folder / "work"
        row = Path("registry/repositories/123456789.toml")
        for root, text in ((generated, 'name = "example/renamed"\n'), (work, 'name = "example/wayfinder"\n')):
            (root / row).parent.mkdir(parents=True)
            (root / row).write_text(text)
        with patch.object(release, "git") as git:
            release.install_tree(generated, work, "registry/refresh-repository-names")
        self.assertEqual((work / row).read_text(), 'name = "example/renamed"\n')
        git.assert_called_once_with("add", "--", row, cwd=work)
        for stray in ("registry/repositories/987654321.toml", "registry/repositories/0123.toml",
                      "registry/projects/Ab3dE9xQ/project.toml"):
            with self.subTest(stray=stray):
                (generated / stray).parent.mkdir(parents=True, exist_ok=True)
                (generated / stray).write_text("")
                with self.assertRaisesRegex(ValueError, "not a set of registered repository files"):
                    release.install_tree(generated, work, "registry/refresh-repository-names")
                (generated / stray).unlink()

    def test_a_release_of_another_kind_replaces_the_project_file_and_the_earlier_releases(self):
        generated, work = self.folder / "release", self.folder / "work"
        root = Path("registry/projects/Ab3dE9xQ")
        row, old, project = root / "releases/1.1.0.toml", root / "releases/1.0.0.toml", root / "project.toml"
        for base, files in ((generated, {row: "new", root / "page/page.toml": "page", project: 'kind = "native-mod"\n'}),
                            (work, {old: "old", root / "page/page.toml": "before", project: 'kind = "mod"\n'})):
            for name, text in files.items():
                (base / name).parent.mkdir(parents=True, exist_ok=True)
                (base / name).write_text(text)
        with patch.object(release, "git") as git:
            release.install_tree(generated, work, "release/Ab3dE9xQ/1.1.0")
        self.assertEqual((work / project).read_text(), 'kind = "native-mod"\n')
        self.assertFalse((work / old).exists())
        self.assertEqual((work / row).read_text(), "new")
        git.assert_any_call("add", "--", project, old, cwd=work)
        # Without a project file in the artifact, the earlier release and the project stay as they are.
        (generated / project).unlink()
        (generated / row).rename(generated / root / "releases/1.2.0.toml")
        with patch.object(release, "git"):
            release.install_tree(generated, work, "release/Ab3dE9xQ/1.2.0")
        self.assertTrue((work / row).exists())
        self.assertEqual((work / project).read_text(), 'kind = "native-mod"\n')
        # Another project's file is never taken.
        stray = generated / "registry/projects/Zz91kLmN/project.toml"
        stray.parent.mkdir(parents=True)
        stray.write_text("")
        (work / root / "releases/1.2.0.toml").unlink()
        with self.assertRaisesRegex(ValueError, "outside its release row and page"):
            release.install_tree(generated, work, "release/Ab3dE9xQ/1.2.0")

    def test_a_maintenance_pr_is_sent_to_the_non_release_classifier_once(self):
        rename = pull("registry/refresh-repository-names", number=12)
        summary = self.folder / "summary.md"
        for statuses, expected in (([], 1), ([{"context": release.STATUS}], 0)):
            api = Mock(side_effect=[{"object": {"sha": "0" * 40}}, statuses])
            with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(summary)), patch.object(release, "api", api), \
                 patch.object(release, "rows", listing([rename])), \
                 patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh") as gh:
                release.publish(self.folder)
            self.assertEqual(gh.call_count, expected)
            if expected:
                gh.assert_called_once_with("workflow", "run", release.NOT_A_RELEASE, "--repo", REPO,
                                           "--ref", "main", "-f", "pr=12")

    def test_main_moving_during_observation_withholds_only_what_it_changed(self):
        origin, generated = self.folder / "origin", self.folder / "watch"
        git = lambda *args: subprocess.run(("git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", *args),
                                           cwd=origin, check=True, capture_output=True).stdout

        def commit(path, text):
            (origin / path).parent.mkdir(parents=True, exist_ok=True)
            (origin / path).write_text(text)
            git("add", "--", path)
            git("commit", "-q", "-m", path)
            return git("rev-parse", "HEAD").decode().strip()

        origin.mkdir()
        git("init", "-q", "-b", "main")
        git("remote", "add", "origin", str(origin))
        held = "registry/repositories/7.toml"
        base = commit(held, 'name = "example/held"\n[[package]]\nproject = "Cc3dE9xQ"\n')
        for project in ("Ab3dE9xQ", "Bb3dE9xQ", "Cc3dE9xQ"):
            (generated / "releases" / project / "1.0.0").mkdir(parents=True)
        (generated / "rename/registry/repositories").mkdir(parents=True)
        (generated / "rename" / held).write_text("")
        moves = (("docs/registry-v2.md", "text\n", {"Ab3dE9xQ", "Bb3dE9xQ", "Cc3dE9xQ", "rename"}),
                 ("registry/projects/Bb3dE9xQ/project.toml", "paused = true\n", {"Ab3dE9xQ", "Cc3dE9xQ", "rename"}),
                 (held, 'name = "example/held"\n[[package]]\nproject = "Cc3dE9xQ"\npath = "a"\n', {"Ab3dE9xQ", "Bb3dE9xQ"}),
                 ("bot/src/watch.rs", "\n", set()))
        for path, text, expected in moves:
            git("checkout", "-q", "main")
            git("reset", "-q", "--hard", base)
            live = commit(path, text)
            git("checkout", "-q", "--detach", base)
            summary = self.folder / "summary.md"
            summary.write_text("")
            with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(summary)), \
                 patch.object(release, "api", return_value={"object": {"sha": live}}), \
                 patch.object(release, "rows", listing([])), patch.object(release, "git", git), \
                 patch.object(release, "open_pr") as open_pr:
                release.publish(generated)
            opened = {call.args[1].split("/")[1] if call.args[1].startswith("release/") else "rename"
                      for call in open_pr.call_args_list}
            self.assertEqual(opened, expected, path)
            self.assertTrue(all(call.args[2] == base for call in open_pr.call_args_list))
            withheld = {"Ab3dE9xQ", "Bb3dE9xQ", "Cc3dE9xQ", "rename"} - expected
            self.assertEqual(summary.read_text().count(path), len(withheld) if expected else 1, path)

    def test_a_closed_release_pr_blocks_its_version_only_while_its_branch_exists(self):
        withdrawn, refused = "release/Ab3dE9xQ/1.0.0", "release/Bb3dE9xQ/1.0.0"
        for branch in (withdrawn, refused):
            (self.folder / "releases" / branch.removeprefix("release/")).mkdir(parents=True)
        closed = [pull(withdrawn, number=10, state="closed"), pull(refused, number=11, state="closed")]
        with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(self.folder / "summary.md")), \
             patch.object(release, "api", return_value={"object": {"sha": SHA}}), \
             patch.object(release, "rows", listing(closed, branches=[refused])), \
             patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh"), \
             patch.object(release, "dispatch_runs", return_value=[]), patch.object(release, "open_pr") as open_pr:
            release.publish(self.folder)
        self.assertEqual([call.args[1] for call in open_pr.call_args_list], [withdrawn])

    def test_an_open_release_pr_holds_a_later_release_back_unless_its_check_failed(self):
        earlier, later = "release/Ab3dE9xQ/1.0.0", "release/Ab3dE9xQ/1.0.1"
        (self.folder / "releases" / later.removeprefix("release/")).mkdir(parents=True)
        for state, opened in (("pending", []), ("failure", [later])):
            with self.subTest(state=state):
                api = Mock(side_effect=lambda endpoint, state=state: {"statuses": [{"context": release.STATUS, "state": state}]}
                           if endpoint.endswith("/status") else {"object": {"sha": SHA}})
                with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(self.folder / "summary.md")), \
                     patch.object(release, "api", api), patch.object(release, "rows", listing([pull(earlier)])), \
                     patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh"), \
                     patch.object(release, "dispatch_runs", return_value=[]), patch.object(release, "open_pr") as open_pr:
                    release.publish(self.folder)
                self.assertEqual([call.args[1] for call in open_pr.call_args_list], opened)

    def test_a_burst_of_open_release_prs_lists_the_dispatched_runs_once(self):
        first, second = pull("release/Ab3dE9xQ/1.0.0", number=10), pull("release/Bb3dE9xQ/1.0.0", number=11)
        api = Mock(return_value={"object": {"sha": "0" * 40}, "statuses": []})
        runs = Mock(return_value=[{"display_title": f"Registry release PR 10 at {SHA}"}])
        with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(self.folder / "summary.md")), \
             patch.object(release, "api", api), patch.object(release, "rows", listing([first, second])), \
             patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh") as gh, \
             patch.object(release, "dispatch_runs", runs):
            release.publish(self.folder)
        runs.assert_called_once_with()
        gh.assert_called_once_with("workflow", "run", release.WORKFLOW, "--repo", REPO,
                                   "--ref", "main", "-f", "pr=11", "-f", f"head={SHA}")

    def test_a_check_stopped_by_github_being_unavailable_is_dispatched_once_more(self):
        title = {"display_title": f"Registry release PR 10 at {SHA}"}
        for description, runs, again in ((release.UNAVAILABLE, [title], True), (release.UNAVAILABLE, [title, title], False),
                                         ("Release checks failed or did not complete", [title], False)):
            with self.subTest(description=description, runs=len(runs)):
                status = {"context": release.STATUS, "state": "failure", "description": description}
                api = Mock(return_value={"object": {"sha": "0" * 40}, "statuses": [status]})
                with patch.dict(os.environ, GITHUB_STEP_SUMMARY=str(self.folder / "summary.md")), \
                     patch.object(release, "api", api), patch.object(release, "rows", listing([pull()])), \
                     patch.object(release, "git", return_value=SHA.encode()), patch.object(release, "gh") as gh, \
                     patch.object(release, "dispatch_runs", return_value=runs):
                    release.publish(self.folder)
                self.assertEqual(gh.call_count, int(again))

    def test_registry_bot_stopping_because_github_was_unavailable_is_passed_to_the_report(self):
        output = self.folder / "output"
        for code, written in ((release.UNAVAILABLE_EXIT, "unavailable=true\n"), (1, "")):
            output.write_text("")
            with patch.dict(os.environ, GITHUB_OUTPUT=str(output), REGISTRY_BOT="registry-bot"), \
                 patch.object(release.subprocess, "run", return_value=Mock(returncode=code)), \
                 self.assertRaisesRegex(ValueError, f"exit {code}"):
                release.bot("release-fit")
            self.assertEqual(output.read_text(), written)

    def test_github_being_unavailable_is_reported_apart_from_a_failed_check(self):
        jobs = self.report_jobs()
        jobs["verify"] = {"result": "failure", "outputs": {"attempt": "1", "unavailable": "true"}}
        jobs["build"]["result"] = jobs["scanner"]["result"] = "skipped"
        jobs["scanner"]["outputs"] = {}
        posts = self.run_report(jobs, downloaded="false")
        self.assertIn("**Not checked**", posts[-2]["body"])
        self.assertEqual((posts[-1]["state"], posts[-1]["description"]), ("failure", release.UNAVAILABLE))

    def report_jobs(self, attempt="1"):
        path = self.folder / "summary.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("summary.md", "A trusted scan report.\n")
        return {"pending": {"result": "success"}, "build": {"result": "success"},
                "verify": {"result": "success", "outputs": {"attempt": attempt, "fit": "fits dev-b; does not fit dev-a"}},
                "scanner": {"result": "success", "outputs": {"attempt": attempt, "summary-id": "84",
                            "summary-digest": hashlib.sha256(path.read_bytes()).hexdigest()}}}

    def run_report(self, jobs, run_id="1", attempt="1", downloaded="true", newest=None):
        calls = Mock(return_value=pull())
        matching_runs = [{"id": int(run_id), "run_attempt": int(attempt),
                          "display_title": f"Registry release PR 10 at {SHA}"}]
        if newest is not None:
            matching_runs.append({**matching_runs[0], "id": int(newest)})
        with patch.dict(os.environ, JOB_RESULTS=json.dumps(jobs), GITHUB_RUN_ID=run_id,
                        GITHUB_RUN_ATTEMPT=attempt, REPORT_DOWNLOADED=downloaded), \
             patch.object(release, "api", calls), patch.object(release, "rows", return_value=[]), \
             patch.object(release, "dispatch_runs", return_value=matching_runs):
            if newest is None:
                release.pending()
            else:
                with self.assertRaisesRegex(ValueError, "not the latest run"):
                    release.pending()
            release.report(self.folder)
        posts = [call for call in calls.call_args_list if call.args[1:] == ("-X", "POST")]
        return [call.kwargs["data"] for call in posts]

    def test_older_success_cannot_overwrite_newer_failure(self):
        jobs = self.report_jobs()
        first = self.run_report(jobs, run_id="111")
        jobs["build"]["result"] = "failure"
        second = self.run_report(jobs, run_id="112")
        self.assertEqual([item["state"] for item in (first[0], first[-1], second[0], second[-1])],
                         ["pending", "success", "pending", "failure"])
        self.assertIn("/112/attempts/1", second[-1]["target_url"])
        jobs["build"]["result"] = "success"
        self.assertEqual(self.run_report(jobs, run_id="111", attempt="2", newest="112"), [])

    def test_skipped_cancelled_and_timed_out_jobs_fail(self):
        for result in ("skipped", "cancelled", "timed_out"):
            with self.subTest(result=result):
                jobs = self.report_jobs()
                jobs["build"]["result"] = result
                self.assertEqual(self.run_report(jobs)[-1]["state"], "failure")

    def test_partial_rerun_never_renders_an_earlier_summary(self):
        jobs = self.report_jobs()
        posts = self.run_report(jobs, attempt="2", downloaded="false")
        self.assertNotIn("A trusted scan report.", posts[-2]["body"])
        self.assertIn("Rerun all jobs", posts[-2]["body"])
        self.assertEqual((posts[-1]["state"], posts[-1]["description"]), ("failure", "Rerun all jobs"))

    def test_the_report_shows_the_fit_on_the_offered_builds(self):
        body = self.run_report(self.report_jobs())[-2]["body"]
        self.assertIn("\n**On the offered builds**: fits dev-b; does not fit dev-a.\n", body)

    def test_a_missing_comparison_is_named_above_the_scan_and_still_passes(self):
        jobs = self.report_jobs()
        self.assertNotIn("No comparison", self.run_report(jobs)[-2]["body"])
        jobs["verify"]["outputs"]["comparison"] = "missing"
        posts = self.run_report(jobs)
        body = posts[-2]["body"]
        self.assertLess(body.index("No comparison with the last approved release"), body.index("A trusted scan report."))
        self.assertEqual(posts[-1]["state"], "success")

    def test_scanner_report_digest_is_from_trusted_job_output(self):
        jobs = self.report_jobs()
        with zipfile.ZipFile(self.folder / "summary.zip", "w") as archive:
            archive.writestr("summary.md", "different report")
        api = Mock(return_value=pull())
        with patch.dict(os.environ, JOB_RESULTS=json.dumps(jobs)), patch.object(release, "api", api), \
             patch.object(release, "current_run", return_value=True):
            with self.assertRaisesRegex(ValueError, "does not match the digest"):
                release.report(self.folder)
        self.assertEqual(api.call_args.kwargs["data"]["state"], "failure")

    def test_release_contract_refuses_old_or_noninteger_tool_descriptors(self):
        for meta in ({}, {"release_contract": 0}, {"release_contract": True}, {"release_contract": "1"}):
            with self.subTest(meta=meta), self.assertRaisesRegex(ValueError, "release_contract >= 1"):
                build_release.require_release_contract(meta)
        build_release.require_release_contract({"release_contract": 1})
        build_release.require_release_contract({"release_contract": 2})
