"""The registration retry: what the registry's answer means, and what the timer does from GitHub's history of runs.

Nothing here touches the network: the GitHub helpers of release_pr.py are replaced.
"""
import inspect
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import Mock, patch

import registration_retry as registration
import release_pr as release

REPO = "sunrise-org/sunrise-mods"
ROOT = Path(__file__).resolve().parents[2]
# Every run is old: nothing in the policy may depend on how old a run is.
LONG_AGO = "2020-01-01T00:00:00Z"
OPEN = ("queued", "in_progress", "waiting", "requested", "pending")


def run(id, event="issues", status="completed", conclusion="failure", number=9, attempt=1, created=LONG_AGO, title=None):
    kind = "Registration retry for issue" if event == "workflow_dispatch" else "Registration of issue"
    return {"id": id, "event": event, "status": status, "conclusion": conclusion, "run_attempt": attempt,
            "created_at": created, "display_title": title or f"{kind} {number}"}


def dispatch(id, **kwargs):
    return run(id, "workflow_dispatch", **kwargs)


def unfinished(id, event="issues", status="in_progress"):
    return run(id, event, status=status, conclusion=None)


def marker(run_id, state, id=77):
    return {"id": id, "run": run_id, "state": state}


def comment(run_id, state, user=release.BOT, id=77):
    return {"id": id, "user": {"login": user}, "body": f"<!-- registry-v2-registration run={run_id} state={state} -->\n🤖 Agent: x\n"}


def issue(number=9, title="Register alice/example", **extra):
    return {"number": number, "title": title, **extra}


def action(runs, marker=None, commented=False):
    return registration.plan(runs, marker, commented)


class Module(unittest.TestCase):
    def test_importing_it_does_nothing_and_the_policy_has_no_clock(self):
        # No sibling import at the top: the helpers are handed in, so the module is the same under `python3 -I`.
        self.assertFalse(hasattr(registration, "release_pr"))
        self.assertEqual(list(inspect.signature(registration.plan).parameters), ["runs", "marker", "commented"])
        self.assertEqual(registration.ATTEMPTS, 5)


