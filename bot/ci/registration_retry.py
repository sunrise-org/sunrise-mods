"""The registration retry: what the registry's answer to a Register issue means, and when the timer asks again.

Two entry points, both run from a checkout of main (see the bottom of the file):

  answer  the last step of register-issue.yml: say what the registry answered, in the issue's one status comment
  retry   a step of timer.yml's `tick`: start register-issue.yml again for an open Register issue with no answer

Importing this module does nothing: no environment, no GitHub call, no sibling import. The GitHub helpers (`api`,
`rows`, `gh`, `safe`, `require`, `BOT`) are release_pr.py's. The entry point imports that module and hands it to
`answer` and `retry`, so there is one copy of them and no process between the two scripts.
"""
import collections
import json
import os
import re
import sys
from pathlib import Path

WORKFLOW = "register-issue.yml"
# The ruling: "at most five interval attempts after the original, one per tick". An attempt is a run started by
# dispatch, counted from GitHub's history of this workflow's runs. "One per tick" is the pace of the serialized `tick`
# job in timer.yml, which holds its concurrency group for the ten minutes between ticks.
ATTEMPTS = 5
FORM = "https://playsunrise.net/mods/submit"
# The workflow's `run-name`: the one place the issue's number is machine-readable on a run.
RUN_TITLE = re.compile(r"Registration (?:of|retry for) issue (?P<issue>[1-9][0-9]*)", re.ASCII)
ISSUE = re.compile(r"[1-9][0-9]*", re.ASCII)
# The first line of the bot's one status comment on an issue: the run that wrote it, and what that run learned.
#   retry       a typed answer: asking again can change it
#   unanswered  no usable answer (no connection, an edge's page, a bare 403, JSON of another shape)
#   done        a definite answer
#   exhausted   written by the timer: no attempt is left
MARKER = re.compile(r"<!-- registry-v2-registration run=(?P<run>[0-9]+) state=(?P<state>retry|unanswered|done|exhausted) -->",
                    re.ASCII)
CODE = re.compile(r"[a-z_]+", re.ASCII)
# Each definite problem the issue route answers, with whether an edit of the issue can fix it.
PROBLEMS = {
    "repository_url": ("The Repository field must hold one link of the form `https://github.com/<owner>/<repository>`.", True),
    "repository_missing": ("GitHub could not find the repository. Check the link, and that the repository is public.", True),
    "repository_private": ("The repository must be public.", True),
    "repository_admin": ("An issue can register only a repository that belongs to the personal account that opened it. "
                         f"For a repository of an organisation, or of another account, an admin of the repository registers it with the form at {FORM}.",
                         True),
    "standing": ("This account cannot register a mod right now.", False),
}
REASONS = {
    "rate": "this account has made too many checks lately",
    "busy": "the registry is checking other requests",
    "github": "GitHub did not answer",
    "registry": "the registry could not finish",
    "write": "the registry could not get permission to write",
    "registry_changed": "the registry changed while the request was checked",
    "standing": "the registry could not read this account's standing",
}
AGAIN = f"It checks again by itself, up to {ATTEMPTS} more times, ten minutes apart. There is nothing for you to change."
UNANSWERED = f"The registry did not answer this check. {AGAIN}"
EXHAUSTED = (f"The registry could not check this repository after the first try and {ATTEMPTS} more. Automatic checks have "
             "stopped, and this shows as a failed run on the registry's Actions page. There is nothing for you to change.")


def reply(status, answer):
    """The registry's HTTP status and decoded JSON body, as (state, comment text or None, unexplained).

    The only reader of the untrusted answer: anything that is not exactly one of the service's typed answers is
    `unanswered`. `unexplained` is an answer a maintainer should look at: no usable answer, or a definite code this
    file does not know.
    """
    match answer:
        case {"registration": int() as number} if status == "200" and type(number) is int and number > 0:
            return "done", (f"The repository passed the registry's checks: its registration is #{number}, which closes this issue "
                            "when it merges. Registering lists nothing by itself: a project appears after its first release "
                            "through `cargo sunrise-mod release` is reviewed and merged."), False
        case {"problem": str() as code} if status.startswith("4") and CODE.fullmatch(code):
            if code == "issue":
                # Closed, retitled or not a registration issue: nothing to say on it.
                return "done", None, False
            if code == "already_registered":
                return "done", "This repository is already registered. Nothing more is needed.", False
            if code == "already_pending":
                return "done", "This repository already has an open registration pull request. Nothing more is needed.", False
            if code in PROBLEMS:
                problem, editable = PROBLEMS[code]
                return "done", (f"This repository could not be registered. {problem}"
                                + (f"\n\nAfter fixing it, edit this issue to have it checked again, or use the form at {FORM}."
                                   if editable else "")), False
            return "done", f"The registry refused this request (`{code}`).", True
        case {"retry": str() as code} if status == "200" and CODE.fullmatch(code):
            reason = f" ({REASONS[code]})" if code in REASONS else ""
            return "retry", f"The registry could not finish checking this repository yet{reason}. {AGAIN}", False
    return "unanswered", UNANSWERED, True


