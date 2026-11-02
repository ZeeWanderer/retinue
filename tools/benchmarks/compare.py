#!/usr/bin/env python3
"""Compare independent process runs, not correlated frames within one run."""

import argparse
import fnmatch
import math
import random
import statistics
import sys
import tomllib

from reports import read_json, validate_report, write_json


EXIT = {"pass": 0, "regression": 1, "invalid": 2, "inconclusive": 3}


def interval(before, after, seed=0):
    rng = random.Random(seed)
    deltas = sorted(statistics.mean(rng.choices(after, k=len(after)))
                    - statistics.mean(rng.choices(before, k=len(before)))
                    for _ in range(4000))
    return deltas[100], deltas[3899]


def rule_for(policy, suite, case):
    rules = [rule for rule in policy["rule"]
             if rule["suite"] == suite and (case == rule["case"] or fnmatch.fnmatchcase(case, rule["case"]))]
    if len(rules) != 1:
        raise ValueError(f"expected exactly one policy rule for {suite}: {case}")
    rule = rules[0]
    for field in ("relative", "absolute"):
        value = rule[field]
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError(f"invalid {field} tolerance")
    if rule["relative"] == rule["absolute"] == 0:
        raise ValueError("policy needs a practical nonzero tolerance")
    return rule


def compare(before, after, policy):
    old, new = validate_report(before), validate_report(after)
    if policy["schema"] != 1:
        raise ValueError("unsupported policy schema")
    for field in ("suite", "compatibility", "machine"):
        if before[field] != after[field]:
            raise ValueError(f"incompatible {field}")
    if old.keys() != new.keys():
        raise ValueError("missing or added cases; review the workload before comparison")
    minimum_runs = policy.get("minimum_runs", 7)
    if not isinstance(minimum_runs, int) or minimum_runs < 7:
        raise ValueError("minimum_runs must be at least 7 independent processes")
    rows = []
    for key in sorted(old):
        for field in ("actual_threads", "work"):
            if old[key][field] != new[key][field]:
                raise ValueError(f"incompatible {field}: {key}")
        rule = rule_for(policy, before["suite"], key)
        metric = rule["metric"]
        a = [r["cases"][key]["metrics"][metric] for r in before["runs"]]
        b = [r["cases"][key]["metrics"][metric] for r in after["runs"]]
        baseline, candidate = statistics.mean(a), statistics.mean(b)
        tolerance = max(rule["absolute"], baseline * rule["relative"])
        enough = min(len(a), len(b)) >= minimum_runs
        if metric.startswith("frame_"):
            required = rule.get("minimum_frames", 60)
            if not isinstance(required, int) or required < 30:
                raise ValueError("minimum_frames must be at least 30")
            enough &= all(r["cases"][key]["frame_samples"] >= required
                          for report in (before, after) for r in report["runs"])
        low, high = interval(a, b) if enough else (None, None)
        verdict = ("inconclusive" if not enough else "regression" if low > tolerance
                   else "pass" if high <= tolerance else "inconclusive")
        rows.append({"case": key, "metric": metric, "before": baseline, "after": candidate,
                     "tolerance": tolerance, "delta_interval_95": [low, high], "verdict": verdict})
    verdict = ("regression" if any(r["verdict"] == "regression" for r in rows)
               else "inconclusive" if any(r["verdict"] == "inconclusive" for r in rows) else "pass")
    return {"verdict": verdict, "cases": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", help="Baseline report.json (never modified)")
    parser.add_argument("after", help="Candidate report.json")
    parser.add_argument("--policy", required=True, help="Reviewed TOML tolerances in metric units")
    parser.add_argument("--output", help="Optional machine-readable verdict")
    args = parser.parse_args()
    try:
        with open(args.policy, "rb") as stream:
            policy = tomllib.load(stream)
        result = compare(read_json(args.before), read_json(args.after), policy)
    except (ValueError, KeyError, TypeError, OSError) as error:
        result = {"verdict": "invalid", "reason": str(error)}
    if args.output:
        try:
            write_json(args.output, result, exclusive=True)
        except OSError as error:
            result = {"verdict": "invalid", "reason": str(error)}
    print(result["verdict"] + (": " + result["reason"] if "reason" in result else ""))
    for row in result.get("cases", []):
        print(f"{row['verdict']:12} {row['case']} {row['metric']}: "
              f"{row['before']:.4g} -> {row['after']:.4g} "
              f"(allowed +{row['tolerance']:.4g}, 95% delta {row['delta_interval_95']})")
    return EXIT[result["verdict"]]


if __name__ == "__main__":
    sys.exit(main())
