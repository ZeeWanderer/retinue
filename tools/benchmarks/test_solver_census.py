import unittest

from solver_census import placement_pairs, summarize


class SolverCensusTest(unittest.TestCase):
    def test_placement_overlap_is_per_frame_not_per_nonempty_frame(self):
        trace = {"group": ["custom_algorithm_on_miss"], "columns": [
            "same_measure_queries", "place_measure_queries", "same_measure_inclusive_ns", "same_measure_misses"
        ], "samples": [["resize", [[["Anchored"], [2, 5, 400, 1]]]],
                       ["scroll", [[["Anchored"], [4, 9, 800, 0]]]], ["resize", []]]}
        self.assertEqual(placement_pairs(trace, "resize"), (2, {"Anchored": (2.5, 1, .5, 200)}))

    def test_budget_counts_only_fixed_work_and_includes_empty_frames(self):
        trace = {"timed": True, "group": ["kind", "known_axes"],
                 "columns": ["queries", "misses", "worker_queries", "miss_self_ns", "leaf_adapter_ns"],
                 "samples": [["drag", [
                     [["custom_leaf", 2], [5, 3, 4, 100_000, 3000]],
                     [["plain_leaf", 2], [3, 2, 0, 1000, 0]],
                     [["flex", 1], [80, 60, 60, 1_000_000, 0]]]],
                     ["drag", []], ["hover", []]]}
        self.assertEqual(summarize(trace, "drag"), (2, 4, 2.5, 2, 2))
        trace["timed"] = False
        self.assertIsNone(summarize(trace, "drag")[-1])
        self.assertIsNone(summarize(trace, "absent"))
