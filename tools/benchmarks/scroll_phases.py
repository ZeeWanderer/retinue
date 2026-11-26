"""Attribute complete-frame tails without summing unrelated stage percentiles."""

import argparse
import json
from math import ceil
from pathlib import Path
from statistics import mean, median


def identity(run):
    # Profiling builds also store elapsed times in the window record.
    geometry = ("before", "after", "offset_before", "offset_after")
    return (run["listing_digest"], run["items"], run["scale"], run["size"], run["threads"],
            run["grid_host"], run["live"], run["mode"],
            [tuple(s["window"][field] for field in geometry) for s in run["samples"]])


def summarize(runs):
    if not runs or not runs[0]["samples"]:
        raise ValueError("no frames")
    key = identity(runs[0])
    schema = runs[0]["phase_schema"]
    if schema["counters"] != ["wall", "owner_cpu", "process_cpu", "owner_runqueue"]:
        raise ValueError("unsupported phase counters")
    stages = schema["stages"]
    if any(identity(r) != key or r["phase_schema"] != schema for r in runs[1:]):
        raise ValueError("inputs, frame sequence or phase schema differ")
    samples = [r["samples"] for r in runs]
    n = len(samples[0])
    k = ceil(n * .05)
    tails, centers = [], []
    for frames in samples:
        order = sorted(range(n), key=lambda i: frames[i]["total_ms"])
        tails.append(order[-k:])
        centers.append(order[(n-k)//2:(n-k)//2+k])
    result = {}
    for col, name in enumerate(stages):
        def avg(frames, ids, axis):
            return mean(frames[i]["phase_ns"][col][axis] for i in ids) / 1e6
        metrics = {}
        for axis, counter in enumerate(schema["counters"]):
            if counter == "owner_runqueue" and not schema["owner_runqueue_available"]:
                continue
            for label, selected in (("tail", tails), ("middle", centers)):
                metrics[f"{label}_{counter}_ms"] = mean(
                    avg(frames, ids, axis) for frames, ids in zip(samples, selected, strict=True))
        if len(runs) > 1:
            # Same input frame, but without choosing the other runs for being slow.
            metrics["tail_matched_wall_ms"] = mean(
                mean(median(other[i]["phase_ns"][col][0] for b, other in enumerate(samples) if b != a)
                     for i in ids) for a, ids in enumerate(tails)) / 1e6
        result[name] = metrics
    return {"runs": len(runs), "frames_per_run": n, "tail_frames_per_run": k,
            "distinct_tail_indices": len(set().union(*tails)), "tail_mean_ms": mean(
                mean(f[i]["total_ms"] for i in ids) for f, ids in zip(samples, tails, strict=True)),
            "middle_mean_ms": mean(mean(f[i]["total_ms"] for i in ids)
                                   for f, ids in zip(samples, centers, strict=True)),
            "stages": result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traces", nargs="+", type=Path)
    parser.add_argument("--mode", choices=("drag", "wheel"), default="drag")
    parser.add_argument("--scale", type=float, default=2)
    args = parser.parse_args()
    runs = []
    for trace in args.traces:
        selected = [r for r in json.loads(trace.read_text())
                    if r["mode"] == args.mode and r["scale"] == args.scale]
        if len(selected) != 1 or "phase_schema" not in selected[0]:
            parser.error(f"{trace}: expected one phase trace at this mode and scale")
        runs.extend(selected)
    try:
        print(json.dumps(summarize(runs), indent=2))
    except ValueError as exc:
        parser.error(str(exc))


if __name__ == "__main__":
    main()
