import unittest

from paragraph_reuse import summarize


class ParagraphReuseTest(unittest.TestCase):
    def test_prefix_primes_both_keys_and_lru_but_is_not_counted(self):
        frames = [[[1, 1], [1, 2]], [], [[1, 1], [2, 3], [2, 3], [1, 2]], [[3, 4]]]
        stats = summarize(frames, 2, 3, 2)
        self.assertEqual(stats["builds"], 4)
        for key in ("shape", "placement"):
            self.assertEqual(stats[key], {"prior_frame": 2, "first_observed": 1, "same_frame": 1})
        self.assertEqual(stats["simulated_placement_lru"]["hits"], 2)

    def test_new_width_and_empty_interval(self):
        frames = [[[0, 0]], [[0, 1]]]
        stats = summarize(frames, 1)
        self.assertEqual(stats["shape"], {"prior_frame": 1})
        self.assertEqual(stats["placement"], {"first_observed": 1})
        self.assertEqual(summarize(frames, 1, 1)["builds"], 0)
        with self.assertRaises(ValueError):
            summarize(frames, 1, 3)
