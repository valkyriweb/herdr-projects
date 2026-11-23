"""Lanes on another machine (SHEP-2): a Pi task's agent runs on the machine Settings' tasks.pi_machine
names, under this machine's Tasks panel. Its worktree stays here too; the other machine gets the branch
as a git bundle over herdr-core (no GitHub sign-in needed there), the agent commits there, and ready
brings those commits back here, where Herdr pushes and opens the pull request as for any task.

The other machine's herdr-core has no link back here, so the lane's own herdr-task calls (status, ready,
show...) come back the way herdr-secret's do: the lane labels its terminal with a call's nonce and waits
on a FIFO; this machine's tracker for that agent (cmd_track) reads the call, runs it here and writes the
answer into the FIFO."""

import base64
import hashlib
import json
import os
import re
import select
import subprocess
import sys
import tempfile
import time

from .base import ENTRY, Fail, git, guard, hc_json, HOME, now, run, STORE

# Only Pi runs elsewhere: pii is a fork installed on this machine only, and the other CLIs'
# sign-ins and hooks live here.
LANE_KINDS = ("pi",)
HERE = ("here", "local", "this machine")
CALL = "herdr.task.call"
CALLS = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.join(HOME, ".local", "state"), "shepherdr", "lane-calls")
CALL_WAIT = 600
NONCE = re.compile(r"[0-9a-f]{32}")


def placement(task_machine, setting, kind, mode, connected, me):
    """Where a new agent runs: (machine, note). machine None means here; note says why a lane that
    was asked for runs here after all (shown as the task's status), else None. Pure: the rule."""
    want = (task_machine or "").strip()
    if want.lower() in HERE:
        return None, None
    want = want or (setting or "").strip()
    if not want or want == me:
        return None, None
    if kind not in LANE_KINDS:
        return None, (f"Only Pi lanes run on {want}: started on this machine." if task_machine else None)
    if mode != "worktree":
        return None, f"Only a worktree task runs on {want}: started on this machine."
    if want not in connected:
        return None, f"{want} is unreachable: started on this machine."
    return want, None


def place(s, t, kind, space=None):
    """placement() for task t as it starts, from its --set machine and Settings."""
    from .machines import connected_machines, core_name
    from .settings import settings_for
    task_machine = (t.get("set") or {}).get("machine")
    setting = settings_for(s, space)["tasks"].get("pi_machine") or ""
    if not (task_machine or setting).strip() or (task_machine or "").strip().lower() in HERE:
        return None, None
    connected = [m["name"] for m in connected_machines() if m.get("state") == "connected"]
    return placement(task_machine, setting, kind, t.get("mode"), connected, core_name())


def note(t, text):
    """A line in the task's status, as an agent's own status shows."""
    t["progress"] = {"text": text, "at": now(), "set": now(), "placement": True}


def remote_python(machine, script, payload, timeout=120):
    from .machines import _remote_python
    return _remote_python(machine, script, payload, timeout)


PREPARE = r'''
import json, os, subprocess, sys
x = json.load(sys.stdin)
home = os.path.expanduser("~")
mirror = os.path.join(home, x["mirror"])
wt = os.path.join(home, x["worktree"])
def git(*a, cwd=None):
    r = subprocess.run(["git", *a], cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if r.returncode != 0:
        raise SystemExit("git " + a[0] + ": " + r.stderr.strip()[-300:])
    return r.stdout.strip()
if x["op"] == "have":
    os.makedirs(os.path.dirname(mirror), exist_ok=True)
    shell = os.environ.get("SHELL") or "/bin/sh"
    have = git("for-each-ref", "--format=%(objectname)", cwd=mirror).split() if os.path.isdir(mirror) else []
    print(json.dumps({"home": home, "shell": shell, "have": have}))
    raise SystemExit(0)
if not os.path.isdir(mirror):
    os.makedirs(os.path.dirname(mirror), exist_ok=True)
    git("init", "-q", "--bare", mirror)
for k, v in x["config"].items():
    if v:
        git("config", k, v, cwd=mirror)
b = x["branch"]
if x.get("bundle"):
    try:
        git("fetch", "-q", x["bundle"], "+refs/heads/%s:refs/heads/%s" % (b, b), cwd=mirror)
    finally:
        os.unlink(x["bundle"])
if not os.path.isdir(wt):
    git("worktree", "prune", cwd=mirror)
    os.makedirs(os.path.dirname(wt), exist_ok=True)
    git("worktree", "add", "-q", wt, b, cwd=mirror)
print(json.dumps({"worktree": wt, "head": git("rev-parse", "HEAD", cwd=wt)}))
'''

