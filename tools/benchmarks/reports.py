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
    fields = ("case", "candidate", "size", "width", "height", "scale", "mode", "over_grid",
              "bounding_damage", "events_per_frame")
    return "/".join(f"{k}={json.dumps(row[k], separators=(',', ':'))}"
                    for k in fields if k in row)


def phase_partition(columns, totals, count, prefix):
    if totals is None or len(totals) != count or any(int(n) != n for n in numbers(totals)):
        raise ValueError(f"invalid {prefix} phase totals")
    for name, values in columns.items():
        if (not name.isidentifier() or len(values) != count
                or any(int(n) != n for n in numbers(values))):
            raise ValueError(f"invalid {prefix} phase: {name}")
    if any(sum(values[i] for values in columns.values()) != t for i, t in enumerate(totals)):
        raise ValueError(f"{prefix} phases must partition their parent")
    return {f"{prefix}_{name}_mean_ms": statistics.mean(values) / 1e6
            for name, values in columns.items()}


def ir_cohorts(detail, exclusive, counts):
    metrics = {}
    for changed, cohort in ((False, "stable"), (True, "structural")):
        indices = [i for i, visits in enumerate(counts["ir_topology"]) if bool(visits) == changed]
        prefix = f"ir_{cohort}_"
        metrics[prefix + "frames"] = len(indices)
        if indices:
            for name, values in {**detail, "total": exclusive["ir"]}.items():
                metrics[prefix + name + "_mean_ms"] = statistics.mean(values[i] for i in indices) / 1e6
            for name in ("composite", "renew"):
                if (values := counts.get("ir_" + name)) is not None:
                    metrics[prefix + name + "_calls_mean"] = statistics.mean(values[i] for i in indices)
    return metrics


def normalize(raw, adapter):
    rows = raw["cases"] if adapter in ("overlay", "quality") else raw
    if not isinstance(rows, list) or not rows:
        raise ValueError("benchmark emitted no cases")
    cases = {}
    for row in rows:
        metrics = {}
        sample_unit = "frame"
        if adapter == "overlay":
            samples = numbers([n / 1e6 for n in row["warm"]["frame_ns"]])
            metrics["first_paint_ms"] = row["first_paint_ns"] / 1e6
            metrics["process_cpu_ms_per_frame"] = row["warm"]["process_cpu_ns"] / 1e6 / len(samples)
            metrics["rss_bytes"] = row["process_memory_after"]["rss_bytes"]
            for name, values in row.get("known_resources", {}).get("phases_ns", {}).items():
                if not name.isidentifier() or len(values) != len(samples):
                    raise ValueError("invalid phase name or sample count")
                values = [n / 1e6 for n in numbers(values)]
                for suffix, fraction in (("p50", .5), ("p95", .95)):
                    metrics[f"phase_{name}_{suffix}_ms"] = percentile(values, fraction)
            exclusive = row.get("known_resources", {}).get("owner_exclusive_ns", {})
            if exclusive:
                metrics.update(phase_partition({k: v for k, v in exclusive.items() if k != "total"},
                                               exclusive.get("total"), len(samples), "exclusive"))
                metrics["exclusive_total_mean_ms"] = statistics.mean(exclusive["total"]) / 1e6
            ir_detail = row.get("known_resources", {}).get("owner_ir_ns", {})
            if ir_detail:
                metrics.update(phase_partition(ir_detail, exclusive.get("ir"), len(samples), "ir"))
            layout = row.get("known_resources", {}).get("layout_job_ns", {})
            if layout:
                wait = {name.removeprefix("layout_wait_"): v for name, v in layout.items()
                        if name.startswith("layout_wait_")}
                metrics.update(phase_partition(wait, exclusive.get("layout_wait"), len(samples), "layout_wait"))
                for key, raw_values in layout.items():
                    if key.startswith("layout_wait_"):
                        continue
                    values = numbers(raw_values)
                    if not key.startswith("layout_") or not key.isidentifier() or len(values) != len(samples):
                        raise ValueError("invalid layout job sample count")
                    metrics[f"{key}_mean_ms"] = statistics.mean(values) / 1e6
            counts = row.get("known_resources", {}).get("work_counts", {})
            for name, values in counts.items():
                if not name.isidentifier() or len(values) != len(samples):
                    raise ValueError("invalid work counter name or sample count")
                values = numbers(values)
                if any(n < 0 or int(n) != n for n in values):
                    raise ValueError("work counters must be nonnegative integers")
                metrics[f"work_{name}_p50"] = percentile(values, .5)
                metrics[f"work_{name}_mean"] = statistics.mean(values)
            if ir_detail and "ir_topology" in counts:
                metrics.update(ir_cohorts(ir_detail, exclusive, counts))
        elif adapter == "foundry":
            samples = row["samples_ms"]
        elif adapter == "gui":
            samples = [s["total_ms"] for s in row["samples"]]
            for field in ("layout_ms", "paint_ms", "input_ms", "submit_ms", "finish_ms",
                          "process_cpu_ms", "update_ms"):
                if row["samples"] and all(field in s for s in row["samples"]):
                    metrics[field + "_p95"] = percentile([s[field] for s in row["samples"]], .95)
        elif adapter == "quality":
            sample_unit = "trace"
            samples = [s["total_ns"] / 1e6 for s in row["samples"]]
            metrics["process_cpu_ms_per_trace"] = statistics.mean(numbers(s["cpu_ns"] / 1e6 for s in row["samples"]))
            metrics["peak_raster_bytes"] = max(numbers(s["peak_raster_bytes"] for s in row["samples"]))
            metrics["surface_bytes"] = max(numbers(s["surface_bytes"] for s in row["samples"]))
        else:
            raise ValueError(f"unknown report adapter: {adapter}")
        samples = numbers(samples)
        metrics.update({f"{sample_unit}_p50_ms": percentile(samples, .5),
                        f"{sample_unit}_p95_ms": percentile(samples, .95),
                        f"{sample_unit}_max_ms": max(samples),
                        f"{sample_unit}_mean_ms": statistics.mean(samples)})
        numbers(metrics.values())
        key = identity(row)
        if not key or key in cases:
            raise ValueError(f"missing or duplicate case identity: {key}")
        frames = sum(len(s["frames"]) for s in row["samples"]) if adapter == "quality" else len(samples)
        cases[key] = {"frame_samples": frames, "metrics": metrics,
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