class Plan(unittest.TestCase):
    def test_no_run_is_asked_about_unless_the_workflow_before_this_one_answered(self):
        self.assertEqual(action([]), ("dispatch", None))
        self.assertEqual(action([], commented=True), ("stop", None))
        # Runs Actions skipped registered nothing, so they are no run either.
        self.assertEqual(action([run(100, conclusion="skipped")]), ("dispatch", None))
        self.assertEqual(action([run(100, conclusion="skipped")], commented=True), ("stop", None))

    def test_a_run_that_has_not_finished_is_never_retried_however_old_it_is(self):
        for status in OPEN:
            with self.subTest(status=status):
                self.assertEqual(action([unfinished(100, status=status)]), ("wait", 100))
                # A dispatch behind a failed original: the cycle's newest run decides.
                self.assertEqual(action([run(100), unfinished(101, "workflow_dispatch", status)]), ("wait", 101))
                self.assertEqual(action([run(100), unfinished(101, "workflow_dispatch", status)], marker(100, "retry")), ("wait", 101))

    def test_the_last_finished_run_with_no_comment_of_its_own_is_retried(self):
        for conclusion in ("failure", "cancelled", "timed_out", "startup_failure", None):
            with self.subTest(conclusion=conclusion):
                self.assertEqual(action([run(100, conclusion=conclusion)]), ("dispatch", 100))
                # A comment from an earlier run says nothing about this one.
                self.assertEqual(action([run(100, conclusion=conclusion)], marker(5, "done")), ("dispatch", 100))
                self.assertEqual(action([run(100, conclusion=conclusion)], commented=True), ("dispatch", 100))

    def test_an_explained_retry_is_asked_again_even_when_the_run_was_green(self):
        self.assertEqual(action([run(100, conclusion="success")], marker(100, "retry")), ("dispatch", 100))
        self.assertEqual(action([run(100)], marker(100, "retry")), ("dispatch", 100))
        self.assertEqual(action([run(100)], marker(100, "unanswered")), ("dispatch", 100))

    def test_a_definite_answer_and_an_ordinary_no_op_stop_it(self):
        self.assertEqual(action([run(100, conclusion="success")], marker(100, "done")), ("stop", 100))
        # Red after it had said a definite thing (a code the workflow does not know): still stopped.
        self.assertEqual(action([run(100)], marker(100, "done")), ("stop", 100))
        self.assertEqual(action([run(100)], marker(100, "exhausted")), ("stop", 100))
        # The registry's silent answer for a closed or retitled issue: green, nothing written.
        self.assertEqual(action([run(100, conclusion="success")]), ("stop", 100))
        self.assertEqual(action([run(100, conclusion="success"), dispatch(101, conclusion="success")]), ("stop", 101))
        # A stranger's edit that Actions skipped does not reopen it, and does not hide the real newest run.
        self.assertEqual(action([run(100, conclusion="success"), run(101, conclusion="skipped")]), ("stop", 100))
        self.assertEqual(action([run(100), run(101, conclusion="skipped")], marker(100, "retry")), ("dispatch", 100))

    def test_four_dispatches_are_followed_by_a_fifth_and_the_fifth_by_exhaustion_at_any_age(self):
        original = run(100)
        four = [dispatch(101 + i) for i in range(4)]
        self.assertEqual(action([original, *four]), ("dispatch", 104))
        five = [*four, dispatch(105)]
        self.assertEqual(action([original, *five]), ("exhausted", 105))
        # The fifth still running is waited for, not judged, however old it is.
        self.assertEqual(action([original, *four, unfinished(105, "workflow_dispatch")]), ("wait", 105))
        # Exhaustion is said once: the comment for the newest run stops it.
        self.assertEqual(action([original, *five], marker(105, "exhausted")), ("stop", 105))
        # A maintainer's own dispatch is an attempt; its failure is exhausted again, and the timer starts nothing.
        self.assertEqual(action([original, *five, dispatch(106)], marker(105, "exhausted")), ("exhausted", 106))

    def test_lost_runs_and_runs_without_an_original_count_like_any_other(self):
        self.assertEqual(action([run(100), *[dispatch(101 + i, conclusion="cancelled") for i in range(5)]]), ("exhausted", 105))
        self.assertEqual(action([dispatch(100 + i) for i in range(5)]), ("exhausted", 104))
        self.assertEqual(action([dispatch(100 + i) for i in range(4)]), ("dispatch", 103))

    def test_the_openers_own_edit_starts_a_new_cycle(self):
        old = [run(100), *[dispatch(101 + i) for i in range(5)]]
        self.assertEqual(action(old), ("exhausted", 105))
        self.assertEqual(action([*old, run(200)]), ("dispatch", 200))
        self.assertEqual(action([*old, run(200, conclusion="success")]), ("stop", 200))
        self.assertEqual(action([*old, unfinished(200)]), ("wait", 200))
        # Dispatches of the new cycle count from zero.
        self.assertEqual(action([*old, run(200), *[dispatch(201 + i) for i in range(4)]]), ("dispatch", 204))

    def test_runs_that_actions_skipped_are_not_attempts_but_a_rerun_keeps_its_identity(self):
        self.assertEqual(action([run(100), *[dispatch(101 + i, conclusion="skipped") for i in range(5)]]), ("dispatch", 100))
        # An Actions re-run is refused by the workflow, so its attempt is skipped; the run is still the same run.
        rerun = run(100, conclusion="skipped", attempt=2)
        self.assertEqual(action([rerun], marker(100, "retry")), ("dispatch", 100))
        self.assertEqual(action([rerun], marker(100, "done")), ("stop", 100))
        rerun_dispatches = [dispatch(101 + i, conclusion="skipped", attempt=2) for i in range(5)]
        self.assertEqual(action([run(100), *rerun_dispatches]), ("exhausted", 105))

    def test_runs_are_ordered_by_id_never_by_timestamp_or_by_the_order_given(self):
        newer = dispatch(101, status="queued", conclusion=None, created=LONG_AGO)
        older = run(100, created="2030-01-01T00:00:00Z")
        self.assertEqual(action([older, newer]), ("wait", 101))
        self.assertEqual(action([newer, older]), ("wait", 101))
        self.assertEqual(action([dispatch(103), run(100), dispatch(101), dispatch(102)]), ("dispatch", 103))