PULL = r'''
import base64, json, os, subprocess, sys, tempfile
x = json.load(sys.stdin)
wt = x["worktree"]
def git(*a):
    r = subprocess.run(["git", *a], cwd=wt, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return r.returncode, r.stdout.decode(errors="replace").strip(), r.stderr.decode(errors="replace").strip()
if not os.path.isdir(wt):
    print(json.dumps({"error": "its worktree there is gone"}))
    raise SystemExit(0)
if git("status", "--porcelain", "--untracked-files=no")[1]:
    print(json.dumps({"dirty": True}))
    raise SystemExit(0)
head = git("rev-parse", "HEAD")[1]
out = {"head": head}
if head != x["have"]:
    known = [x["have"]] if x["have"] and git("cat-file", "-e", x["have"] + "^{commit}")[0] == 0 else []
    fd, path = tempfile.mkstemp(suffix=".bundle")
    os.close(fd)
    try:
        code, _, err = git("bundle", "create", "-q", path, "HEAD", *["^" + k for k in known])
        if code != 0:
            print(json.dumps({"error": err[-300:]}))
            raise SystemExit(0)
        with open(path, "rb") as f:
            out["bundle"] = base64.b64encode(f.read()).decode()
    finally:
        os.unlink(path)
print(json.dumps(out))
'''


def _paths(s, t):
    """The mirror and worktree there, relative to its home: one mirror per folder here."""
    folder = s.loc["folder"]
    name = re.sub(r"[^A-Za-z0-9._-]+", "-", os.path.basename(folder.rstrip("/"))) or "repo"
    tag = hashlib.sha1(folder.encode()).hexdigest()[:6]
    mine = os.path.basename(t["worktree"].rstrip("/"))
    return (f".local/share/shepherdr/lanes/{name}-{tag}.git", f".local/share/shepherdr/worktrees/{name}-{tag}/{mine}")


