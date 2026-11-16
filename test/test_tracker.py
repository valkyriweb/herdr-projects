"""Tracker session discovery uses the kind's filesystem route, never live hc or lsof.

The process-loop fixture replaces external core/store boundaries; real session discovery
reads temporary log files, including a new session between ticks.
"""
import home_guard
import contextlib
import os
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(__file__)), "mods", "tasks", "bin"))
from herdr_task import automations, sessions


class Tracker(unittest.TestCase):
    def test_pi_session_changes_and_terminal_exit(self):
        with tempfile.TemporaryDirectory() as logs:
            sid1 = "11111111-1111-1111-1111-111111111111"
            sid2 = "22222222-2222-2222-2222-222222222222"
            first = os.path.join(logs, "2026-11-15_" + sid1 + ".jsonl")
            second = os.path.join(logs, "2026-11-15_" + sid2 + ".jsonl")
            open(first, "w").close()
            os.utime(first, (1794700000, 1794700000))
            agent = {"address": "t1", "agent": "pi", "at": 1794700000000}
            task = {"agents": [agent]}
            store = types.SimpleNamespace(find=lambda _: task, dirty=False)
            @contextlib.contextmanager
            def fixture_store(_):
                yield store
            terminal = {"address": "t1", "state": "live", "cwd": "/fallback",
                        "foreground": {"pid": 123, "name": "node", "cwd": "/project"}}
            def cleared(_):
                self.assertEqual(agent["session"], sid1)
                open(second, "w").close()
                os.utime(second, (1794700001, 1794700001))
            with mock.patch.object(automations, "Store", fixture_store), \
                    mock.patch.object(automations, "locate", lambda p: p), \
                    mock.patch.object(automations, "hc_json", side_effect=[terminal, terminal, {"state": "finished"}]), \
                    mock.patch.object(automations.time, "sleep", side_effect=lambda n: cleared(n) if not os.path.exists(second) else None), \
                    mock.patch.object(sessions.K, "session_dir", return_value=logs) as session_dir, \
                    mock.patch.object(sessions, "run", side_effect=AssertionError("must not spawn pgrep or lsof")):
                automations.cmd_track(types.SimpleNamespace(folder="/project", id=1, address="t1"))
            self.assertEqual((agent["session"], agent["log"]), (sid2, second))
            self.assertIn("ended", agent)
            self.assertTrue(store.dirty)
            session_dir.assert_called_with("pi", "/project")

    def test_missing_agent_does_not_poll(self):
        store = mock.MagicMock()
        store.__enter__.return_value.find.return_value = {"agents": []}
        with mock.patch.object(automations, "Store", return_value=store), \
                mock.patch.object(automations, "locate", lambda p: p), \
                mock.patch.object(automations, "hc_json") as hc:
            automations.cmd_track(types.SimpleNamespace(folder="/project", id=1, address="t1"))
        hc.assert_not_called()