class Reply(unittest.TestCase):
    def test_a_typed_answer_is_explained_and_anything_else_is_unanswered(self):
        cases = (
            ("200", {"registration": 12}, "done", True, False),
            ("409", {"problem": "already_registered"}, "done", True, False),
            ("409", {"problem": "already_pending"}, "done", True, False),
            ("400", {"problem": "repository_url"}, "done", True, False),
            ("403", {"problem": "repository_admin"}, "done", True, False),
            ("403", {"problem": "repository_private"}, "done", True, False),
            ("403", {"problem": "standing"}, "done", True, False),
            ("404", {"problem": "repository_missing"}, "done", True, False),
            # A refused dispatch (closed, retitled): definite, and nothing to say.
            ("404", {"problem": "issue"}, "done", False, False),
            # A definite code the workflow does not know: stops, but a maintainer should see it.
            ("400", {"problem": "something_new"}, "done", True, True),
            ("200", {"retry": "rate"}, "retry", True, False),
            ("200", {"retry": "busy"}, "retry", True, False),
            ("200", {"retry": "registry_changed"}, "retry", True, False),
            ("200", {"retry": "write"}, "retry", True, False),
            ("200", {"retry": "standing"}, "retry", True, False),
            ("200", {"retry": "something_new"}, "retry", True, False),
            ("000", None, "unanswered", True, True),
            ("502", None, "unanswered", True, True),
            ("403", None, "unanswered", True, True),
            ("200", {}, "unanswered", True, True),
            ("200", [], "unanswered", True, True),
            ("200", {"registration": True}, "unanswered", True, True),
            ("200", {"registration": 0}, "unanswered", True, True),
            ("200", {"registration": "12"}, "unanswered", True, True),
            ("502", {"retry": "github"}, "unanswered", True, True),
            ("200", {"problem": "repository_url"}, "unanswered", True, True),
            ("400", {"problem": "Not A Code\n::error::"}, "unanswered", True, True),
            ("500", {"problem": "repository_url"}, "unanswered", True, True),
        )
        for status, body, state, text, unexplained in cases:
            with self.subTest(status=status, body=body):
                got = registration.reply(status, body)
                self.assertEqual((got[0], got[1] is not None, got[2]), (state, text, unexplained))

    def test_what_the_comment_says_to_the_author(self):
        said = lambda status, body: registration.reply(status, body)[1]
        self.assertIn("#12", said("200", {"registration": 12}))
        self.assertIn("already registered", said("409", {"problem": "already_registered"}))
        # Only a problem the author can fix asks for an edit.
        self.assertIn("edit this issue", said("400", {"problem": "repository_url"}))
        self.assertNotIn("edit this issue", said("403", {"problem": "standing"}))
        for text in (said("200", {"retry": "rate"}), said("000", None), registration.EXHAUSTED):
            self.assertIn("nothing for you to change", text.lower())
            self.assertNotIn("edit this issue", text)
        # No notification is promised: nothing here sends one.
        for text in (said("000", None), said("200", {"retry": "busy"}), registration.EXHAUSTED):
            self.assertNotIn("told", text)
            self.assertNotIn("alert", text.lower())