def prepare(s, t, machine):
    """Gives the machine the task's branch and a worktree of it; records t["lane"]. Raises Fail."""
    wt, branch = t["worktree"], t["branch"]
    mirror, there = _paths(s, t)
    base = {"mirror": mirror, "worktree": there, "branch": branch}
    info = remote_python(machine, PREPARE, {**base, "op": "have"}, timeout=60)
    head = git(wt, "rev-parse", "HEAD")[1]
    have = [h for h in info.get("have") or [] if git(wt, "cat-file", "-e", h + "^{commit}")[0]]
    payload = {**base, "op": "add", "config": {k: git(wt, "config", k)[1] for k in ("user.name", "user.email")}}
    if head not in have:
        fd, path = tempfile.mkstemp(suffix=".bundle")
        os.close(fd)
        try:
            ok, _, err = git(wt, "bundle", "create", "-q", path, f"refs/heads/{branch}", *["^" + h for h in have])
            if not ok:
                raise Fail(f"Couldn't bundle {branch}: {err}")
            target = f"{mirror}.incoming-{os.getpid()}.bundle"
            guard(["hc"])
            with open(path, "rb") as f:
                r = subprocess.run(["hc", "file", "write", "--on", machine, target], stdin=f,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if r.returncode != 0:
                raise Fail(f"Couldn't send {branch} to {machine}: {r.stderr.decode(errors='replace').strip()}")
        finally:
            os.unlink(path)
        payload["bundle"] = os.path.join(info["home"], target)
    res = remote_python(machine, PREPARE, payload, timeout=300)
    t["lane"] = {"machine": machine, "home": info["home"], "shell": info.get("shell") or "/bin/sh",
                 "worktree": res["worktree"], "synced": res["head"]}
    s.dirty = True
    return t["lane"]


def pull(s, t):
    """Brings the lane's commits into the task's worktree here (fast-forward; or the lane's branch
    when nothing changed here since the last sync). Raises Fail for uncommitted work there."""
    lane = t.get("lane")
    wt = t.get("worktree")
    if not lane or not wt or not os.path.isdir(wt):
        return
    have = git(wt, "rev-parse", "HEAD")[1]
    res = remote_python(lane["machine"], PULL, {"worktree": lane["worktree"], "have": have}, timeout=120)
    if res.get("error"):
        raise Fail(f"Couldn't read the lane on {lane['machine']}: {res['error']}")
    if res.get("dirty"):
        raise Fail(f"{lane['worktree']} on {lane['machine']} has uncommitted changes. Commit them, then run ready again.")
    if res["head"] != have:
        fd, path = tempfile.mkstemp(suffix=".bundle")
        try:
            with os.fdopen(fd, "wb") as f:
                f.write(base64.b64decode(res["bundle"]))
            ok, _, err = git(wt, "fetch", "-q", path, "HEAD")
            if not ok:
                raise Fail(f"Couldn't bring the commits from {lane['machine']}: {err}")
        finally:
            os.unlink(path)
        if not git(wt, "merge", "-q", "--ff-only", res["head"])[0]:
            # Rewritten there (a rebase): its branch wins only when nothing was added here since.
            if have != lane.get("synced"):
                raise Fail(f"{t['branch']} changed both here and on {lane['machine']}: bring them together there, then run ready again.")
            ok, _, err = git(wt, "reset", "-q", "--keep", res["head"])
            if not ok:
                raise Fail(f"Couldn't take the lane's branch from {lane['machine']}: {err}")
    lane["synced"] = res["head"]
    s.dirty = True


def remote_argv(lane, argv, env=()):
    """The agent's command there: low priority (no scope: that's that machine's), without a parent
    session's markers, in its own interactive shell; HERDR_TASK_LANE sends its herdr-task calls here."""
    from .launch import clean_env, LOW
    from .settings import machine_option
    shell = lane.get("shell") or "/bin/sh"
    cmd = [shell, "-ic", '"$@"; exec "$0" -i', shell, *argv] if os.path.basename(shell) in ("bash", "zsh", "sh", "ksh", "dash") else argv
    cmd = clean_env(["env", *env, "HERDR_TASK_LANE=1", *cmd])
    return cmd if machine_option("low_priority", True) is False else ["sh", "-c", LOW, "herdr-low", *cmd]


def own_path(lane):
    return os.path.join(lane["home"], ".local", "bin", "herdr-task")


# ---- calls from the lane ----------------------------------------------------------------------

def forward(argv):
    """In the lane: runs a herdr-task call on the main machine (serve()) and returns its exit code."""
    me = os.environ.get("HC_TERMINAL_ID")
    if not me:
        print("herdr-task: run it from the lane's own terminal (there's no HC_TERMINAL_ID here).", file=sys.stderr)
        return 1
    os.makedirs(CALLS, exist_ok=True)
    os.chmod(CALLS, 0o700)
    nonce = os.urandom(16).hex()
    req, fifo = os.path.join(CALLS, nonce + ".json"), os.path.join(CALLS, nonce)
    with open(req, "w") as f:
        json.dump({"argv": argv}, f)
    os.mkfifo(fifo, 0o600)
    buf = b""
    try:
        fd = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
        try:
            r = subprocess.run(["hc", "label", me, f"{CALL}={nonce}"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                               universal_newlines=True)
            if r.returncode != 0:
                print(f"herdr-task: couldn't reach the main machine: {r.stderr.strip()}", file=sys.stderr)
                return 1
            end = time.time() + CALL_WAIT
            while time.time() < end and not buf.endswith(b"\n"):
                select.select([fd], [], [], 0.5)
                try:
                    chunk = os.read(fd, 65536)
                except BlockingIOError:
                    chunk = b""
                if chunk:
                    buf += chunk
                else:
                    time.sleep(0.1)
        finally:
            os.close(fd)
    finally:
        for p in (fifo, req):
            try:
                os.unlink(p)
            except OSError:
                pass
        subprocess.run(["hc", "label", me, f"{CALL}="], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        d = json.loads(buf)
    except ValueError:
        print("herdr-task: the main machine didn't answer (its Tasks scheduler checks every 15 s); try again in a minute.",
              file=sys.stderr)
        return 1
    sys.stdout.write(d.get("out") or "")
    sys.stderr.write(d.get("err") or "")
    return int(d.get("code") or 0)


WHERE_CALL = '"${XDG_STATE_HOME:-$HOME/.local/state}/shepherdr/lane-calls/$1"'


def serve(address):
    """Main machine: answers the lane call its terminal's label names, once. True when it answered."""
    from .launch import OWN
    k = hc_json("get", address) if "/" in address else None
    nonce = str(((k or {}).get("labels") or {}).get(CALL) or "")
    if not NONCE.fullmatch(nonce):
        return False
    machine = address.split("/", 1)[0]
    served = os.path.join(STORE, ".lane-calls")
    os.makedirs(served, exist_ok=True)
    for f in os.listdir(served):
        p = os.path.join(served, f)
        try:
            if time.time() - os.path.getmtime(p) > 3600:
                os.unlink(p)
        except OSError:
            pass
    try:
        os.close(os.open(os.path.join(served, nonce), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600))
    except FileExistsError:
        return False
    r = run(["hc", "exec", "--on", machine, "--timeout", "30", "--", "sh", "-c", f"exec cat {WHERE_CALL}.json", "sh", nonce])
    try:
        argv = json.loads(r.stdout)["argv"]
    except (ValueError, KeyError, TypeError):
        argv = None
    if not isinstance(argv, list) or not argv or not all(isinstance(a, str) for a in argv) or argv[0] not in OWN:
        out = {"code": 1, "out": "", "err": f"herdr-task: a lane runs only {', '.join(OWN)} on the main machine.\n"}
    else:
        env = {k: v for k, v in os.environ.items() if k != "HERDR_TASK_LANE"}
        env["HC_TERMINAL_ID"] = address
        try:
            p = subprocess.run([sys.executable, ENTRY, *argv], env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, universal_newlines=True, timeout=CALL_WAIT - 30)
            out = {"code": p.returncode, "out": p.stdout, "err": p.stderr}
        except subprocess.TimeoutExpired:
            out = {"code": 1, "out": "", "err": "herdr-task: it took too long on the main machine.\n"}
    r = subprocess.run(["hc", "exec", "--stdin", "--on", machine, "--timeout", "30", "--", "sh", "-c",
                        f"[ -p {WHERE_CALL} ] && exec cat > {WHERE_CALL}; exit 3", "sh", nonce],
                       input=(json.dumps(out) + "\n").encode(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return r.returncode == 0
