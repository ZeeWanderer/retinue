#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def cases(report):
    return {
        (case["case"], tuple(case["size"]), case["scale"]): case
        for case in report["cases"]
    }


def compare(before, after):
    old, new = cases(before), cases(after)
    lines = []
    for field in ("cpu", "rustc", "profile", "kernel"):
        if before[field] != after[field]:
            lines.append(f"Note: {field} differs between runs.")
    if before.get("cadence_hz", 0) != after.get("cadence_hz", 0):
        lines.append("Note: repaint cadence differs between runs.")
    lines.append("Case / pixels / scale                 p95 ms before -> after    change")
    for key in sorted(old.keys() & new.keys()):
        a, b = old[key], new[key]
        previous, current = a["warm"]["p95_ns"], b["warm"]["p95_ns"]
        change = f"{(current / previous - 1) * 100:+.1f}%" if previous else "n/a"
        name, (width, height), scale = key
        label = f"{name} {width}x{height} @{scale:g}"
        lines.append(f"{label:<38} {previous / 1e6:8.3f} -> {current / 1e6:8.3f} {change:>8}")
    for key in sorted(old.keys() ^ new.keys()):
        lines.append(f"Unmatched case: {key}")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Compare two headless overlay benchmark runs.")
    parser.add_argument("before", type=Path, help="Earlier report directory")
    parser.add_argument("after", type=Path, help="Later report directory")
    args = parser.parse_args()
    reports = sorted(path.name for path in args.before.glob("*.json"))
    if not reports:
        parser.error("no reports found in the earlier directory")
    for name in reports:
        current = args.after / name
        if not current.exists():
            print(f"{name}: missing from later run")
            continue
        before = json.loads((args.before / name).read_text())
        after = json.loads(current.read_text())
        if before["schema"] != 1 or after["schema"] != 1:
            parser.error(f"{name}: unsupported report schema")
        print(f"\n{before['game']}\n{compare(before, after)}")


if __name__ == "__main__":
    main()