class Answer(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.answer = Path(directory.name) / "answer.json"
        environment = patch.dict(os.environ, ISSUE="9", GITHUB_RUN_ID="555", GITHUB_REPOSITORY=REPO, GITHUB_EVENT_NAME="issues",
                                 ANSWER=str(self.answer), STATUS="200")
        environment.start()
        self.addCleanup(environment.stop)

    def answered(self, comments, **env):
        with patch.dict(os.environ, env), patch.object(release, "rows", return_value=comments) as rows, \
                patch.object(release, "api") as api:
            registration.answer(release)
        rows.assert_called_once_with("issues/9/comments?per_page=100")
        return api

    def fails(self, comments, **env):
        with patch.dict(os.environ, env), patch.object(release, "rows", return_value=comments), patch.object(release, "api") as api:
            with self.assertRaisesRegex(ValueError, "not one the workflow understands"):
                registration.answer(release)
        return api

    def test_a_registration_writes_one_comment_and_the_run_stays_green(self):
        self.answer.write_text('{"registration": 12}')
        api = self.answered([])
        api.assert_called_once()
        self.assertEqual(api.call_args.args, ("issues/9/comments", "-X", "POST"))
        body = api.call_args.kwargs["data"]["body"]
        self.assertTrue(body.startswith("<!-- registry-v2-registration run=555 state=done -->\n🤖 Agent: "))
        self.assertIn("#12", body)

    def test_the_bots_comment_is_edited_and_another_users_copy_of_the_marker_is_ignored(self):
        self.answer.write_text('{"retry": "busy"}')
        api = self.answered([comment(1, "done", user="mallory", id=50)])
        self.assertEqual(api.call_args.args, ("issues/9/comments", "-X", "POST"))
        api = self.answered([comment(1, "done", user="mallory", id=50), comment(400, "retry", id=77)])
        self.assertEqual(api.call_args.args, ("issues/comments/77", "-X", "PATCH"))
        self.assertIn("run=555 state=retry", api.call_args.kwargs["data"]["body"])

    def test_a_refused_dispatch_says_nothing_and_reads_nothing(self):
        self.answer.write_text('{"problem": "issue"}')
        with patch.dict(os.environ, STATUS="404"), patch.object(release, "rows") as rows, patch.object(release, "api") as api:
            registration.answer(release)
        rows.assert_not_called()
        api.assert_not_called()

    def test_no_usable_answer_says_so_and_the_first_of_a_cycle_is_red(self):
        for status, body in (("000", None), ("502", "<html>bad gateway</html>"), ("200", "{}"), ("403", ""), ("abc", "{}"), ("", "[]")):
            for event in ("issues", "workflow_dispatch"):
                with self.subTest(status=status, event=event):
                    self.answer.unlink(missing_ok=True)
                    if body is not None:
                        self.answer.write_text(body)
                    api = self.fails([], STATUS=status, GITHUB_EVENT_NAME=event)
                    self.assertIn("state=unanswered", api.call_args.kwargs["data"]["body"])

    def test_a_dispatch_that_again_gets_no_answer_is_green_and_every_other_case_is_red(self):
        self.answer.write_text("<html>")
        for event, previous, red in (
            ("workflow_dispatch", "unanswered", False),
            ("workflow_dispatch", "retry", True),
            ("workflow_dispatch", "exhausted", True),
            ("workflow_dispatch", "done", True),
            ("issues", "unanswered", True),
        ):
            with self.subTest(event=event, previous=previous):
                comments = [comment(400, previous, id=77)]
                if red:
                    api = self.fails(comments, STATUS="502", GITHUB_EVENT_NAME=event)
                else:
                    api = self.answered(comments, STATUS="502", GITHUB_EVENT_NAME=event)
                self.assertEqual(api.call_args.args, ("issues/comments/77", "-X", "PATCH"))
                self.assertIn("state=unanswered", api.call_args.kwargs["data"]["body"])

    def test_an_unknown_definite_code_stops_but_is_always_red(self):
        self.answer.write_text('{"problem": "something_new"}')
        api = self.fails([comment(400, "unanswered")], STATUS="400", GITHUB_EVENT_NAME="workflow_dispatch")
        self.assertIn("state=done", api.call_args.kwargs["data"]["body"])

    def test_a_bad_issue_number_is_refused_before_anything_is_written(self):
        for bad in ("9; rm -rf /", "0", "09", "", "9 "):
            with self.subTest(issue=bad), patch.dict(os.environ, ISSUE=bad), patch.object(release, "api") as api, \
                    patch.object(release, "rows") as rows:
                with self.assertRaisesRegex(ValueError, "positive number"):
                    registration.answer(release)
                api.assert_not_called()
                rows.assert_not_called()


class Retry(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
        environment.start()
        self.addCleanup(environment.stop)

    def look(self, issues, runs, comments=None, gh=None, total=None, history=None):
        """Runs one look. Returns what happened, in order, and the error it ended in: ("rows", endpoint), ("read", endpoint, args),
        ("write", endpoint, body) and ("dispatch", argv)."""
        comments = comments or {}
        events = []

        def rows(endpoint):
            events.append(("rows", endpoint))
            if endpoint.startswith("issues?"):
                return issues
            return comments.get(int(endpoint.split("/")[1]), [])

        def api(endpoint, *args, data=None):
            if endpoint.startswith("actions/"):
                events.append(("read", endpoint, args))
                if history:
                    return history()
                return [{"total_count": len(runs) if total is None else total, "workflow_runs": runs}]
            events.append(("write", endpoint, data["body"]))
            return {}

        gh = gh or Mock(side_effect=lambda *argv: events.append(("dispatch", argv)))
        with patch.object(release, "rows", side_effect=rows), patch.object(release, "api", side_effect=api), patch.object(release, "gh", gh):
            try:
                registration.retry(release)
                error = None
            except ValueError as raised:
                error = str(raised)
        return events, error

    def dispatches(self, events):
        return [argv[-1] for kind, *rest in events if kind == "dispatch" for argv in rest]

    def test_it_reads_open_issues_then_the_whole_unfiltered_history_once(self):
        events, error = self.look([issue(9)], [run(100)])
        self.assertIsNone(error)
        self.assertEqual(events[0], ("rows", "issues?state=open&per_page=100"))
        reads = [event for event in events if event[0] == "read"]
        self.assertEqual(reads, [("read", "actions/workflows/register-issue.yml/runs?per_page=100", ("--paginate", "--slurp"))])
        # Any filter is a cap on how much history GitHub will return.
        for name in ("created", "event", "branch", "actor", "status", "head_sha", "check_suite_id", "exclude_pull_requests"):
            self.assertNotIn(name + "=", reads[0][1])

    def test_one_run_for_each_open_register_issue_with_no_answer_and_nothing_for_the_rest(self):
        issues = [issue(9), issue(10, "Register bob/other"), issue(11, "A question"), issue(12, pull_request={}),
                  issue(13, "Register carol/done"), issue(14, "Register dave/pending")]
        runs = [run(100), run(110, number=10, conclusion="success"), run(130, number=13, conclusion="success"),
                run(140, number=14, status="queued", conclusion=None)]
        events, error = self.look(issues, runs, {13: [comment(130, "done")]})
        self.assertIsNone(error)
        self.assertEqual(self.dispatches(events), ["issue=9"])
        self.assertEqual([event for event in events if event[0] == "dispatch"],
                         [("dispatch", ("workflow", "run", "register-issue.yml", "--repo", REPO, "--ref", "main", "-f", "issue=9"))])

    def test_a_run_lost_before_it_spoke_is_said_so_on_the_issue_before_it_is_asked_again(self):
        events, error = self.look([issue(9)], [run(100)])
        self.assertIsNone(error)
        kinds = [event[0] for event in events if event[0] in ("write", "dispatch")]
        self.assertEqual(kinds, ["write", "dispatch"])
        write = next(event for event in events if event[0] == "write")
        self.assertEqual(write[1], "issues/9/comments")
        self.assertTrue(write[2].startswith("<!-- registry-v2-registration run=100 state=unanswered -->"))

    def test_an_answered_typed_retry_is_asked_again_without_a_new_comment(self):
        events, error = self.look([issue(9)], [run(100, conclusion="success")], {9: [comment(100, "retry")]})
        self.assertIsNone(error)
        self.assertEqual(self.dispatches(events), ["issue=9"])
        self.assertEqual([event for event in events if event[0] == "write"], [])

    def test_a_run_that_is_pending_or_queued_is_not_retried(self):
        for status in OPEN:
            with self.subTest(status=status):
                events, error = self.look([issue(9)], [run(100), unfinished(101, "workflow_dispatch", status)])
                self.assertIsNone(error)
                self.assertEqual([event for event in events if event[0] in ("write", "dispatch")], [])

    def test_the_closure_of_an_issue_and_the_silent_answer_are_ordinary_no_ops(self):
        # A closed issue is not in the open list, so its runs are never looked at.
        events, error = self.look([], [run(100)])
        self.assertIsNone(error)
        self.assertEqual([event for event in events if event[0] in ("write", "dispatch")], [])
        # A green run with no comment (the registry's silent answer) and an issue the older workflow answered.
        events, error = self.look([issue(9), issue(10)], [run(100, conclusion="success")], {10: [{"id": 5, "user": {"login": release.BOT}, "body": "🤖 Agent: done"}]})
        self.assertIsNone(error)
        self.assertEqual([event for event in events if event[0] in ("write", "dispatch")], [])

    def test_an_issue_the_event_never_reached_is_asked_about_once(self):
        events, error = self.look([issue(9)], [])
        self.assertIsNone(error)
        self.assertEqual(self.dispatches(events), ["issue=9"])
        # No run to name: nothing was written on the issue.
        self.assertEqual([event for event in events if event[0] == "write"], [])

    def test_only_a_title_the_workflow_wrote_names_an_issue(self):
        hostile = [run(100 + i, number=9, title=title) for i, title in enumerate((
            "Registration of issue 09", "Registration of issue 9 and more", "registration of issue 9",
            "Registration of issue 9\nRegistration of issue 9", "Register alice/example"))]
        events, _ = self.look([issue(9)], [*hostile, *[dispatch(200 + i, number=10) for i in range(5)]])
        # Nothing of the above is a run for issue 9, and issue 10's attempts are not issue 9's.
        self.assertEqual(self.dispatches(events), ["issue=9"])
        self.assertEqual([event for event in events if event[0] == "write"], [])

    def test_exhaustion_is_written_once_and_the_step_is_red_for_it(self):
        runs = [run(100), *[dispatch(101 + i) for i in range(5)]]
        events, error = self.look([issue(9)], runs)
        self.assertEqual(self.dispatches(events), [])
        write = next(event for event in events if event[0] == "write")
        self.assertTrue(write[2].startswith("<!-- registry-v2-registration run=105 state=exhausted -->"))
        self.assertEqual(error, "#9 got no answer from the registry after the first try and 5 more")
        # The next look finds the comment for the newest run and is quiet.
        events, error = self.look([issue(9)], runs, {9: [comment(105, "exhausted")]})
        self.assertIsNone(error)
        self.assertEqual([event for event in events if event[0] in ("write", "dispatch")], [])

    def test_a_failure_on_one_issue_does_not_hide_the_others(self):
        gh = Mock(side_effect=[release.CommandError("gh: HTTP 502"), b""])
        events, error = self.look([issue(9), issue(10)], [run(100), run(110, number=10)], gh=gh)
        self.assertEqual(gh.call_count, 2)
        self.assertEqual(error, "#9: gh: HTTP 502")

    def test_history_the_look_could_not_read_in_full_dispatches_nothing(self):
        gh = Mock()
        # GitHub says there are more runs than it gave: an error, never an empty history that reads as "no run".
        events, error = self.look([issue(9)], [run(100)], total=2, gh=gh)
        self.assertRegex(error, "shorter than its own total")
        # No page at all, and a failed request.
        events, error = self.look([issue(9)], [], history=lambda: [], gh=gh)
        self.assertRegex(error, "no page")
        failing = Mock(side_effect=release.CommandError("gh: HTTP 500"))
        events, error = self.look([issue(9)], [], history=failing, gh=gh)
        self.assertRegex(error, "HTTP 500")
        gh.assert_not_called()

    def test_the_history_of_every_page_is_read_and_a_run_seen_twice_counts_once(self):
        pages = lambda: [{"total_count": 7, "workflow_runs": [run(100), *[dispatch(101 + i) for i in range(3)]]},
                         {"total_count": 8, "workflow_runs": [dispatch(103), *[dispatch(104 + i) for i in range(3)]]}]
        # Seven different runs: the original and six dispatches, one of them on two pages.
        events, error = self.look([issue(9)], [], history=pages)
        self.assertEqual(self.dispatches(events), [])
        self.assertEqual(error, "#9 got no answer from the registry after the first try and 5 more")


class Workflows(unittest.TestCase):
    def test_the_retry_is_one_step_of_the_serialized_tick_and_stops_no_step_after_it(self):
        text = (ROOT / ".github/workflows/timer.yml").read_text(encoding="utf-8")
        self.assertNotIn("\n  registrations:", text)
        tick = text.split("\n  tick:\n", 1)[1].split("\n  builds:\n", 1)[0]
        header, *steps = tick.split("\n      - ")
        self.assertIn("group: registry-timer", header)
        self.assertEqual(sum("registration_retry.py retry" in step for step in steps), 1)
        # Every step after the first runs unless the run was cancelled, so no failure before or at the retry skips one.
        for step in steps[1:]:
            self.assertIn("!cancelled()", step)
        names = [step.splitlines()[0] for step in steps]
        retry = next(i for i, step in enumerate(steps) if "registration_retry.py retry" in step)
        wait = next(i for i, name in enumerate(names) if "Wait ten minutes" in name)
        self.assertLess(retry, wait)
        # No step hides a failure of the retry.
        self.assertNotIn("continue-on-error", tick)

    def test_the_runs_title_is_what_the_timer_reads(self):
        text = (ROOT / ".github/workflows/register-issue.yml").read_text(encoding="utf-8")
        titles = re.findall(r"format\('([^']*\{0\}[^']*)'", text)
        self.assertEqual(len(titles), 2)
        for title in titles:
            self.assertEqual(registration.RUN_TITLE.fullmatch(title.format(9))["issue"], "9")
        self.assertIn("registration_retry.py answer", text)


if __name__ == "__main__":
    unittest.main()