def read_comments(github, issue):
    """What the bot has said on the issue: (its status comment as {"id", "run", "state"} or None, whether it said anything).

    Only the bot's own comments count, so a copied marker in anyone else's comment decides nothing.
    """
    ours = [comment for comment in github.rows(f"issues/{issue}/comments?per_page=100") if comment["user"]["login"] == github.BOT]
    for comment in ours:
        if found := MARKER.match(comment["body"]):
            return {"id": comment["id"], "run": int(found["run"]), "state": found["state"]}, True
    return None, bool(ours)


def say(github, issue, run, state, text, existing):
    """Writes the issue's one status comment: created once, then edited in place."""
    body = f"<!-- registry-v2-registration run={run} state={state} -->\n🤖 Agent: {text}\n"
    if existing:
        github.api(f"issues/comments/{existing['id']}", "-X", "PATCH", data={"body": body})
    else:
        github.api(f"issues/{issue}/comments", "-X", "POST", data={"body": body})


def answer(github):
    """The register-issue run's last step: write what the registry answered, and go red when a maintainer should look.

    The first unexplained red of a cycle is the maintainers' notice. A dispatch that again gets no answer, right after
    a run that got none, writes the same comment and stays green; the run's log has the status either way.
    """
    issue = os.environ["ISSUE"]
    github.require(ISSUE.fullmatch(issue), "the issue is not a positive number")
    status = os.environ["STATUS"]
    if not re.fullmatch(r"[0-9]+", status):
        status = "000"
    try:
        with open(os.environ["ANSWER"], "rb") as stream:
            body = json.load(stream)
    except (OSError, ValueError, RecursionError):
        body = None
    state, text, unexplained = reply(status, body)
    print(f"The registry answered {status}: {state}{', unexplained' if unexplained else ''}")
    existing = None
    if text is not None:
        existing, _ = read_comments(github, issue)
        say(github, issue, os.environ["GITHUB_RUN_ID"], state, text, existing)
    repeated = (state == "unanswered" and os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch"
                and existing is not None and existing["state"] == "unanswered")
    github.require(not unexplained or repeated, f"the registry's answer ({status}) is not one the workflow understands")


def plan(runs, opener, marker, commented):
    """What the timer does for one open Register issue: (action, id of the cycle's newest run or None).

    `runs` are this issue's runs from GitHub's history of the workflow, in any order. A run is identified and ordered by
    its id, never by a timestamp; a re-run of a run keeps its id. `opener` is the live issue's `user.id`. `marker` is
    the bot's status comment (see `read_comments`) and `commented` whether the bot said anything on the issue. The
    action is:

      stop       answered, or nothing to retry
      wait       the newest run has not finished
      dispatch   start another run
      exhausted  the retries are used up; the timer says so

    A cycle is the opener's newest own `issues` run and the dispatches after it, so an edit by the opener starts a new
    one. The opener's run is the one whose original `actor.id` is `opener`. `triggering_actor` is never read: it
    names whoever last re-ran the run, while `actor` stays the account that caused the first attempt. A run counts
    only when it ran on main, and an `issues` run only when it is the opener's, so a maintainer's edit or re-run and a
    dispatch from another branch, skipped or re-run, neither start a cycle nor use an attempt. Of the runs left, an
    attempt-one run that Actions skipped registered nothing and is dropped; a skipped later attempt is a re-run of a
    run that did run (the workflow refuses a re-run), which keeps its id and stays.
    """
    runs = sorted((run for run in runs if run["head_branch"] == "main"
                   and (run["event"] != "issues" or (run["actor"] or {}).get("id") == opener)
                   and (run["conclusion"] != "skipped" or run["run_attempt"] > 1)), key=lambda run: run["id"])
    start = max((run["id"] for run in runs if run["event"] == "issues"), default=0)
    cycle = [run for run in runs if run["id"] >= start]
    if not cycle:
        # No run at all: the opener's event never started one, unless the workflow from before this one answered.
        return ("stop" if commented else "dispatch"), None
    newest = cycle[-1]
    if newest["status"] != "completed":
        return "wait", newest["id"]
    state = marker["state"] if marker and marker["run"] == newest["id"] else None
    # A green run with no comment of its own is the registry's silent answer (an issue that is closed or not a
    # registration); a red, cancelled or timed-out one is a run that was lost before it spoke.
    if state in ("done", "exhausted") or (state is None and newest["conclusion"] == "success"):
        return "stop", newest["id"]
    if sum(run["event"] == "workflow_dispatch" for run in cycle) >= ATTEMPTS:
        return "exhausted", newest["id"]
    return "dispatch", newest["id"]


