import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import poe2


class ReportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.repository = Path(temporary.name)
        patched = patch.object(poe2, "REPOSITORY", self.repository)
        patched.start()
        self.addCleanup(patched.stop)

    def test_default_report_locations_are_durable_and_unique(self):
        first = poe2.report_directory(None)
        second = poe2.report_directory(None)
        self.assertEqual(first.parent, self.repository / "research/retinue/tests/poe2")
        self.assertNotEqual(first, second)
        self.assertTrue(first.is_dir() and second.is_dir())

    def test_explicit_output_never_overwrites_a_report(self):
        output = self.repository / "report"
        self.assertEqual(poe2.report_directory(output), output)
        (output / "report.json").write_text("keep")
        with self.assertRaises(FileExistsError):
            poe2.report_directory(output)
        self.assertEqual((output / "report.json").read_text(), "keep")

    def test_reports_survive_runtime_cleanup_on_success_and_failure(self):
        for failing in (False, True):
            with self.subTest(failing=failing):
                output = self.repository / ("failed" if failing else "passed")
                runtime = []

                def execute(stage, root, evidence, report):
                    runtime.append(root)
                    self.assertLess(len(str(root / "api.sock").encode()), 108)
                    (evidence / "daemon.log").write_text("test log")
                    for name in ("cache", "state", "home"):
                        (root / name).mkdir()
                        (root / name / "data").write_text("fixture")
                    if failing:
                        raise RuntimeError("fixture failure")

                with patch.object(poe2, "execute", side_effect=execute), \
                     patch.object(poe2.signal, "signal"), patch.object(poe2.signal, "alarm"), \
                     contextlib.redirect_stdout(io.StringIO()):
                    result = poe2.main(["--output", str(output)])
                self.assertEqual(result, 1 if failing else 0)
                self.assertFalse(runtime[0].exists())
                report = json.loads((output / "report.json").read_text())
                self.assertEqual(report["result"], "failed" if failing else "passed")
                self.assertEqual(report["evidence"], str(output))
                self.assertEqual((output / "daemon.log").read_text(), "test log")
                for name in ("cache", "state"):
                    self.assertEqual((output / name / "data").read_text(), "fixture")
                self.assertFalse((output / "home").exists())


if __name__ == "__main__":
    unittest.main()
