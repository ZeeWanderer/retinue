"""Summarize exact-size child queries per frame in a GUI solver census."""

import argparse
import json
from collections import defaultdict
from pathlib import Path


def workloads(report):
    if isinstance(report, list):
        for case in report:
            yield f'{case["page"]} {case["scale"]:.0%}', case["solver"]
    elif "cases" in report:
        for case in report["cases"]:
            yield f'{case["case"]} {case["scale"]:.0%}', case["known_resources"]["solver"]
    else:
        yield f'catalog {report["scale"]:.0%}', report["solver"]


def summarize(trace, phase=None):
    frames = calls = misses = workers = 0
    direct = 0
    for label, rows in trace["samples"]:
        if phase is not None and label != phase:
            continue
        frames += 1
        for group, values in rows:
            key = dict(zip(trace["group"], group, strict=True))
            if key["known_axes"] != 2:
                continue
            row = dict(zip(trace["columns"], values, strict=True))
            calls += row["queries"]
            misses += row["misses"]
            workers += row["worker_queries"]
            if key["kind"] in ("plain_leaf", "flex", "grid"):
                direct += row["miss_self_ns"]
            else:
                direct += row["leaf_adapter_ns"]
    if not frames:
        return None
    return (frames, calls / frames, misses / frames, workers / frames,
            direct / (1000 * frames) if trace["timed"] else None)


def placement_pairs(trace, phase=None):
    fields = ("place_measure_queries", "same_measure_queries", "same_measure_misses",
              "same_measure_inclusive_ns")
    indices = [trace["columns"].index(k) for k in fields]
    algorithm = trace["group"].index("custom_algorithm_on_miss")
    totals = defaultdict(lambda: [0] * len(fields))
    frames = 0
    for label, rows in trace["samples"]:
        if phase is not None and phase != label:
            continue
        frames += 1
        for group, counts in rows:
            if not counts[indices[0]]:
                continue
            dest = totals[group[algorithm]]
            for i, col in enumerate(indices):
                dest[i] += counts[col]
    return frames, {key: tuple(v / frames for v in counts) for key, counts in totals.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--phase", help="include only samples with this label")
    parser.add_argument("--placement-pairs", action="store_true",
                        help="show placement queries repeating a measured child and proposal in the same invocation")
    args = parser.parse_args()
    if args.placement_pairs:
        print("workload\talgorithm\tframes\tplace queries/frame\tmatches/frame\tmatch misses/frame\tmatched inclusive us/frame")
        matched_phase = False
        for name, trace in workloads(json.loads(args.report.read_bytes())):
            try:
                frames, rows = placement_pairs(trace, args.phase)
            except ValueError:
                parser.error("report has no placement-pair census")
            matched_phase |= bool(frames)
            for key, (q, repeated, misses, ns) in sorted(rows.items()):
                time = f"{ns / 1000:.2f}" if trace["timed"] else "-"
                print(f"{name}\t{key}\t{frames}\t{q:.2f}\t{repeated:.2f}\t{misses:.2f}\t{time}")
        if not matched_phase:
            parser.error("no matching workload samples")
        return
    print("workload\tframes\tfixed queries/frame\tmisses/frame\tworker queries/frame\tbox work us/frame")
    matches = 0
    for name, trace in workloads(json.loads(args.report.read_bytes())):
        summary = summarize(trace, args.phase)
        if summary is not None:
            frames, queries, misses, workers, us = summary
            cost = "-" if us is None else f"{us:.2f}"
            print(f"{name}\t{frames}\t{queries:.2f}\t{misses:.2f}\t{workers:.2f}\t{cost}")
            matches += 1
    if not matches:
        parser.error("no matching workload samples")


if __name__ == "__main__":
    main()
