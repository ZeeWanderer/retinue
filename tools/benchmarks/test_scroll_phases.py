from copy import deepcopy
import unittest

from scroll_phases import summarize


def case(outlier):
    return {"listing_digest": "fixture", "items": 5, "scale": 2., "size": [80, 60],
            "threads": 4, "grid_host": "production", "live": False, "mode": "drag",
            "phase_schema": {"stages": ["update", "paint"], "owner_runqueue_available": False,
                             "counters": ["wall", "owner_cpu", "process_cpu", "owner_runqueue"]},
            "samples": [{"window": {"before": [0, 2], "after": [0, 2], "offset_before": i-1,
                                    "offset_after": i}, "total_ms": 3 + 6 * (i == outlier),
                         "phase_ns": [[1_000_000, 800_000, 800_000, 0],
                                      [2_000_000 + 6_000_000*(i == outlier), 0, 0, 0]]}
                        for i in range(20)]}


class ScrollPhaseTest(unittest.TestCase):
    def test_same_frame_decomposition_and_matched_reference(self):
        s = summarize([case(4), case(6)])
        self.assertEqual(s["tail_mean_ms"], 9)
        self.assertEqual(s["distinct_tail_indices"], 2)
        self.assertEqual(sum(p["tail_wall_ms"] for p in s["stages"].values()), 9)
        self.assertEqual(s["stages"]["paint"]["tail_matched_wall_ms"], 2)
        self.assertNotIn("tail_owner_runqueue_ms", s["stages"]["paint"])

    def test_refuses_incomparable_inputs_or_trajectory(self):
        a = case(0)
        for change in (lambda b: b.update(listing_digest="other"),
                       lambda b: b["samples"][4]["window"].update(offset_after=0)):
            b = deepcopy(a)
            change(b)
            with self.assertRaises(ValueError):
                summarize([a, b])

    def test_profile_timing_is_not_an_input(self):
        a, b = case(0), case(0)
        b["samples"][0]["window"]["tick_ms"] = 2
        self.assertEqual(summarize([a, b])["runs"], 2)
