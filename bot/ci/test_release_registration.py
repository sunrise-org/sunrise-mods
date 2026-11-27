"""The registration retry: what the registry's answer means, and what the timer does from GitHub's history of runs.

Nothing here touches the network: `api`, `rows` and `gh` are replaced.
"""
from datetime import datetime, timedelta, timezone
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, call, patch

import release_pr as release

REPO = "sunrise-org/sunrise-mods"
NOW = datetime(2026, 11, 27, 12, 0, 0, tzinfo=timezone.utc)
OPENER = 7


def ago(seconds):
    return (NOW - timedelta(seconds=seconds)).strftime("%Y-%m-%dT%H:%M:%SZ")


def issue(number=9, created=3600, title="Register alice/example"):
    return {"number": number, "title": title, "created_at": ago(created), "user": {"id": OPENER}}


def run(id, event, age, status="completed", conclusion="failure", actor=OPENER, number=9):
    title = f"Registration retry for issue {number}" if event == "workflow_dispatch" else f"Registration of issue {number}"
    return {"id": id, "event": event, "created_at": ago(age), "status": status, "conclusion": conclusion,
            "actor": {"id": actor}, "display_title": title}


def original(age=3000, **kwargs):
    return run(100, "issues", age, **kwargs)


def dispatch(id, age, **kwargs):
    return run(id, "workflow_dispatch", age, **kwargs)


def marker(run_id, state, user=release.BOT):
    return {"id": 1, "user": {"login": user}, "body": f"<!-- registry-v2-registration run={run_id} state={state} -->\n🤖 Agent: x\n"}


def plan(runs, comments=(), number=9, created=3600):
    return release.registration_plan(issue(number, created), runs, list(comments), NOW.timestamp())


class Reply(unittest.TestCase):
    def test_a_typed_answer_is_explained_and_anything_else_is_unanswered_and_red(self):
        cases = (
            ("200", {"registration": 12}, "done", True, False),
            ("409", {"problem": "already_registered"}, "done", True, False),
            ("409", {"problem": "already_pending"}, "done", True, False),
            ("400", {"problem": "repository_url"}, "done", True, False),
            ("403", {"problem": "repository_admin"}, "done", True, False),
            ("403", {"problem": "standing"}, "done", True, False),
            ("404", {"problem": "repository_missing"}, "done", True, False),
            # A refused dispatch (closed, retitled): definite, and nothing to say.
            ("404", {"problem": "issue"}, "done", False, False),
            # A definite code the workflow does not know: stops, but red so it is seen.
            ("400", {"problem": "something_new"}, "done", True, True),
            ("200", {"retry": "rate"}, "retry", True, False),
            ("200", {"retry": "busy"}, "retry", True, False),
            ("200", {"retry": "write"}, "retry", True, False),
            ("000", None, "retry", True, True),
            ("502", None, "retry", True, True),
            ("403", None, "retry", True, True),
            ("200", {}, "retry", True, True),
            ("200", [], "retry", True, True),
            ("200", {"registration": True}, "retry", True, True),
            ("200", {"registration": 0}, "retry", True, True),
            ("200", {"registration": "12"}, "retry", True, True),
            ("502", {"retry": "github"}, "retry", True, True),
            ("200", {"problem": "repository_url"}, "retry", True, True),
            ("400", {"problem": "Not A Code\n::error::"}, "retry", True, True),
            ("500", {"problem": "repository_url"}, "retry", True, True),
        )
        for status, answer, state, text, red in cases:
            with self.subTest(status=status, answer=answer):
                got = release.registration_reply(status, answer)
                self.assertEqual((got[0], got[1] is not None, got[2]), (state, text, red))

    def test_what_the_comment_says_to_the_author(self):
        said = lambda status, answer: release.registration_reply(status, answer)[1]
        self.assertIn("#12", said("200", {"registration": 12}))
        self.assertIn("already registered", said("409", {"problem": "already_registered"}))
        # A definite problem asks for an edit; a retry and an unanswered check never do.
        self.assertIn("edit this issue", said("400", {"problem": "repository_url"}))
        for text in (said("200", {"retry": "rate"}), said("000", None)):
            self.assertIn("nothing for you to change", text.lower())
            self.assertNotIn("edit this issue", text)
        self.assertIn("maintainers have been told", said("000", None))
        self.assertNotIn("maintainers", said("200", {"retry": "rate"}))


