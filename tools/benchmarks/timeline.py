"""Publish compact, comparable time series from the immutable benchmark archive."""

import argparse
from collections import defaultdict
from datetime import datetime
import json
from pathlib import Path
from statistics import median

from history import DEFAULT_ROOT, baselines, record_path, series
from reports import read_json


def measurements(report):
    runs = report["runs"]
    for case, first in sorted(runs[0]["cases"].items()):
        for metric in sorted(first["metrics"]):
            yield case, metric, [run["cases"][case]["metrics"][metric] for run in runs]


def export(root, dest):
    groups = defaultdict(list)
    accepted = baselines(root)["series"]
    for path in sorted((root / "records").glob("*.json")):
        report = read_json(path)
        key = series(report)
        if key != report["history"]["series"] or path != record_path(root, report["id"]):
            raise ValueError(f"invalid archived identity: {path}")
        groups[(report["suite"], key)].append(report)
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "records").mkdir(exist_ok=True)
    index = []
    for (suite, key), reports in sorted(groups.items()):
        reports.sort(key=lambda r: (r["created_at"], r["id"]))
        points, values = [], defaultdict(dict)
        for i, r in enumerate(reports):
            points.append({"id": r["id"], "date": r["created_at"], "revision": r["provenance"]["revision"],
                           "binary": r["provenance"].get("binary_sha256"),
                           "dirty": r["provenance"]["dirty"], "runs": len(r["runs"])})
            sample_values = defaultdict(dict)
            for case, metric, samples in measurements(r):
                values[case].setdefault(metric, {})[i] = median(samples)
                sample_values[case][metric] = samples
            public = {**points[-1], "suite": suite, "series": key, "metrics": sample_values}
            (dest / "records" / (r["id"] + ".json")).write_text(json.dumps(public, separators=(",", ":"), allow_nan=False))
        baseline = accepted.get(key, {}).get("record")
        (dest / (key + ".json")).write_text(json.dumps({"points": points, "values": values},
                                                       separators=(",", ":"), allow_nan=False))
        config = reports[0]["compatibility"].get("config", {})
        index.append({"suite": suite, "series": key, "baseline": baseline, "count": len(points),
                      "cpu": reports[0]["machine"].get("cpu"), "threads": config.get("threads"),
                      "samples": config.get("samples"),
                      "first": points[0]["date"], "last": points[-1]["date"]})
    (dest / "index.json").write_text(json.dumps(index, separators=(",", ":")))


def lnt(report):
    key = series(report)
    tests = []
    for case, metric, values in measurements(report):
        if "ms" in metric.split("_"):
            tests.append({"name": f"{report['suite']}/{case}/{metric}",
                          "execution_time": [v / 1000 for v in values]})
    if not tests:
        raise ValueError("no duration metrics for the LNT nts schema")
    timestamp = datetime.fromisoformat(report["created_at"])
    return {"format_version": "2", "machine": {"name": "retinue-" + key},
            "run": {"start_time": timestamp.strftime("%Y-%m-%dT%H:%M:%S"),
                    "llvm_project_revision": str(int(timestamp.timestamp() * 1_000_000)),
                    "source_revision": report["provenance"]["revision"], "measurement": report["id"]},
            "tests": tests}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="command", required=True)
    site = commands.add_parser("site")
    site.add_argument("output", type=Path)
    nts = commands.add_parser("lnt", help="export duration samples as an LLVM LNT v2 nts report")
    nts.add_argument("id")
    cli.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = cli.parse_args()
    if args.command == "site":
        export(args.root, args.output)
    else:
        print(json.dumps(lnt(read_json(record_path(args.root, args.id))), allow_nan=False))
