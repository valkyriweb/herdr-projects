"""herdr-task: every module compiles, the add-on switch it reads, and (on CI only, where HOME is
a throwaway) its commands start. On a person's machine herdr-task never runs with another HOME."""
import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "mods", "tasks", "bin")
CI = os.environ.get("CI") == "true"


class HerdrTask(unittest.TestCase):
    def test_compiles(self):
        files = [os.path.join(BIN, "herdr-task")] + [os.path.join(BIN, "herdr_task", f) for f in sorted(os.listdir(os.path.join(BIN, "herdr_task"))) if f.endswith(".py")]
        files += [os.path.join(ROOT, "mods", "artifacts", "bin", "herdr-artifact"), os.path.join(ROOT, "bin", "herdr-secret"),
                  os.path.join(ROOT, "bin", "sync-shared"), os.path.join(ROOT, "bin", "shepherdr")]
        for f in files:
            with open(f, "rb") as fh:
                compile(fh.read(), f, "exec")

    def test_addon_off(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"everywhere": {"off": ["browser"]}}, f)
        code = ("import sys; sys.dont_write_bytecode = True; sys.path.insert(0, sys.argv[1]);"
                "from herdr_task.base import addon_off; print(addon_off('browser'), addon_off('voice'))")
        env = {**os.environ, "HERDR_TASK_SETTINGS": f.name}
        out = subprocess.run([sys.executable, "-c", code, BIN], capture_output=True, text=True, env=env)
        os.unlink(f.name)
        self.assertEqual(out.stdout.split(), ["True", "False"], out.stderr)

    @unittest.skipUnless(CI, "runs only on CI: herdr-task never runs with a changed HOME on a person's machine")
    def test_commands_start(self):
        home = tempfile.mkdtemp()
        env = {**os.environ, "HOME": home, "HERDR_TASK_STORE": os.path.join(home, "tasks")}
        for args in (["--help"], ["--version"], ["list", "--help"], ["machines", "sync", "--help"]):
            p = subprocess.run([sys.executable, os.path.join(BIN, "herdr-task"), *args], capture_output=True, text=True, env=env)
            self.assertEqual(p.returncode, 0, f"herdr-task {' '.join(args)}: {p.stderr}")


