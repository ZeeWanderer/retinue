import copy
import tempfile
import unittest
from pathlib import Path

from history import DEFAULT_ROOT, archive
from reports import read_json, write_json
from test_history import measurement
from timeline import export, lnt, trend_dimensions
from run import catalog


class TimelineTests(unittest.TestCase):
    def test_trends_follow_the_workload_not_the_toolchain(self):
        before = measurement()
        before["compatibility"].update(adapter="foundry", workload=1, config={"threads": 4})
        after = copy.deepcopy(before)
        after["compatibility"].update(build={"rustc": "next"}, fixture_sha256="new")
        after["machine"].update(kernel="new", affinity=[4, 5, 6, 7])
        self.assertEqual(trend_dimensions(before), trend_dimensions(after))
        for field, value in [("workload", 2), ("config", {"threads": 1})]:
            changed = copy.deepcopy(after)
            changed["compatibility"][field] = value
            self.assertNotEqual(trend_dimensions(before), trend_dimensions(changed))

    def test_maintained_suites_explain_their_workload(self):
        for name, spec in catalog()["suite"].items():
            self.assertTrue(spec.get("description", "").strip(), name)

    def test_spread_describes_independent_runs_not_frames(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for i, samples in enumerate(([2], [4, 1, 3, 2], [0, 0, 0, 0])):
                r = measurement()
                r.update(id=str(i) * 32, created_at=f"2026-11-0{i + 1}T00:00:00+00:00",
                         expected_runs=len(samples), runs=[copy.deepcopy(r["runs"][0]) for _ in samples])
                for run, value in zip(r["runs"], samples):
                    run["cases"]["scroll"]["metrics"]["frame_p95_ms"] = value
                path = root / f"{i}.json"
                write_json(path, r)
                archive(root, path)
            export(root, root / "site")
            [s] = read_json(root / "site/index.json")
            chart = read_json(root / "site" / (s["series"] + ".json"))
            self.assertEqual(chart["spreads"]["scroll"]["frame_p95_ms"], {"1": [1.75, 3.25], "2": [0, 0]})
            self.assertEqual(chart["values"]["scroll"]["frame_p95_ms"], {"0": 2, "1": 2.5, "2": 0})

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

    def test_distinct_sessions_and_binaries_can_share_a_source_revision(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for i, binary in enumerate(["a", "b", "b"]):
                report = measurement()
                report["id"] = str(i) * 32
                report["provenance"].update(binary_sha256=binary * 64, dirty=i != 0)
                path = root / (str(i) + ".json")
                write_json(path, report)
                archive(root, path)
            export(root, root / "site")
            [series] = read_json(root / "site/index.json")
            points = read_json(root / "site" / (series["series"] + ".json"))["points"]
            self.assertEqual(len({p["id"] for p in points}), 3)
            self.assertEqual(len({p["binary"] for p in points}), 2)
            self.assertEqual(len({p["revision"] for p in points}), 1)

    def test_lnt_uses_seconds_and_keeps_independent_runs(self):
        r = measurement()
        output = lnt(r)
        entry = next(t for t in output["tests"] if t["name"].endswith("/frame_p95_ms"))
        self.assertEqual(entry["execution_time"], [
            run["cases"]["scroll"]["metrics"]["frame_p95_ms"] / 1000 for run in r["runs"]])
