#!/usr/bin/env python3
"""Compare optimizer choices with measured legal alternatives over complete traces."""

import argparse
import math
from pathlib import Path
import random
import statistics
import sys

from reports import digest, numbers, percentile, read_json, validate_report, write_json


def interval(deltas, comparisons):
    rng = random.Random(0)
    draws = sorted(statistics.mean(rng.choices(deltas, k=len(deltas))) for _ in range(8000))
    tail = .025 / max(1, comparisons)
    return [draws[max(0, math.ceil(len(draws) * tail) - 1)],
            draws[max(0, math.ceil(len(draws) * (1 - tail)) - 1)]]


def measurements(row):
    samples = row["samples"]
    trace = numbers(s["total_ns"] / 1e6 for s in samples)
    cpu = numbers(s["cpu_ns"] / 1e6 for s in samples)
    frames = [frame for sample in samples for frame in sample["frames"]]
    if not frames or any(not sample["frames"] for sample in samples):
        raise ValueError("empty measured trace")
    return {
        "trace_mean_ms": statistics.mean(trace),
        "frame_p95_ms": percentile([frame["total_ns"] / 1e6 for frame in frames], .95),
        "cpu_ms_per_trace": statistics.mean(cpu),
        "peak_raster_bytes": max(numbers(s["peak_raster_bytes"] for s in samples)),
        "surface_bytes": max(numbers(s["surface_bytes"] for s in samples)),
    }


def analyze(raws, *, metric="trace_mean_ms", relative=None, absolute=None, max_raster_bytes=None):
    if metric not in ("trace_mean_ms", "frame_p95_ms"):
        raise ValueError("unknown latency objective")
    for value in (relative, absolute, max_raster_bytes):
        if value is not None and (isinstance(value, bool) or not math.isfinite(value) or value < 0):
            raise ValueError("tolerances and memory constraints must be nonnegative and finite")
    if not raws:
        raise ValueError("no independent process results")
    first = raws[0]
    alternatives = first["alternatives"]
    selected = first["selected"]
    if not alternatives or len(set(alternatives)) != len(alternatives) or selected not in alternatives:
        raise ValueError("invalid policy space")
    grouped = {}
    expected = None
    for raw in raws:
        if raw["schema"] != 1 or any(raw[key] != first[key] for key in ("selected", "alternatives", "requested_threads", "scope")):
            raise ValueError("incompatible quality report")
        seen = set()
        for row in raw["cases"]:
            key = row["case"]
            policy = row["candidate"]
            if policy not in alternatives or (key, policy) in seen:
                raise ValueError("unknown or duplicate policy result")
            seen.add((key, policy))
            group = grouped.setdefault(key, {"scenario": row["scenario"], "policies": {p: [] for p in alternatives}})
            if group["scenario"] != row["scenario"]:
                raise ValueError("scenario changed between alternatives or processes")
            if not row["decisions"] or len(row["decisions"]) != row["scenario"]["frames"]:
                raise ValueError("missing validation trace")
            for sample in row["samples"]:
                if len(sample["frames"]) != row["scenario"]["frames"]:
                    raise ValueError("truncated measurement trace")
                numbers([sample["setup_ns"], sample["setup_cpu_ns"]])
                if sample["total_ns"] != sample["setup_ns"] + sum(f["total_ns"] for f in sample["frames"]):
                    raise ValueError("trace total does not include cold setup and all frames")
            group["policies"][policy].append(measurements(row))
        if expected is None:
            expected = {(key, policy) for key in grouped for policy in alternatives}
        if not seen or seen != expected:
            raise ValueError("missing scenario or policy")
    enough = len(raws) >= 7
    reviewed = relative is not None or absolute is not None
    result = []
    for key, group in grouped.items():
        candidates = group["policies"]
        means = {p: statistics.mean(r[metric] for r in runs) for p, runs in candidates.items()}
        peak = {p: max(r["peak_raster_bytes"] for r in runs) for p, runs in candidates.items()}
        legal = [p for p in alternatives if max_raster_bytes is None or peak[p] <= max_raster_bytes]
        if not legal or selected not in legal:
            raise ValueError(f"selected policy violates the memory constraint: {key}")
        best = min(legal, key=means.get)
        tolerance = max(absolute or 0, means[selected] * (relative or 0))
        comparisons = []
        for p in legal:
            if p == selected:
                continue
            deltas = [a[metric] - b[metric] for a, b in zip(candidates[selected], candidates[p])]
            bounds = interval(deltas, len(legal) - 1) if enough else [None, None]
            comparisons.append({"alternative": p, "selected_minus_alternative_ms": statistics.mean(deltas),
                                "familywise_delta_interval_95": bounds})
        verdict = "unreviewed" if not reviewed else "inconclusive"
        if reviewed and enough:
            if any(c["familywise_delta_interval_95"][0] > tolerance for c in comparisons):
                verdict = "missed-opportunity"
            elif all(c["familywise_delta_interval_95"][1] <= tolerance for c in comparisons):
                verdict = "near-best-tested"
        result.append({
            "case": key, "scenario": group["scenario"], "verdict": verdict,
            "selected": selected, "selected_ms": means[selected], "best_observed": best, "best_ms": means[best],
            "observed_regret_percent": (means[selected] / means[best] - 1) * 100 if means[best] else None,
            "tolerance_ms": tolerance if reviewed else None,
            "alternatives": [{"policy": p, "eligible": p in legal, "mean_metric_ms": means[p],
                              "cpu_ms_per_trace": statistics.mean(r["cpu_ms_per_trace"] for r in candidates[p]),
                              "peak_raster_bytes": peak[p], "surface_bytes": max(r["surface_bytes"] for r in candidates[p])}
                             for p in alternatives],
            "comparisons": comparisons,
        })
    return {"schema": 1, "objective": metric, "independent_processes": len(raws), "cases": result,
            "scope": "Best among measured fixed policies, not a global optimum or best per-frame schedule. Observed best/regret uses the same data for selection and is exploratory. Confidence comparisons use paired process summaries, Bonferroni-adjusted across alternatives within each scenario. No cross-scenario claim. Raster memory is sampled after completed frames, not transient peak allocation. Timing verdicts are advisory on an uncontrolled workstation."}