def runs_by_issue(github):
    """GitHub's whole history of this workflow's runs, grouped by the issue number in the run's title.

    Never filtered: a list filtered by event, branch, actor, status or date stops at GitHub's total cap, and the
    runs past it would read as no history. A history shorter than GitHub says it is stays an error here.
    """
    pages = github.api(f"actions/workflows/{WORKFLOW}/runs?per_page=100", "--paginate", "--slurp")
    github.require(pages, "GitHub returned no page of workflow runs")
    runs = {run["id"]: run for page in pages for run in page["workflow_runs"]}
    github.require(len(runs) >= pages[0]["total_count"], "GitHub's list of workflow runs is shorter than its own total")
    history = collections.defaultdict(list)
    for run in runs.values():
        if titled := RUN_TITLE.fullmatch(run["display_title"]):
            history[int(titled["issue"])].append(run)
    return history


def retry(github):
    """The timer's look at the open Register issues, once per tick: at most one dispatch for each.

    A failure on one issue does not hide the others. The step is red for each one that needs a maintainer, and for a
    look that could not read the history, but it never dispatches on a history it could not read.
    """
    repo = os.environ["GITHUB_REPOSITORY"]
    # Issues first, then the history: an issue found here already has its event's run in the list read after it.
    issues = [issue for issue in github.rows("issues?state=open&per_page=100")
              if "pull_request" not in issue and issue["title"].startswith("Register ")]
    history = runs_by_issue(github)
    problems = []
    for issue in issues:
        number = issue["number"]
        try:
            marker, commented = read_comments(github, number)
            action, newest = plan(history.get(number, []), issue["user"]["id"], marker, commented)
            if action != "stop":
                print(f"#{number}: {action}")
            if action == "dispatch":
                if newest is not None and (marker is None or marker["run"] != newest):
                    # The newest run ended without saying anything: say that it will be asked again, then ask.
                    say(github, number, newest, "unanswered", UNANSWERED, marker)
                github.gh("workflow", "run", WORKFLOW, "--repo", repo, "--ref", "main", "-f", f"issue={number}")
            elif action == "exhausted":
                say(github, number, newest, "exhausted", EXHAUSTED, marker)
                problems.append(f"#{number} got no answer from the registry after the first try and {ATTEMPTS} more")
        except (OSError, ValueError, KeyError) as error:
            problems.append(f"#{number}: {error}")
    for problem in problems:
        print(f"::error::{github.safe(problem)}")
    github.require(not problems, "; ".join(problems))


if __name__ == "__main__":
    # `python3 -I` leaves this file's folder off sys.path. release_pr.py beside it is trusted code of the same checkout
    # of main, so it is added after the standard library: it can shadow nothing.
    sys.path.append(str(Path(__file__).resolve().parent))
    import release_pr

    try:
        match sys.argv[1:]:
            case ["answer"]:
                answer(release_pr)
            case ["retry"]:
                retry(release_pr)
            case _:
                raise ValueError("usage: registration_retry.py answer|retry")
    except (OSError, ValueError, KeyError, TypeError) as error:
        sys.exit("registry registration: " + release_pr.safe(error))
