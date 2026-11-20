import copy
import tempfile
import unittest
from pathlib import Path

from history import DEFAULT_ROOT, archive
from reports import read_json, write_json
from test_history import measurement
from timeline import export, lnt


class TimelineTests(unittest.TestCase):
    def test_archive_contains_only_machine_readable_records(self):
        self.assertEqual([p for p in DEFAULT_ROOT.rglob("*") if p.is_file() and p.suffix != ".json"], [])

    def test_missing_metric_is_a_gap_and_suites_are_not_combined(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            one = measurement()
            for index in range(3):
                r = copy.deepcopy(one)
                r["id"] = str(index) * 32
                r["created_at"] = f"2026-11-0{index + 1}T00:00:00+00:00"
                if index == 1:
                    for run in r["runs"]:
                        run["cases"]["scroll"]["metrics"]["new_count"] = 7
                if index == 2:
                    r["machine"]["cpu"] = "other"
                source = root / (str(index) + ".json")
                write_json(source, r)
                archive(root, source)
            export(root, root / "site")
            groups = read_json(root / "site/index.json")
            self.assertEqual(sorted(g["count"] for g in groups), [1, 2])
            key = next(g["series"] for g in groups if g["count"] == 2)
            chart = read_json(root / "site" / (key + ".json"))
            self.assertEqual(chart["values"]["scroll"]["new_count"], {"1": 7})

    def test_lnt_uses_seconds_and_keeps_independent_runs(self):
        r = measurement()
        output = lnt(r)
        entry = next(t for t in output["tests"] if t["name"].endswith("/frame_p95_ms"))
        self.assertEqual(entry["execution_time"], [
            run["cases"]["scroll"]["metrics"]["frame_p95_ms"] / 1000 for run in r["runs"]])
