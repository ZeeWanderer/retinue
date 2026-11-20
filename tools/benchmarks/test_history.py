import copy
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from history import archive, baseline_for, select, series, verify_revision
from reports import ROOT, read_json, write_json
from test_compare import report


def measurement():
    value = report()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    value.update(id="a" * 32, provenance={"revision": commit, "dirty": False, "untracked": {}},
                 created_at="2026-11-02T00:00:00+00:00")
    return value


class HistoryTests(unittest.TestCase):
    def test_dangling_commit_is_not_a_durable_source_link(self):
        with tempfile.TemporaryDirectory() as temp, patch("history.ROOT", Path(temp)):
            def git(*args):
                return subprocess.check_output(["git", "-c", "user.name=Bench", "-c",
                                                "user.email=bench@example.invalid", *args],
                                               cwd=temp, text=True).strip()
            git("init", "-q")
            git("commit", "-q", "--allow-empty", "-m", "reachable")
            verify_revision(git("rev-parse", "HEAD"))
            dangling = git("commit-tree", "HEAD^{tree}", "-m", "unreferenced")
            with self.assertRaisesRegex(ValueError, "not in this checkout"):
                verify_revision(dangling)

    def test_archive_rejects_unreferenceable_measurements_before_writing(self):
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip()
        for fields, error in [({"dirty": True}, "clean measured checkout"),
                              ({"dirty": None}, "clean measured checkout"),
                              ({"revision": "HEAD"}, "full Git commit ID"),
                              ({"revision": "0" * 40}, "not in this checkout"),
                              ({"revision": tree}, "not in this checkout")]:
            with self.subTest(fields=fields), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                r = measurement()
                r["provenance"].update(fields)
                source = root / "candidate.json"
                write_json(source, r)
                with self.assertRaisesRegex(ValueError, error):
                    archive(root, source)
                self.assertFalse((root / "records").exists())

    def test_revision_and_timings_do_not_change_the_series(self):
        old = measurement()
        new = copy.deepcopy(old)
        new["provenance"]["revision"] = "new"
        new["runs"][0]["cases"]["scroll"]["metrics"]["frame_p95_ms"] = 3
        self.assertEqual(series(old), series(new))
        for key in ("machine", "compatibility"):
            new = copy.deepcopy(old)
            new[key]["changed"] = True
            self.assertNotEqual(series(old), series(new))

    def test_archive_is_immutable_and_select_does_not_modify_reports(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "source.json"
            original = measurement()
            write_json(source, original)
            destination = archive(root, source, "release check")
            saved = destination.read_bytes()
            with self.assertRaises(FileExistsError):
                archive(root, source, "overwrite")
            with self.assertRaises(ValueError):
                baseline_for(root, original)
            select(root, original["id"], "initial reviewed baseline")
            self.assertEqual(baseline_for(root, original)["id"], original["id"])
            self.assertEqual(destination.read_bytes(), saved)
            self.assertEqual(read_json(source), original)
            new = copy.deepcopy(original)
            new["machine"]["cpu"] = "other"
            with self.assertRaises(ValueError):
                baseline_for(root, new)

    def test_smoke_incomplete_and_unsafe_ids_are_not_baselines(self):
        for kind in ("smoke", "incomplete", "path", "reason"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                value = measurement()
                if kind == "smoke":
                    value["runs"] = value["runs"][:1]
                    value["expected_runs"] = 1
                elif kind == "incomplete":
                    value["state"] = "recording"
                elif kind == "path":
                    value["id"] = "../escape"
                source = root / "source.json"
                write_json(source, value)
                with self.assertRaises(ValueError):
                    archive(root, source, "experiment")
                    select(root, value["id"], "" if kind == "reason" else "accept")


if __name__ == "__main__":
    unittest.main()
