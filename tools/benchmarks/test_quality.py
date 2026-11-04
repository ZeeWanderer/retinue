import argparse
import copy
import unittest

from quality import analyze
from reports import normalize
from run import measured_config


def raw(selected=110, alternative=100):
    rows = []
    for policy, value in (("selected", selected), ("alternative", alternative)):
        rows.append({"case": "example", "candidate": policy, "scenario": {"frames": 2},
                     "size": [100, 100], "scale": 1,
                     "decisions": [{"frame": 0}, {"frame": 1}],
                     "samples": [{"total_ns": value * 1000, "cpu_ns": value * 2000,
                                  "setup_ns": 1000, "setup_cpu_ns": 2000,
                                  "peak_raster_bytes": 10 if policy == "selected" else 20,
                                  "surface_bytes": 40000,
                                  "frames": [{"total_ns": (value - 1) * 500}] * 2}]})
    return {"schema": 1, "scope": "test", "selected": "selected", "requested_threads": 2,
            "alternatives": ["selected", "alternative"], "cases": rows}


class QualityTests(unittest.TestCase):
    def test_independent_runs_not_frames_determine_uncertainty(self):
        report = raw()
        report["cases"][0]["samples"] *= 100
        result = analyze([report], relative=.01)
        self.assertEqual(result["cases"][0]["verdict"], "inconclusive")
        self.assertEqual(result["cases"][0]["comparisons"][0]["familywise_delta_interval_95"], [None, None])

    def test_unreviewed_timing_has_no_automatic_acceptance(self):
        self.assertEqual(analyze([raw()] * 8)["cases"][0]["verdict"], "unreviewed")

    def test_regret_uses_native_measurements_and_reviewed_tolerance(self):
        row = analyze([raw()] * 8, relative=.01)["cases"][0]
        self.assertEqual(row["best_observed"], "alternative")
        self.assertAlmostEqual(row["observed_regret_percent"], 10)
        self.assertEqual(row["verdict"], "missed-opportunity")
        self.assertEqual(analyze([raw()] * 8, relative=.2)["cases"][0]["verdict"], "near-best-tested")
        self.assertEqual(analyze([raw(90, 100)] * 8, absolute=.001)["cases"][0]["verdict"], "near-best-tested")

    def test_memory_constraint_restricts_candidate_space(self):
        row = analyze([raw()] * 8, relative=.01, max_raster_bytes=15)["cases"][0]
        self.assertEqual(row["best_observed"], "selected")
        with self.assertRaises(ValueError):
            analyze([raw()], max_raster_bytes=5)

    def test_incomplete_changed_or_duplicate_results_are_invalid(self):
        for change in (
            lambda r: r["cases"].pop(),
            lambda r: r["cases"].append(copy.deepcopy(r["cases"][0])),
            lambda r: r["cases"][0]["scenario"].update(frames=3),
            lambda r: r["cases"][0]["samples"][0].update(total_ns=99),
            lambda r: r["cases"][0]["samples"][0].update(cpu_ns=float("nan")),
        ):
            changed = raw()
            change(changed)
            with self.assertRaises(ValueError):
                analyze([raw(), changed])

    def test_quality_reports_use_trace_not_frame_latency(self):
        cases = normalize(raw(), "quality")
        self.assertEqual(len(cases), 2)
        first = next(iter(cases.values()))
        self.assertIn("trace_mean_ms", first["metrics"])
        self.assertNotIn("frame_mean_ms", first["metrics"])
        self.assertEqual(first["frame_samples"], 2)

    def test_case_filter_is_supported_but_cadence_is_not(self):
        args = argparse.Namespace(threads=2, samples=10, repeats=7, hz=0, cases="example", timeout=600)
        spec = {"adapter": "quality", "cases": ["example"]}
        self.assertEqual(measured_config(args, spec)["cases"], "example")
        args.hz = 60
        with self.assertRaises(ValueError):
            measured_config(args, spec)


if __name__ == "__main__":
    unittest.main()
