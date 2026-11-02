import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "test-quiet"


class QuietRunnerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        stub = self.root / "rebar3"
        stub.write_text('#!/bin/sh\nprintf "runner: %s\\n" "$*"\nexit "${TEST_EXIT:-0}"\n')
        stub.chmod(0o755)
        self.env = {**os.environ, "REBAR3": str(stub)}
        self.env.pop("RETINUE_TEST_LOG_DIR", None)

    def run_quiet(self, *args):
        return subprocess.run([str(SCRIPT), *args], cwd=self.root, env=self.env,
                              capture_output=True, text=True)

    def test_success_is_quiet_and_removes_log(self):
        result = self.run_quiet("eunit", "--module", "example")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "rebar3 eunit: passed\n")
        self.assertEqual(list(self.root.rglob("*.log")), [])

    def test_failure_keeps_evidence_in_research_and_preserves_exit(self):
        self.env["TEST_EXIT"] = "7"
        result = self.run_quiet("ct", "--suite", "example")
        self.assertEqual(result.returncode, 7)
        logs = list((self.root / "research/retinue/tests").glob("ct.*.log"))
        self.assertEqual(len(logs), 1)
        self.assertEqual(logs[0].read_text(), "runner: ct --suite example\n")
        self.assertIn("research/retinue/tests/", result.stderr)

    def test_custom_log_directory(self):
        self.env.update(TEST_EXIT="1", RETINUE_TEST_LOG_DIR=str(self.root / "custom"))
        self.assertEqual(self.run_quiet("ct").returncode, 1)
        self.assertEqual(len(list((self.root / "custom").glob("*.log"))), 1)

    def test_unknown_command_does_not_run(self):
        self.assertEqual(self.run_quiet("unknown").returncode, 2)
        self.assertFalse((self.root / "research").exists())


if __name__ == "__main__":
    unittest.main()
