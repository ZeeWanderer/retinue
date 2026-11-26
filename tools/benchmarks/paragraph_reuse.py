"""Count repeat cold paragraph inputs in an instrumented GUI scroll trace."""

import argparse
from collections import Counter, OrderedDict
import json
from pathlib import Path


def summarize(frames, start=0, stop=None, capacity=128):
    stop = len(frames) if stop is None else stop
    if not 0 <= start <= stop <= len(frames) or capacity < 1:
        raise ValueError("invalid frame interval or cache capacity")
    seen = [set(), set()]
    lru = OrderedDict()
    counts = [Counter(), Counter()]
    hits = builds = 0
    for i, frame in enumerate(frames[:stop]):
        current = [set(), set()]
        for shape, placement in frame:
            measured = i >= start
            builds += measured
            for j, key in enumerate((shape, placement)):
                if measured:
                    category = ("prior_frame" if key in seen[j] else
                                "same_frame" if key in current[j] else "first_observed")
                    counts[j][category] += 1
                current[j].add(key)
            if placement in lru:
                hits += measured
                lru.move_to_end(placement)
            else:
                lru[placement] = None
                if len(lru) > capacity:
                    lru.popitem(last=False)
        for history, additions in zip(seen, current):
            history.update(additions)
    return {"frames": [start, stop], "builds": builds, "shape": counts[0],
            "placement": counts[1], "simulated_placement_lru": {"entries": capacity, "hits": hits}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--start", type=int, default=0, help="first measured frame; earlier frames prime history")
    parser.add_argument("--stop", type=int, help="exclusive end frame")
    parser.add_argument("--capacity", type=int, default=128, help="simulated placement cache capacity in entries")
    args = parser.parse_args()
    result = []
    for case in json.loads(args.trace.read_text()):
        if "paragraph_keys" not in case:
            parser.error("missing paragraph_keys; record with GUI_BENCH_TEXT_KEYS=1 and layout-profiling,geometry-census")
        try:
            stats = summarize(case["paragraph_keys"], args.start, args.stop, args.capacity)
        except ValueError as exc:
            parser.error(str(exc))
        result.append({**{k: case[k] for k in ("page", "mode", "scale", "live") if k in case}, **stats})
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
