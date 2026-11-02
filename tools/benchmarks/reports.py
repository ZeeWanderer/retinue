"""Normalize maintained workloads; keep native reports as the detailed evidence."""

import hashlib
import json
import math
from pathlib import Path
import statistics


SCHEMA = 1
ROOT = Path(__file__).resolve().parents[2]


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, value, *, exclusive=False):
    text = json.dumps(value, indent=2, allow_nan=False) + "\n"
    with Path(path).open("x" if exclusive else "w") as stream:
        stream.write(text)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fixture_digest(patterns):
    files = set()
    for pattern in patterns:
        matches = {path for path in ROOT.glob(pattern) if path.is_file()}
        if not matches:
            raise ValueError(f"fixture pattern matches no files: {pattern}")
        files.update(matches)
    entries = {str(p.relative_to(ROOT)): digest(p) for p in sorted(files)}
    return hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest()


def numbers(values):
    values = list(values)
    if not values or any(isinstance(v, bool) or not isinstance(v, (int, float))
                         or not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("measurements must be nonempty, finite, nonnegative numbers")
    return values


def percentile(values, fraction):
    ordered = sorted(numbers(values))
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def identity(row):
    fields = ("case", "size", "width", "height", "scale", "mode", "over_grid",
              "bounding_damage", "events_per_frame")
    return "/".join(f"{k}={json.dumps(row[k], separators=(',', ':'))}"
                    for k in fields if k in row)


def normalize(raw, adapter):
    rows = raw["cases"] if adapter == "overlay" else raw
    if not isinstance(rows, list) or not rows:
        raise ValueError("benchmark emitted no cases")
    cases = {}
    for row in rows:
        metrics = {}
        if adapter == "overlay":
            samples = numbers([n / 1e6 for n in row["warm"]["frame_ns"]])
            metrics["first_paint_ms"] = row["first_paint_ns"] / 1e6
            metrics["process_cpu_ms_per_frame"] = row["warm"]["process_cpu_ns"] / 1e6 / len(samples)
            metrics["rss_bytes"] = row["process_memory_after"]["rss_bytes"]
        elif adapter == "foundry":
            samples = row["samples_ms"]
        elif adapter == "gui":
            samples = [s["total_ms"] for s in row["samples"]]
            for field in ("layout_ms", "paint_ms", "input_ms", "submit_ms", "finish_ms",
                          "process_cpu_ms", "update_ms"):
                if row["samples"] and all(field in s for s in row["samples"]):
                    metrics[field + "_p95"] = percentile([s[field] for s in row["samples"]], .95)
        else:
            raise ValueError(f"unknown report adapter: {adapter}")
        samples = numbers(samples)
        metrics.update(frame_p50_ms=percentile(samples, .5),
                       frame_p95_ms=percentile(samples, .95),
                       frame_max_ms=max(samples), frame_mean_ms=statistics.mean(samples))
        numbers(metrics.values())
        key = identity(row)
        if not key or key in cases:
            raise ValueError(f"missing or duplicate case identity: {key}")
        cases[key] = {"frame_samples": len(samples), "metrics": metrics,
                      "actual_threads": row.get("actual_threads", row.get("known_resources", {}).get("render_threads")),
                      "work": {k: row[k] for k in ("items", "total_cards", "visible_cards",
                               "columns", "live") if k in row}}
    return cases


def validate_report(report, *, complete=True):
    if report["schema"] != SCHEMA or report["purpose"] != "regression":
        raise ValueError("not a maintained regression report")
    runs = report["runs"]
    if complete and (report["state"] != "complete" or len(runs) != report["expected_runs"]):
        raise ValueError("incomplete measurement session")
    if not runs or not runs[0]["cases"]:
        raise ValueError("no measured runs/cases")
    expected = runs[0]["cases"]
    for run in runs:
        if run["cases"].keys() != expected.keys():
            raise ValueError("case set changed between runs")
        for key, case in run["cases"].items():
            if case["metrics"].keys() != expected[key]["metrics"].keys():
                raise ValueError(f"metrics changed between runs: {key}")
            numbers(case["metrics"].values())
            if not isinstance(case["frame_samples"], int) or case["frame_samples"] < 1:
                raise ValueError("invalid frame sample count")
            for field in ("actual_threads", "work"):
                if case[field] != expected[key][field]:
                    raise ValueError(f"{field} changed between runs: {key}")
    return expected
