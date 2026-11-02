import copy
from pathlib import Path
import tempfile
import unittest

from history import archive, baseline_for, select, series
from reports import read_json, write_json
from test_compare import report


def measurement():
    value = report()
    value.update(id="a" * 32, provenance={"revision": "old", "dirty": False, "untracked": {}},
                 created_at="2026-11-02T00:00:00+00:00")
    return value


class HistoryTests(unittest.TestCase):
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

    def test_smoke_dirty_incomplete_and_unsafe_ids_are_not_baselines(self):
        for kind in ("dirty", "smoke", "incomplete", "path", "reason"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                value = measurement()
                if kind == "dirty":
                    value["provenance"]["dirty"] = True
                elif kind == "smoke":
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