class Kinds(unittest.TestCase):
    """The agent kinds table (lib/kinds.json), as herdr-task starts, resumes and names each CLI."""

    def run_py(self, code):
        pre = "import sys; sys.dont_write_bytecode = True; sys.path.insert(0, sys.argv[1]); "
        p = subprocess.run([sys.executable, "-c", pre + code, BIN], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout.strip()

    def test_table_is_the_source(self):
        with open(os.path.join(ROOT, "lib", "kinds.json")) as f:
            table = {k: v for k, v in json.load(f).items() if not k.startswith("_")}
        for key in ("claude", "codex", "pi", "pii", "grok", "hermes", "amp", "cursor"):
            self.assertIn(key, table)
        for key, k in table.items():
            self.assertTrue(k["run"] and k["name"], key)
            self.assertIsInstance(k["perms"], list, key)
            self.assertIsInstance(k["models"], list, key)
        with open(os.path.join(BIN, "herdr_task", "kinds.json")) as f:
            self.assertEqual({k: v for k, v in json.load(f).items() if not k.startswith("_")}, table, "run bin/sync-shared")
        with open(os.path.join(ROOT, "lib", "kinds.js")) as f:
            js = f.read()
        self.assertEqual(json.loads(js.split("export const KINDS = ", 1)[1].rstrip().rstrip(";")), table, "run bin/sync-shared")

    def test_start_argv(self):
        out = self.run_py(
            "from herdr_task.settings import argv_for, argv_model, agent_key, AGENTS; import json;"
            "print(json.dumps(["
            "argv_for({'agent': 'pi', 'model': 'clawrouter/claude-opus-5-5-200k', 'effort': 'high'}),"
            "argv_for({'agent': 'pii', 'model': 'clawrouter/gpt-6-luna'}),"
            "argv_for({'agent': 'grok', 'model': 'grok-4', 'effort': 'high', 'perm': 'full'}),"
            "argv_for({'agent': 'hermes', 'model': 'anthropic/claude-sonnet-4.6', 'effort': 'xhigh', 'perm': 'full'}),"
            "argv_for({'agent': 'amp', 'model': 'ignored', 'effort': 'ultra'}),"
            "argv_for({'agent': 'cursor', 'model': 'claude-opus-4-8', 'effort': 'high', 'perm': 'plan'}),"
            "argv_for({'agent': 'claude', 'model': 'opus', 'effort': 'high'}),"
            "argv_model(['pi', '--model', 'clawrouter/x', '--thinking', 'low']),"
            "argv_model(['cursor-agent', '--model', 'm[effort=low]']),"
            "agent_key(['/usr/local/bin/cursor-agent']), agent_key(['pi']), sorted(AGENTS)]))")
        got = json.loads(out)
        self.assertEqual(got[0], ["pi", "--model", "clawrouter/claude-opus-5-5-200k", "--thinking", "high"])
        self.assertEqual(got[1], ["pii", "--model", "clawrouter/gpt-6-luna"])
        self.assertEqual(got[2], ["grok", "--model", "grok-4", "--reasoning-effort", "high", "--always-approve"])
        self.assertEqual(got[3], ["hermes", "-m", "anthropic/claude-sonnet-4.6", "--reasoning", "xhigh", "--yolo"])
        self.assertEqual(got[4], ["amp", "--mode", "ultra"])
        self.assertEqual(got[5], ["cursor-agent", "--model", "claude-opus-4-8[effort=high]", "--mode", "plan"])
        self.assertEqual(got[6][:5], ["claude", "--model", "opus", "--effort", "high"])
        self.assertEqual(got[7], ["clawrouter/x", "low"])
        self.assertEqual(got[8], ["m", "low"])
        self.assertEqual(got[9:11], ["cursor", "pi"])
        self.assertEqual(got[11], ["amp", "claude", "codex", "cursor", "grok", "hermes", "pi", "pii"])

    def test_bad_effort_refused(self):
        out = self.run_py("from herdr_task.settings import argv_for; from herdr_task.base import Fail\n"
                          "try: argv_for({'agent': 'pi', 'effort': 'ultra'}); print('no')\n"
                          "except Fail as e: print('fail', e)")
        self.assertTrue(out.startswith("fail Pi has no 'ultra'"), out)

    def test_resume_and_sessions(self):
        out = self.run_py(
            "from herdr_task.launch import resume_argv; from herdr_task import kinds as K; import json;"
            "print(json.dumps(["
            "resume_argv(['pi', '--model', 'x'], 'abc'), resume_argv(['codex', '--no-daemon'], 'u'), resume_argv(['claude'], 'u'),"
            "resume_argv(['amp', '--mode', 'high'], 'T-1'), resume_argv(['grok'], 'g'), resume_argv(['hermes'], 'h'),"
            "resume_argv(['cursor-agent'], 'c'), resume_argv(['nvim'], 'x'),"
            "K.session_dir('pi', '/a/b c').split('/')[-1], K.session_dir('pii', '/a/b').split('/')[-1], K.session_dir('grok', '/a'),"
            "K.session_id('pi', '/d/2026-11-04T12-46-01-986Z_01a1a698-4f02-7475-a76f-678b2b1a1306.jsonl'),"
            "K.session_id('claude', '/d/01a1a698-4f02-7475-a76f-678b2b1a1306.jsonl')]))")
        got = json.loads(out)
        self.assertEqual(got[0], ["pi", "--model", "x", "--session", "abc"])
        self.assertEqual(got[1], ["codex", "resume", "--no-daemon", "u"])
        self.assertEqual(got[2], ["claude", "--resume", "u"])
        self.assertEqual(got[3], ["amp", "threads", "continue", "T-1", "--mode", "high"])
        self.assertEqual(got[4:7], [["grok", "--resume", "g"], ["hermes", "--resume", "h"], ["cursor-agent", "--resume", "c"]])
        self.assertIsNone(got[7])
        self.assertEqual(got[8:11], ["--a-b c--", "--a-b--", None])
        self.assertEqual(got[11], "01a1a698-4f02-7475-a76f-678b2b1a1306")
        self.assertEqual(got[12], "01a1a698-4f02-7475-a76f-678b2b1a1306")


if __name__ == "__main__":
    unittest.main()