def load(path):
    path = Path(path)
    report = read_json(path)
    validate_report(report)
    if report["suite"] != "render-quality" or report["compatibility"]["adapter"] != "quality":
        raise ValueError("expected a complete render-quality report.json")
    raws = []
    for run in report["runs"]:
        source = path.parent / run["raw"]
        if digest(source) != run["raw_sha256"]:
            raise ValueError("raw evidence changed after collection")
        raws.append(read_json(source))
    return report, raws


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--metric", choices=("trace_mean_ms", "frame_p95_ms"), default="trace_mean_ms")
    parser.add_argument("--relative", type=float, help="Reviewed relative tolerance, e.g. 0.05")
    parser.add_argument("--absolute", type=float, help="Reviewed absolute tolerance in milliseconds")
    parser.add_argument("--max-raster-bytes", type=int)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report, raws = load(args.report)
        result = analyze(raws, metric=args.metric, relative=args.relative, absolute=args.absolute,
                         max_raster_bytes=args.max_raster_bytes)
        result["measurement_id"] = report["id"]
        result["provenance"] = report["provenance"]
        write_json(args.output, result, exclusive=True)
    except (OSError, KeyError, TypeError, ValueError) as error:
        print(f"invalid: {error}", file=sys.stderr)
        return 2
    for row in result["cases"]:
        print(f"{row['case']}: {row['verdict']}; selected {row['selected_ms']:.3f} ms, "
              f"best tested {row['best_ms']:.3f} ms ({row['best_observed']})")
    print(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
