#!/usr/bin/env python3
"""Archive selected measurements and explicitly choose compatible baselines."""

import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib

from compare import EXIT, compare
from reports import read_json, validate_report, write_json


DEFAULT_ROOT = Path(__file__).with_name("history")


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                    allow_nan=False).encode()).hexdigest()


def series(report):
    cases = validate_report(report)
    return fingerprint({"suite": report["suite"], "compatibility": report["compatibility"],
                        "machine": report["machine"], "cases": {
                            key: {name: case[name] for name in ("actual_threads", "work")}
                            for key, case in cases.items()}})


def record_path(root, record_id):
    if not re.fullmatch(r"[a-f0-9]{32}", record_id):
        raise ValueError("expected a measurement ID (32 lowercase hex digits)")
    return root / "records" / (record_id + ".json")


def archive(root, source, note):
    report = read_json(source)
    key = series(report)
    if not note.strip():
        raise ValueError("an archive note is required")
    path = record_path(root, report["id"])
    compact = copy.deepcopy(report)
    provenance = compact["provenance"]
    if "untracked" in provenance:
        provenance["untracked_sha256"] = fingerprint(provenance.pop("untracked"))
    for run in compact["runs"]:
        run.pop("raw", None)
    compact["history"] = {"series": key, "note": note,
                          "archived_at": datetime.now(timezone.utc).isoformat(),
                          "report_sha256": fingerprint(report)}
    path.parent.mkdir(parents=True, exist_ok=True)
    write_json(path, compact, exclusive=True)
    return path


def baselines(root):
    path = root / "baselines.json"
    value = read_json(path) if path.exists() else {"schema": 1, "series": {}}
    if value["schema"] != 1:
        raise ValueError("unsupported baseline index")
    return value


def select(root, record_id, reason):
    report = read_json(record_path(root, record_id))
    key = series(report)
    if report["provenance"]["dirty"]:
        raise ValueError("dirty-worktree measurements can be archived, not selected as baselines")
    if report["expected_runs"] < 7:
        raise ValueError("a baseline needs at least seven independent runs")
    if not reason.strip():
        raise ValueError("baseline changes require a reason")
    index = baselines(root)
    previous = index["series"].get(key, {}).get("record")
    index["series"][key] = {"suite": report["suite"], "record": record_id,
                            "previous": previous, "reason": reason,
                            "selected_at": datetime.now(timezone.utc).isoformat()}
    path = root / "baselines.json"
    pending = path.with_suffix(".json.new")
    write_json(pending, index, exclusive=True)
    pending.replace(path)
    return path


def baseline_for(root, candidate):
    key = series(candidate)
    entry = baselines(root)["series"].get(key)
    if entry is None:
        raise ValueError("no accepted baseline for this workload and environment")
    report = read_json(record_path(root, entry["record"]))
    if series(report) != key:
        raise ValueError("baseline index does not match its report")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    commands = parser.add_subparsers(dest="command", required=True)
    record = commands.add_parser("archive", help="Copy compact results, not raw traces or executables")
    record.add_argument("report", type=Path)
    record.add_argument("--note", required=True)
    baseline = commands.add_parser("baseline", help="Explicitly select an archived clean measurement")
    baseline.add_argument("id")
    baseline.add_argument("--reason", required=True)
    check = commands.add_parser("compare", help="Compare against the accepted compatible baseline")
    check.add_argument("report", type=Path)
    check.add_argument("--policy", type=Path, required=True)
    check.add_argument("--output", type=Path)
    commands.add_parser("list", help="List accepted baselines")
    args = parser.parse_args()
    try:
        if args.command == "archive":
            print(archive(args.root, args.report, args.note))
        elif args.command == "baseline":
            print(select(args.root, args.id, args.reason))
        elif args.command == "list":
            for key, entry in sorted(baselines(args.root)["series"].items()):
                print(f"{entry['suite']:24} {key[:12]} {entry['record']} {entry['reason']}")
        else:
            candidate = read_json(args.report)
            with args.policy.open("rb") as stream:
                result = compare(baseline_for(args.root, candidate), candidate, tomllib.load(stream))
            if args.output:
                write_json(args.output, result, exclusive=True)
            print(result["verdict"])
            for row in result["cases"]:
                print(f"{row['verdict']:12} {row['case']}: {row['before']:.4g} -> {row['after']:.4g}")
            return EXIT[result["verdict"]]
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"invalid: {error}", file=sys.stderr)
        return EXIT["invalid"]
    return 0


if __name__ == "__main__":
    sys.exit(main())
