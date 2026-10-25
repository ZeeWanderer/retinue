import unittest

from compare import compare


class ComparisonTests(unittest.TestCase):
    def test_warns_about_different_repaint_cadences(self):
        report = {
            "cpu": "cpu", "rustc": "rustc", "profile": "release", "kernel": "kernel",
            "cases": [],
        }
        result = compare(report, {**report, "cadence_hz": 60})
        self.assertIn("repaint cadence differs", result)

    def test_matches_case_identity_not_report_order(self):
        def report(values):
            return {
                "cpu": "cpu", "rustc": "rustc", "profile": "release", "kernel": "kernel",
                "cases": [
                    {"case": name, "size": [1920, 1080], "scale": 1.0,
                     "warm": {"p95_ns": value}}
                    for name, value in values
                ],
            }
        result = compare(
            report([("hover", 2_000_000), ("scroll", 1_000_000)]),
            report([("scroll", 2_000_000), ("hover", 1_000_000), ("new", 10)]),
        )
        self.assertIn("-50.0%", result)
        self.assertIn("+100.0%", result)
        self.assertIn("Unmatched case: ('new'", result)


if __name__ == "__main__":
    unittest.main()
