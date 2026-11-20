import copy
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from compare import compare, rule_for
from reports import normalize, write_json


def report(values=(1.,) * 7, samples=120):
    return {"schema": 1, "purpose": "regression", "suite": "example", "machine": {"cpu": "cpu"},
            "state": "complete", "expected_runs": len(values),
            "compatibility": {"fixture": "fixed", "cadence": 60},
            "runs": [{"cases": {"scroll": {"frame_samples": samples, "actual_threads": 4,
                                           "work": {"items": 100}, "metrics": {"frame_p95_ms": value}}}}
                     for value in values]}


def policy():
    return {"schema": 1, "minimum_runs": 7, "rule": [
        {"suite": "example", "case": "*", "metric": "frame_p95_ms", "relative": .05, "absolute": .1}]}


class ComparisonTests(unittest.TestCase):
    def test_unchanged_and_faster_pass(self):
        for value in (1., .5, 1.05):
            self.assertEqual(compare(report(), report((value,) * 7), policy())["verdict"], "pass")

    def test_clear_regression(self):
        self.assertEqual(compare(report(), report((2.,) * 7), policy())["verdict"], "regression")

    def test_noise_or_insufficient_repetitions_are_not_passes(self):
        for candidate in (report((1.,) * 2), report((1.,) * 7, samples=20), report((.5, 2., .8, 1., 2., .4, 1.3))):
            self.assertEqual(compare(report(), candidate, policy())["verdict"], "inconclusive")

    def test_zero_baseline_uses_absolute_tolerance(self):
        self.assertEqual(compare(report((0.,) * 7), report((.01,) * 7), policy())["verdict"], "pass")

    def test_incompatible_reports_rejected(self):
        for field in ("schema", "suite", "machine", "compatibility", "purpose", "state", "expected_runs"):
            candidate = report()
            candidate[field] = "different"
            with self.subTest(field=field), self.assertRaises(ValueError):
                compare(report(), candidate, policy())

    def test_missing_added_duplicate_or_changing_cases_rejected(self):
        for mutate in (lambda r: r["runs"][0]["cases"].clear(),
                       lambda r: r["runs"][0]["cases"].update(extra=copy.deepcopy(r["runs"][0]["cases"]["scroll"]))):
            candidate = report()
            mutate(candidate)
            with self.assertRaises(ValueError):
                compare(report(), candidate, policy())
        row = {"size": [1920, 1080], "scale": 1, "samples_ms": [1, 2]}
        with self.assertRaises(ValueError):
            normalize([row, row], "foundry")

    def test_changing_threads_or_work_is_invalid(self):
        for field, value in (("actual_threads", 2), ("work", {"items": 99})):
            candidate = report()
            for run in candidate["runs"]:
                run["cases"]["scroll"][field] = value
            with self.assertRaises(ValueError):
                compare(report(), candidate, policy())

    def test_nonfinite_negative_and_boolean_measurements_rejected(self):
        for value in (float("nan"), float("inf"), -1, True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                compare(report(), report((value,) * 7), policy())

    def test_missing_metric_is_invalid(self):
        candidate = report()
        for run in candidate["runs"]:
            run["cases"]["scroll"]["metrics"] = {"other": 1}
        with self.assertRaises(KeyError):
            compare(report(), candidate, policy())

    def test_policy_must_cover_cases_without_ambiguity(self):
        for rules in ([], policy()["rule"] * 2):
            with self.assertRaises(ValueError):
                rule_for({"rule": rules}, "example", "scroll")
        for value in (-1, float("inf"), True):
            bad = policy()
            bad["rule"][0]["relative"] = value
            with self.assertRaises(ValueError):
                rule_for(bad, "example", "scroll")

    def test_frame_percentile_is_normalized_from_raw_frames(self):
        a = normalize([{"scale": 1, "samples_ms": [1., 2., 3.]}], "foundry")
        b = normalize([{"scale": 1, "samples": [{"total_ms": n} for n in (1., 2., 3.)]}], "gui")
        self.assertEqual(a, b)
        self.assertEqual(a["scale=1"]["metrics"]["frame_p95_ms"], 3.)

    def test_no_samples_is_invalid(self):
        for raw, adapter in (([], "gui"), ([{"scale": 1, "samples_ms": []}], "foundry")):
            with self.assertRaises(ValueError):
                normalize(raw, adapter)

    def test_optional_phase_samples_are_preserved_and_validated(self):
        row = {"case": "dag", "first_paint_ns": 10, "warm": {
            "frame_ns": [10, 20, 30], "process_cpu_ns": 60},
            "process_memory_after": {"rss_bytes": 100},
            "known_resources": {"phases_ns": {"owner": [1, 2, 3]}}}
        metrics = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(metrics["phase_owner_p50_ms"], 2 / 1e6)
        self.assertEqual(metrics["phase_owner_p95_ms"], 3 / 1e6)
        for phases in ({"owner": [1]}, {"bad name": [1, 2, 3]}, {"owner": [1, True, 3]}):
            row["known_resources"]["phases_ns"] = phases
            with self.assertRaises(ValueError):
                normalize({"cases": [row]}, "overlay")

    def test_layout_wait_but_not_summed_worker_cpu_partitions_the_owner(self):
        samples = {"layout_" + name: [value] for name, value in (
            ("dispatch", 3), ("worker_wall", 10), ("pickup", 4), ("serial_cpu", 8),
            ("batch_cpu", 16), ("wait_solving", 5), ("wait_dispatch", 1),
            ("wait_pickup", 1), ("wait_other", 0))}
        row = {"case": "layout", "first_paint_ns": 1, "warm": {
            "frame_ns": [20], "process_cpu_ns": 35}, "process_memory_after": {"rss_bytes": 100},
            "known_resources": {"owner_exclusive_ns": {"layout_wait": [7], "total": [7]},
                                "layout_job_ns": samples}}
        metrics = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(metrics["layout_batch_cpu_mean_ms"], 16 / 1e6)
        self.assertEqual(metrics["layout_wait_solving_mean_ms"], 5 / 1e6)
        samples["layout_wait_pickup"] = [2]
        with self.assertRaises(ValueError):
            normalize({"cases": [row]}, "overlay")

    def test_optional_work_counts_are_preserved_and_validated(self):
        row = {"case": "layout", "first_paint_ns": 10, "warm": {
            "frame_ns": [10, 20, 30], "process_cpu_ns": 60},
            "process_memory_after": {"rss_bytes": 100},
            "known_resources": {"work_counts": {"layouts": [0, 3, 9]}}}
        metrics = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(metrics["work_layouts_p50"], 3)
        self.assertEqual(metrics["work_layouts_mean"], 4)
        for counts in ({"layouts": [1]}, {"bad name": [1, 2, 3]},
                       {"layouts": [1, True, 3]}, {"layouts": [1, -1, 3]},
                       {"layouts": [1, 1.5, 3]}):
            row["known_resources"]["work_counts"] = counts
            with self.assertRaises(ValueError):
                normalize({"cases": [row]}, "overlay")

    def test_exclusive_phases_partition_frames_and_retain_means(self):
        row = {"case": "layout", "first_paint_ns": 10, "warm": {
            "frame_ns": [10, 20], "process_cpu_ns": 30},
            "process_memory_after": {"rss_bytes": 100},
            "known_resources": {"owner_exclusive_ns": {
                "host": [3, 8], "ir": [7, 12], "total": [10, 20]}}}
        metrics = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(metrics["exclusive_host_mean_ms"], 5.5 / 1e6)
        self.assertEqual(metrics["exclusive_ir_mean_ms"], 9.5 / 1e6)
        self.assertEqual(metrics["exclusive_total_mean_ms"], 15 / 1e6)
        row["known_resources"]["owner_ir_ns"] = {"observe": [2, 3], "resolve": [5, 9]}
        detailed = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(detailed["ir_resolve_mean_ms"], 7 / 1e6)
        self.assertEqual(detailed["exclusive_ir_mean_ms"], metrics["exclusive_ir_mean_ms"])
        row["known_resources"]["work_counts"] = {"ir_topology": [0, 2]}
        cohorts = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(cohorts["ir_structural_frames"], 1)
        self.assertEqual(cohorts["ir_structural_resolve_mean_ms"], 9 / 1e6)
        self.assertEqual(cohorts["ir_stable_total_mean_ms"], 7 / 1e6)
        row["known_resources"]["work_counts"]["ir_topology"] = [0, 0]
        stable = next(iter(normalize({"cases": [row]}, "overlay").values()))["metrics"]
        self.assertEqual(stable["ir_structural_frames"], 0)
        self.assertNotIn("ir_structural_total_mean_ms", stable)
        row["known_resources"]["owner_ir_ns"]["resolve"] = [4, 10]
        with self.assertRaises(ValueError):
            normalize({"cases": [row]}, "overlay")
        del row["known_resources"]["owner_ir_ns"]
        for phases in (
            {"host": [3, 8]},
            {"host": [3, 8], "total": [3]},
            {"host": [3, 8], "total": [4, 8]},
            {"host": [True, 8], "total": [1, 8]},
            {"host": [-3, 8], "total": [-3, 8]},
            {"host": [0.5, 8], "total": [0.5, 8]},
        ):
            row["known_resources"]["owner_exclusive_ns"] = phases
            with self.assertRaises(ValueError):
                normalize({"cases": [row]}, "overlay")

    def test_cli_exit_codes_and_baseline_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            baseline, candidate = root / "before.json", root / "after.json"
            write_json(baseline, report())
            original = baseline.read_bytes()
            rules = root / "policy.toml"
            rules.write_text('schema=1\n[[rule]]\nsuite="example"\ncase="*"\nmetric="frame_p95_ms"\nrelative=0.05\nabsolute=0.1\n')
            for data, code in ((report(), 0), (report((2.,) * 7), 1), ({}, 2), (report((1.,)), 3)):
                write_json(candidate, data)
                completed = subprocess.run([sys.executable, str(Path(__file__).with_name("compare.py")),
                                            str(baseline), str(candidate), "--policy", str(rules)],
                                           capture_output=True, text=True)
                self.assertEqual(completed.returncode, code, completed.stderr + completed.stdout)
                self.assertEqual(baseline.read_bytes(), original)
            write_json(candidate, report())
            completed = subprocess.run([sys.executable, str(Path(__file__).with_name("compare.py")),
                                        str(baseline), str(candidate), "--policy", str(rules),
                                        "--output", str(baseline)],
                                       capture_output=True, text=True)
            self.assertEqual(completed.returncode, 2, completed.stderr + completed.stdout)
            self.assertEqual(baseline.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