class Answer(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.answer = Path(directory.name) / "answer.json"
        environment = patch.dict(os.environ, ISSUE="9", GITHUB_RUN_ID="555", GITHUB_REPOSITORY=REPO,
                                 ANSWER=str(self.answer), STATUS="200")
        environment.start()
        self.addCleanup(environment.stop)

    def answered(self, comments, **env):
        with patch.dict(os.environ, env), patch.object(release, "rows", return_value=comments) as rows, \
                patch.object(release, "api") as api:
            release.registration_answer()
        rows.assert_called_once_with("issues/9/comments?per_page=100")
        return api

    def test_a_registration_writes_one_comment_and_the_run_stays_green(self):
        self.answer.write_text('{"registration": 12}')
        api = self.answered([])
        self.assertEqual(api.call_args.args, ("issues/9/comments", "-X", "POST"))
        body = api.call_args.kwargs["data"]["body"]
        self.assertTrue(body.startswith("<!-- registry-v2-registration run=555 state=done -->\n🤖 Agent: "))
        self.assertIn("#12", body)

    def test_the_bots_comment_is_edited_and_another_users_copy_of_the_marker_is_ignored(self):
        self.answer.write_text('{"retry": "busy"}')
        spoof = marker(1, "done", user="mallory")
        spoof["id"] = 50
        api = self.answered([spoof])
        self.assertEqual(api.call_args.args, ("issues/9/comments", "-X", "POST"))
        ours = marker(400, "retry")
        ours["id"] = 77
        api = self.answered([spoof, ours])
        self.assertEqual(api.call_args.args, ("issues/comments/77", "-X", "PATCH"))
        self.assertIn("run=555 state=retry", api.call_args.kwargs["data"]["body"])

    def test_a_refused_dispatch_says_nothing(self):
        self.answer.write_text('{"problem": "issue"}')
        with patch.dict(os.environ, STATUS="404"), patch.object(release, "rows") as rows, patch.object(release, "api") as api:
            release.registration_answer()
        rows.assert_not_called()
        api.assert_not_called()

    def test_no_usable_answer_comments_and_goes_red(self):
        for status, body in (("000", None), ("502", "<html>bad gateway</html>"), ("200", "{}"), ("403", ""), ("abc", "{}")):
            with self.subTest(status=status):
                self.answer.unlink(missing_ok=True)
                if body is not None:
                    self.answer.write_text(body)
                with patch.dict(os.environ, STATUS=status), patch.object(release, "rows", return_value=[]), \
                        patch.object(release, "api") as api:
                    with self.assertRaisesRegex(ValueError, "not one the workflow understands"):
                        release.registration_answer()
                self.assertIn("state=retry", api.call_args.kwargs["data"]["body"])

    def test_an_unknown_definite_code_stops_but_goes_red(self):
        self.answer.write_text('{"problem": "something_new"}')
        with patch.dict(os.environ, STATUS="400"), patch.object(release, "rows", return_value=[]), \
                patch.object(release, "api") as api:
            with self.assertRaises(ValueError):
                release.registration_answer()
        self.assertIn("state=done", api.call_args.kwargs["data"]["body"])

    def test_a_bad_issue_number_is_refused_before_anything_is_written(self):
        with patch.dict(os.environ, ISSUE="9; rm -rf /"), patch.object(release, "api") as api:
            with self.assertRaisesRegex(ValueError, "positive number"):
                release.registration_answer()
        api.assert_not_called()


class Plan(unittest.TestCase):
    def action(self, *args, **kwargs):
        return plan(*args, **kwargs)[0]

    def test_nothing_ran_is_unanswered_unless_the_workflow_before_this_one_answered(self):
        self.assertEqual(plan([]), ("dispatch", None, False))
        old = {"id": 3, "user": {"login": release.BOT}, "body": "🤖 Agent: The registry could not check this issue right now."}
        self.assertEqual(plan([], [old])[0], "stop")
        # Another user's comment is not an answer.
        self.assertEqual(plan([], [{"id": 3, "user": {"login": "bob"}, "body": "hello"}])[0], "dispatch")

    def test_a_definite_answer_or_a_green_run_with_nothing_to_say_stops_it(self):
        self.assertEqual(self.action([original(conclusion="success")]), "stop")
        self.assertEqual(self.action([original(conclusion="success")], [marker(100, "done")]), "stop")
        # Red, but it had said a definite thing (a code the workflow does not know).
        self.assertEqual(self.action([original()], [marker(100, "done")]), "stop")
        # A marker left by an earlier cycle does not decide this one.
        self.assertEqual(self.action([original(conclusion="success")], [marker(5, "retry")]), "stop")
        self.assertEqual(self.action([original()], [marker(5, "done")]), "dispatch")

    def test_an_explained_retry_an_unexplained_red_and_a_lost_worker_are_asked_again(self):
        self.assertEqual(plan([original(conclusion="success")], [marker(100, "retry")]), ("dispatch", original(conclusion="success"), True))
        for conclusion in ("failure", "cancelled", "timed_out"):
            with self.subTest(conclusion=conclusion):
                action, newest, answered = plan([original(conclusion=conclusion)])
                self.assertEqual((action, newest["id"], answered), ("dispatch", 100, False))

    def test_a_run_in_progress_is_waited_for_until_it_is_stale(self):
        self.assertEqual(self.action([original(age=60, status="in_progress", conclusion=None)]), "wait")
        self.assertEqual(self.action([original(age=60, status="queued", conclusion=None)]), "wait")
        self.assertEqual(self.action([original(age=1300, status="in_progress", conclusion=None)]), "dispatch")
        # A fresh dispatch is waited for too: one at a time per issue.
        self.assertEqual(self.action([original(), dispatch(201, 30, status="queued", conclusion=None)]), "wait")

    def test_one_timer_interval_is_not_counted_twice(self):
        first = dispatch(201, 100)
        # A second look 100 s after the dispatch (a push-started timer run) starts nothing more.
        self.assertEqual(self.action([original(), first]), "wait")
        # The next tick, ten minutes later, does.
        self.assertEqual(self.action([original(), dispatch(201, 600)]), "dispatch")
        # The original run is not an interval: its failure is retried at the next tick however soon.
        self.assertEqual(self.action([original(age=20)]), "dispatch")

    def test_five_dispatches_after_the_original_is_the_limit_and_exhaustion_is_reported_once(self):
        fours = [dispatch(200 + i, 2400 - 600 * i) for i in range(4)]
        self.assertEqual(self.action([original(age=2700), *fours]), "dispatch")
        fives = [*fours, dispatch(204, 600)]
        action, newest, answered = plan([original(age=2700), *fives])
        self.assertEqual((action, newest["id"], answered), ("exhausted", 204, False))
        # Once the comment says so, it stops without a second notice.
        self.assertEqual(self.action([original(age=2700), *fives], [marker(204, "exhausted")]), "stop")
        # The fifth still running is waited for before it is judged.
        running = dispatch(204, 100, status="in_progress", conclusion=None)
        self.assertEqual(self.action([original(age=2700), *fours, running]), "wait")
        # A lost worker counts like any other: no comment ever came from the runs, and it still stops at five.
        self.assertEqual(self.action([original(age=5000), *[dispatch(300 + i, 4000 - 600 * i, conclusion="cancelled") for i in range(5)]]), "exhausted")

    def test_a_manual_dispatch_is_an_attempt_too(self):
        manual = [dispatch(300 + i, 3000 - 600 * i, actor=99) for i in range(5)]
        self.assertEqual(self.action([original(age=3500), *manual]), "exhausted")

    def test_the_openers_edit_starts_a_new_cycle(self):
        old = [original(age=9000), *[dispatch(200 + i, 8000 - 600 * i) for i in range(5)]]
        self.assertEqual(self.action(old), "exhausted")
        edited = run(101, "issues", 300)
        self.assertEqual(plan([*old, edited])[0:2], ("dispatch", edited))
        # Edited and answered green: stopped, whatever the older dispatches were.
        self.assertEqual(self.action([*old, run(101, "issues", 300, conclusion="success")]), "stop")

    def test_only_the_openers_own_unskipped_event_starts_a_cycle(self):
        # Someone else's edit, and a run Actions skipped, start nothing.
        stranger = run(102, "issues", 100, actor=99, conclusion="success")
        skipped = run(103, "issues", 90, conclusion="skipped")
        action, newest, _ = plan([original(), stranger, skipped])
        self.assertEqual((action, newest["id"]), ("dispatch", 100))
        # A skipped dispatch (it ran from another branch) is not an attempt.
        self.assertEqual(plan([original(), dispatch(201, 700, conclusion="skipped")])[1]["id"], 100)

    def test_dispatches_before_the_newest_original_do_not_count(self):
        runs = [dispatch(200 + i, 5000 - 100 * i) for i in range(5)]
        self.assertEqual(self.action([*runs, original(age=300)]), "dispatch")


class Retry(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
        environment.start()
        self.addCleanup(environment.stop)
        clock = patch.object(release, "time", Mock(time=Mock(return_value=NOW.timestamp())))
        clock.start()
        self.addCleanup(clock.stop)

    def look(self, issues, runs, comments=None, gh=None):
        comments = comments or {}

        def rows(endpoint):
            if endpoint.startswith("issues?"):
                return issues
            return comments.get(int(endpoint.split("/")[1]), [])

        api = Mock(return_value=[{"workflow_runs": runs}])
        gh = gh or Mock()
        post = Mock()
        with patch.object(release, "rows", side_effect=rows), patch.object(release, "gh", gh), \
                patch.object(release, "api", api), patch.object(release, "upsert_registration_comment", post):
            try:
                release.registration_retry()
                error = None
            except ValueError as raised:
                error = str(raised)
        return api, gh, post, error

    def test_it_starts_one_run_for_each_unanswered_open_register_issue(self):
        issues = [issue(9), issue(10, title="Register bob/other"), issue(11, title="A question"),
                  {**issue(12), "pull_request": {}}]
        runs = [original(conclusion="failure"), run(110, "issues", 800, conclusion="success", number=10)]
        api, gh, post, error = self.look(issues, runs)
        self.assertIsNone(error)
        # The history is read once, from a day back, for the registration workflow only.
        endpoint = api.call_args.args[0]
        self.assertTrue(endpoint.startswith("actions/workflows/register-issue.yml/runs?per_page=100&created=%3E%3D2026-11-26T12%3A00%3A00Z"), endpoint)
        self.assertEqual(gh.call_args_list, [call("workflow", "run", "register-issue.yml", "--repo", REPO, "--ref", "main", "-f", "issue=9")])
        # The run for #9 was lost before it spoke: the comment says so before the retry.
        post.assert_called_once_with(9, 100, "retry", release.REGISTRATION_UNANSWERED)

    def test_an_answered_run_leaves_its_own_comment_alone(self):
        runs = [original(conclusion="success")]
        _, gh, post, error = self.look([issue(9)], runs, {9: [marker(100, "retry")]})
        self.assertIsNone(error)
        gh.assert_called_once()
        post.assert_not_called()

    def test_an_old_issue_with_no_runs_in_the_window_is_not_looked_at(self):
        api, gh, post, error = self.look([issue(9, created=90000)], [])
        self.assertIsNone(error)
        gh.assert_not_called()
        # While a new one the event never reached is asked about.
        _, gh, _, _ = self.look([issue(9, created=300)], [])
        gh.assert_called_once()

    def test_exhaustion_is_written_once_and_the_look_goes_red_for_it(self):
        runs = [original(age=2700), *[dispatch(200 + i, 2400 - 600 * i) for i in range(5)]]
        _, gh, post, error = self.look([issue(9)], runs)
        gh.assert_not_called()
        post.assert_called_once_with(9, 204, "exhausted", release.REGISTRATION_EXHAUSTED)
        self.assertRegex(error, r"#9 got no answer .* 5 more")
        # The next look finds the notice in the comment and is quiet.
        _, gh, post, error = self.look([issue(9)], runs, {9: [marker(204, "exhausted")]})
        self.assertIsNone(error)
        gh.assert_not_called()
        post.assert_not_called()

    def test_a_failure_on_one_issue_does_not_hide_the_others(self):
        runs = [original(), run(110, "issues", 800, number=10)]
        gh = Mock(side_effect=[release.CommandError("gh: HTTP 502"), b""])
        _, gh, _, error = self.look([issue(9), issue(10)], runs, gh=gh)
        self.assertEqual(gh.call_count, 2)
        self.assertEqual(error, "#9: gh: HTTP 502")

    def test_a_failure_to_read_the_history_is_red_and_dispatches_nothing(self):
        gh = Mock()
        with patch.object(release, "api", side_effect=release.CommandError("gh: HTTP 500")), patch.object(release, "gh", gh):
            with self.assertRaisesRegex(ValueError, "HTTP 500"):
                release.registration_retry()
        gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
