"""Observe an already merged PR. This path never asks GitHub to merge or delete a ref."""

import json
import os
import re

from .base import Fail, git
from .github import gh, mid_turn, pr_pending
from .settings import read_settings


PR_URL = re.compile(r"https://github\.com/([\w.-]+/[\w.-]+)/pull/([1-9][0-9]*)", re.ASCII)
SHA = re.compile(r"[0-9a-f]{40}")
PROTECTED = re.compile(r"(^|[/_-])(my-pi|pii?|pi-[^/]*|[^/]*harness[^/]*)(/|$)|harness|vanilla|(^|/)\.pi(/|$)", re.I)


def enabled(repo):
    repos = read_settings().get("auto_accept_merged_repos")
    return (isinstance(repo, str) and not PROTECTED.search(repo) and isinstance(repos, list)
            and any(isinstance(r, str) and r.casefold() == repo.casefold() for r in repos))


def bind_prs(urls):
    """Bind manual PRs to the heads the worker said were ready, not to future pushes."""
    prs = []
    for url in dict.fromkeys(urls):
        m = PR_URL.fullmatch(url)
        if not m:
            raise Fail(f"Expected a GitHub pull request URL for --closeout-pr, not {url!r}.")
        repo, number = m[1].lower(), int(m[2])
        if not enabled(repo):
            raise Fail(f"{repo} is not enabled for automatic closeout; report Ready without --closeout-pr.")
        ok, out, _ = gh("pr", "view", str(number), "--repo", repo, "--json", "headRefOid")
        try:
            head = json.loads(out).get("headRefOid") if ok else None
        except (ValueError, AttributeError):
            head = None
        if not isinstance(head, str) or not SHA.fullmatch(head):
            raise Fail(f"Couldn't verify the head of {repo}#{number}. Check GitHub and run ready again.")
        prs.append({"number": number, "repo": repo, "url": url, "head": head})
    return prs


def ready_for_closeout(s, task, prs):
    return (task["state"] == "ready" and not task.get("kept_open") and bool(prs)
            and all(enabled(pr.get("repo")) for pr in prs)
            and not any(PROTECTED.search(os.path.realpath(path)) for path in (s.loc["folder"], task.get("worktree")) if path)
            and not mid_turn(s, task))


def green_merge(v, head):
    """GitHub's check rollup for the PR head must contain a pass and nothing unresolved."""
    if not v or v.get("state") != "MERGED" or not (v.get("mergeCommit") or {}).get("oid"):
        return False
    if not head or v.get("headRefOid") != head or v.get("checks_unknown") or pr_pending(v):
        return False
    checks = v.get("statusCheckRollup") or []
    results = {c.get("conclusion") or c.get("state") for c in checks}
    return "SUCCESS" in results and results <= {"SUCCESS", "NEUTRAL", "SKIPPED"}


def worktree_head(s, task):
    """The reported head must still be the local task branch and checked-out head, if present."""
    wt, branch, head = task.get("worktree"), task.get("branch"), task.get("head")
    if not (wt and branch and head):
        return None
    ok, tip, _ = git(s.loc["folder"], "rev-parse", "--verify", f"refs/heads/{branch}")
    if not ok or tip != head:
        return None
    if os.path.lexists(wt):
        ok, tip, _ = git(wt, "rev-parse", "--verify", "HEAD")
        if not ok or tip != head:
            return None
    return head
