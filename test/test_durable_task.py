"""Opted-in launch and restart use only the task's own durable checkpoint."""
import home_guard  # noqa: F401

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "mods" / "tasks" / "bin"))
from herdr_task import commands, durable, launch, sessions, settings
from herdr_task.base import Fail


class DurableTask(unittest.TestCase):
    def setUp(self):
        root = self.enterContext(tempfile.TemporaryDirectory())
        self.store = SimpleNamespace(path=os.path.join(root, "list.json"), key=lambda task: "list#1", dirty=False)
        self.task = {"id": 1, "agents": []}
        self.argv = ["pi", "--model", "clawrouter/model", "--thinking", "high"]
        self.enterContext(patch.object(launch, "folder_for", return_value=root))
        self.enterContext(patch.object(launch, "coordinator_tab", return_value=None))
        self.enterContext(patch.object(launch, "agent_name", return_value="test-agent"))
        self.enterContext(patch.multiple(launch, clean_env=lambda argv: argv, in_shell=lambda argv: argv,
                                         low_priority=lambda argv, name: argv, own_argv=lambda argv, folder: argv))
        self.run = self.enterContext(patch.object(launch, "run", return_value=subprocess.CompletedProcess(
            [], 0, json.dumps({"address": "t1"}), "")))
        self.track = self.enterContext(patch.object(launch, "track"))
        self.runner = self.enterContext(patch.object(durable.shutil, "which", return_value="/bin/pi-durable-task"))

    def command(self):
        return self.run.call_args.args[0]

    def test_task_only_and_rollback(self):
        self.assertFalse(durable.enabled(self.task))
        launch.launch(self.store, self.task, self.argv, "space", "vanilla brief")
        self.assertEqual(self.command()[-(len(self.argv) + 1):], self.argv + ["vanilla brief"])
        self.track.assert_called_once()
        self.runner.assert_not_called()
        settings.set_opts(self.task, SimpleNamespace(set=["durable=on"]))
        state = durable.state_dir(self.store, self.task)
        second = {"id": 2}
        settings.set_task(second, "durable", "on")
        self.assertNotEqual(durable.state_dir(self.store, second), state)
        settings.set_task(self.task, "durable", "off")
        self.assertFalse(durable.enabled(self.task))
        settings.set_task(self.task, "durable", "on")
        self.assertEqual(durable.state_dir(self.store, self.task), state)
        with self.assertRaisesRegex(Fail, "on or off"):
            settings.set_task(self.task, "durable", "typo")

    def test_launch_and_resume_use_same_runner_cwd_state_and_model(self):
        settings.set_task(self.task, "durable", "on")
        first = launch.launch(self.store, self.task, self.argv, "space", "initial\nbrief")
        cmd = self.command()
        state = Path(durable.state_dir(self.store, self.task))
        self.assertEqual(cmd[cmd.index("--cwd") + 1], str(Path(self.store.path).parent))
        self.assertIn("herdr.agent=pi", cmd)
        self.assertEqual(cmd[-5:], ["/bin/pi-durable-task", "--state-dir=" + str(state), "--prompt=initial\nbrief",
                                    "--model=clawrouter/model", "--effort=high"])
        self.assertTrue(first["durable"])
        self.assertEqual(first["argv"], self.argv)
        self.track.assert_not_called()
        state.mkdir(parents=True)
        (state / "task.sqlite").touch()
        with patch.multiple(commands, parent_for=lambda *_: None, unarchive=lambda *_: None,
                            brief=lambda *_: "new render of the original brief"):
            _, resumed = commands.start_again(self.store, self.task, "space", message="automatic nudge", recovering=True)
            self.assertTrue(resumed)
            self.assertIn("--state-dir=" + str(state), self.command())
            self.assertIn("--prompt=new render of the original brief", self.command())
            self.assertNotIn("--message=automatic nudge", self.command())
            self.assertNotIn("--session", self.command())
            commands.start_again(self.store, self.task, "space", message="actual message")
            self.assertEqual(self.command()[-1], "--message=actual message")
        with patch.object(sessions, "detail", return_value={"address": "t1", "foreground": {"pid": 1}}), \
             patch.object(sessions, "session_of") as finder:
            sessions.refresh_sessions(self.store, self.task, [{"address": "t1"}])
        finder.assert_not_called()

    def test_no_fallback_to_vanilla_or_effect_on_review(self):
        settings.set_task(self.task, "durable", "on")
        with self.assertRaisesRegex(Fail, "needs the Pi worker"):
            launch.launch(self.store, self.task, ["codex"], "space", "brief")
        with self.assertRaisesRegex(Fail, "only --model and --thinking"):
            launch.launch(self.store, self.task, self.argv + ["--session", "other"], "space", "brief")
        self.runner.return_value = None
        with self.assertRaisesRegex(Fail, "Install pi-durable-task"):
            launch.launch(self.store, self.task, self.argv, "space", "brief")
        self.run.assert_not_called()
        launch.launch(self.store, self.task, ["codex"], "space", "review", role="reviewer")
        self.assertEqual(self.command()[-2:], ["codex", "review"])
