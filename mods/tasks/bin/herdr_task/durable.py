"""Opt-in Pi runner; a checkpoint belongs to one task, not to its working directory."""

import os
import shutil

from .base import Fail


def enabled(task):
    return (task.get("set") or {}).get("durable") == "on"


def state_dir(store, task):
    return os.path.join(os.path.dirname(store.path), "pi-durable", task["durable_id"])


def checkpoint_exists(store, task):
    return os.path.isfile(os.path.join(state_dir(store, task), "task.sqlite"))


def command(store, task, argv, prompt, message=None):
    if os.path.basename(argv[0]) != "pi":
        raise Fail("durable=on needs the Pi worker. Use --agent pi or turn durable off for this task.")
    # Don't silently drop a custom flag (including a session or permission option).
    options = {"--model": "model", "--thinking": "effort"}
    args = argv[1:]
    if len(args) % 2 or any(flag not in options or not value or value.startswith("-")
                            for flag, value in zip(args[::2], args[1::2])):
        raise Fail("Durable Pi takes only --model and --thinking from Pi's argv.")
    runner = shutil.which("pi-durable-task")
    if not runner:
        raise Fail("Install pi-durable-task on this machine's PATH before starting a durable task, or turn durable off.")
    cmd = [os.path.abspath(runner), "--state-dir=" + state_dir(store, task), "--prompt=" + prompt]
    cmd += [f"--{options[flag]}={value}" for flag, value in zip(args[::2], args[1::2])]
    return cmd + (["--message=" + message] if message else [])
