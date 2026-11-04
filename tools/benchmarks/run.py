#!/usr/bin/env python3
"""Build once, then measure maintained native benchmarks in fresh processes."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import time
import tomllib
import uuid

from reports import ROOT, SCHEMA, digest, fixture_digest, normalize, read_json, validate_report, write_json


CATALOG = Path(__file__).with_name("suites.toml")
BUILD_ENV = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CC", "CXX", "CFLAGS", "CXXFLAGS",
             "LDFLAGS", "LLVM_ROOT", "WFCLI_CPU_BASELINE")
MEASURE_ENV = ("LD_PRELOAD", "LD_LIBRARY_PATH", "MALLOC_CONF", "MALLOC_ARENA_MAX",
               "MIMALLOC_VERBOSE", "MIMALLOC_PURGE_DELAY")


def catalog():
    with CATALOG.open("rb") as stream:
        return tomllib.load(stream)


def output(command):
    return subprocess.check_output(command, cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()


def clean_environment():
    return {k: v for k, v in os.environ.items()
            if not k.startswith(("RETINUE_BENCH_", "GUI_BENCH_", "RETINUE_REPAIR_"))}


def executable(messages):
    found = {row["executable"] for row in messages
             if row.get("reason") == "compiler-artifact" and row.get("profile", {}).get("test")
             and row.get("executable")}
    if len(found) != 1:
        raise ValueError(f"expected one test executable, found {len(found)}")
    return Path(found.pop())


def check_selector(binary, selector, env):
    listing = subprocess.check_output([str(binary), "--list", "--ignored", "--format=terse"],
                                      env=env, cwd=ROOT, text=True)
    names = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
    if names.count(selector) != 1:
        raise ValueError(f"exact ignored test not found: {selector}; check suites.toml")


def prepare(suite_id, destination):
    spec = catalog()["suite"][suite_id]
    destination = destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    command = [*shlex.split(os.environ.get("CARGO", "cargo")), "test", "--locked", "--release",
               "--no-run", "--message-format=json", "-p", spec["package"], *spec["target"]]
    env = clean_environment()
    with (destination / "build.jsonl").open("w") as stdout, (destination / "build.log").open("w") as stderr:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
    if result.returncode:
        raise ValueError(f"build failed; see {destination / 'build.log'}")
    with (destination / "build.jsonl").open() as stream:
        artifacts = [json.loads(line) for line in stream if line.startswith("{")]
    binary = executable(artifacts)
    artifact = next(row for row in artifacts if row.get("executable") == str(binary))
    if artifact["profile"]["debug_assertions"] or artifact["profile"]["opt_level"] == "0":
        raise ValueError("performance workloads require optimized builds without debug assertions")
    check_selector(binary, spec["test"], env)
    shutil.copy2(binary, destination / "benchmark")
    patch = subprocess.check_output(["git", "diff", "--binary", "HEAD"], cwd=ROOT)
    untracked = output(["git", "ls-files", "-z", "--others", "--exclude-standard"]).split("\0")
    source = {p: digest(ROOT / p) for p in untracked if (ROOT / p).is_file()}
    provenance = {
        "revision": output(["git", "rev-parse", "HEAD"]),
        "patch_sha256": hashlib.sha256(patch).hexdigest(), "untracked": source,
        "submodules": output(["git", "submodule", "status"]),
        "binary_sha256": digest(destination / "benchmark"),
        "dirty": bool(output(["git", "status", "--porcelain"])),
        "external_assets": {str(p.relative_to(ROOT)): digest(p)
                            for p in (ROOT / ".cache/fonts/fontin").glob("*.ttf")}
                           if spec["package"] == "poe2companion" else {},
    }
    settings = {k: os.environ.get(k, "") for k in BUILD_ENV}
    settings["rustc"] = output(["rustc", "-Vv"])
    settings["profile"] = "release"
    settings["cargo_profile"] = artifact["profile"]
    settings["features"] = artifact["features"]
    settings["profile_environment"] = {k: v for k, v in os.environ.items()
                                       if k.startswith("CARGO_PROFILE_") or k == "CARGO_BUILD_TARGET"}
    settings["cargo_target"] = spec["target"]
    for compiler, fallback in (("CC", "cc"), ("CXX", "c++")):
        settings[compiler + "_version"] = output([*shlex.split(settings[compiler] or fallback), "--version"])
    snapshot = {"schema": SCHEMA, "suite": suite_id, "spec": spec, "provenance": provenance,
                "build": settings, "fixture_sha256": fixture_digest(spec["fixtures"])}
    write_json(destination / "source-manifest.json", source)
    (destination / "source.patch").write_bytes(patch)
    write_json(destination / "snapshot.json", snapshot)
    return destination


def machine():
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")), platform.machine())
    governors = {p.read_text().strip() for p in Path("/sys/devices/system/cpu").glob(
        "cpu*/cpufreq/scaling_governor")}
    return {"host": platform.node(), "kernel": platform.release(), "cpu": cpu,
            "affinity": sorted(os.sched_getaffinity(0)), "governors": sorted(governors),
            "locale": {k: os.environ.get(k, "") for k in ("LANG", "LC_ALL", "TZ")},
            "allocator_environment": {k: os.environ.get(k, "") for k in MEASURE_ENV}}


def open_report(snapshot_path, destination, config, repeats):
    snapshot_path, destination = snapshot_path.resolve(), destination.resolve()
    snapshot = read_json(snapshot_path / "snapshot.json")
    if snapshot["schema"] != SCHEMA or digest(snapshot_path / "benchmark") != snapshot["provenance"]["binary_sha256"]:
        raise ValueError("snapshot binary identity does not match its manifest")
    check_selector(snapshot_path / "benchmark", snapshot["spec"]["test"], clean_environment())
    destination.mkdir(parents=True, exist_ok=False)
    report = {"schema": SCHEMA, "suite": snapshot["suite"], "purpose": "regression",
              "id": uuid.uuid4().hex, "created_at": datetime.now(timezone.utc).isoformat(),
              "environment": "headless", "state": "recording", "expected_runs": repeats,
              "provenance": snapshot["provenance"], "machine": machine(),
              "compatibility": {"workload": snapshot["spec"]["workload"],
                                "adapter": snapshot["spec"]["adapter"],
                                "fixture_sha256": snapshot["fixture_sha256"],
                                "external_assets": snapshot["provenance"].get("external_assets", {}),
                                "build": snapshot["build"], "config": config}, "runs": []}
    return {"snapshot": snapshot, "binary": snapshot_path / "benchmark",
            "directory": destination, "report": report}


def run_once(job):
    directory, report, spec = job["directory"], job["report"], job["snapshot"]["spec"]
    config = report["compatibility"]["config"]
    run_dir = directory / f"run-{len(report['runs']) + 1:02d}"
    run_dir.mkdir()
    env = clean_environment()
    env.update(RETINUE_BENCH_OUTPUT=str(run_dir), RETINUE_BENCH_SAMPLES=str(config["samples"]),
               RETINUE_BENCH_THREADS=str(config["threads"]), RETINUE_BENCH_HZ=str(config["hz"]),
               RETINUE_BENCH_CASES=config["cases"], GUI_BENCH_OUTPUT=str(run_dir / spec["raw"]),
               GUI_BENCH_THREADS=str(config["threads"]), GUI_BENCH_LIVE="0")
    env["RETINUE_BENCH_REVISION"] = report["provenance"]["revision"]
    env["RETINUE_BENCH_DIRTY"] = str(report["provenance"]["dirty"]).lower()
    env["RETINUE_BENCH_RUSTC"] = report["compatibility"]["build"]["rustc"]
    for name in ("CACHE", "CONFIG", "STATE"):
        path = run_dir / name.lower()
        path.mkdir()
        env[f"XDG_{name}_HOME"] = str(path)
    start = time.monotonic()
    with (run_dir / "process.log").open("w") as log:
        result = subprocess.run([str(job["binary"]), spec["test"], "--exact", "--ignored",
                                 "--nocapture", "--test-threads=1"], cwd=ROOT, env=env,
                                stdout=log, stderr=subprocess.STDOUT, timeout=config["timeout"])
    if result.returncode:
        raise ValueError(f"benchmark failed; see {run_dir / 'process.log'}")
    raw_path = run_dir / spec["raw"]
    run = {"raw": str(raw_path.relative_to(directory)), "raw_sha256": digest(raw_path),
           "elapsed_seconds": time.monotonic() - start, "load_average": os.getloadavg(),
           "cases": normalize(read_json(raw_path), spec["adapter"])}
    report["runs"].append(run)
    validate_report(report, complete=False)
    if machine() != report["machine"]:
        raise ValueError("machine configuration changed while measuring")
    write_json(directory / "report.json", report)
    print(f"{report['suite']}: run {len(report['runs'])}, {len(run['cases'])} cases", flush=True)


def balanced_order(rounds):
    return [label for index in range(rounds) for label in ((0, 1) if index % 2 == 0 else (1, 0))]


def finish_report(job):
    report = job["report"]
    report["state"] = "complete"
    report["completed_at"] = datetime.now(timezone.utc).isoformat()
    validate_report(report)
    write_json(job["directory"] / "report.json", report)
    print(job["directory"] / "report.json")


def measured_config(args, spec):
    if not 1 <= args.threads <= 8 or not 10 <= args.samples <= 5000 or args.repeats < 1:
        raise ValueError("threads: 1..8; samples: 10..5000; repeats: at least 1")
    if not 0 <= args.hz <= 1000 or args.timeout < 1:
        raise ValueError("hz: 0..1000; timeout: positive seconds")
    if spec["adapter"] != "overlay" and args.hz:
        raise ValueError("cadence is supported only by overlay workloads")
    if spec["adapter"] not in ("overlay", "quality") and args.cases:
        raise ValueError("case filters are supported only by overlay and optimizer-quality workloads")
    if args.cases:
        selected = args.cases.split(",")
        if len(selected) != len(set(selected)) or any(case not in spec["cases"] for case in selected):
            raise ValueError("unknown or duplicate case; see suites.toml")
    return {"threads": args.threads, "samples": args.samples, "hz": args.hz,
            "cases": args.cases, "timeout": args.timeout, "cache": "fresh-process-and-xdg-per-run"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list", help="List maintained workloads, experiments and live checks")
    prep = commands.add_parser("prepare", help="Save a release test binary with provenance")
    prep.add_argument("suite", choices=sorted(catalog()["suite"]))
    prep.add_argument("--output", type=Path, required=True)
    for name in ("record", "pair"):
        sub = commands.add_parser(name, help={"record": "Measure one build", "pair": "Interleave two prepared builds (ABBA)"}[name])
        if name != "pair":
            sub.add_argument("suite", choices=sorted(catalog()["suite"]))
        else:
            sub.add_argument("before", type=Path)
            sub.add_argument("after", type=Path)
        sub.add_argument("--output", type=Path, required=True)
        sub.add_argument("--repeats", type=int, default=7)
        sub.add_argument("--threads", type=int, default=4)
        sub.add_argument("--samples", type=int, default=120, help="Warm overlay frames or complete quality traces per policy; GUI traces have fixed length")
        sub.add_argument("--hz", type=int, default=0)
        sub.add_argument("--cases", default="", help="Comma-separated overlay or optimizer-quality case names")
        sub.add_argument("--timeout", type=int, default=600, help="Seconds per independent process")
    args = parser.parse_args()
    try:
        if args.command == "list":
            for name, spec in catalog()["suite"].items():
                print(f"{name:24} regression  headless             {spec['owner']}")
            for spec in catalog()["manual"]:
                print(f"{spec['id']:24} {spec['purpose']:11} {spec['environment']:20} {spec['command']}")
        elif args.command == "prepare":
            print(prepare(args.suite, args.output))
        else:
            if args.command == "record":
                config = measured_config(args, catalog()["suite"][args.suite])
                args.output.mkdir(parents=True, exist_ok=False)
                snapshot = prepare(args.suite, args.output / "snapshot")
                jobs = [open_report(snapshot, args.output / "measurements", config, args.repeats)]
                order = [0] * args.repeats
            else:
                snapshots = [read_json(p / "snapshot.json") for p in (args.before, args.after)]
                config = measured_config(args, snapshots[0]["spec"])
                args.output.mkdir(parents=True, exist_ok=False)
                jobs = [open_report(path, args.output / label, config, args.repeats)
                        for path, label in ((args.before, "before"), (args.after, "after"))]
                for field in ("suite", "compatibility"):
                    if jobs[0]["report"][field] != jobs[1]["report"][field]:
                        raise ValueError(f"incompatible {field}; refusing paired timing")
                order = balanced_order(args.repeats)
            for index in order:
                run_once(jobs[index])
            for job in jobs:
                finish_report(job)
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        print(f"invalid: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
