"""Build an unstaged native app with separate symbols, without runtime probes."""

import argparse
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys

from symbols import output, package, save_manifest


ROOT = Path(__file__).resolve().parents[2]
APPS = ("wfgui", "wfcompanion", "poe2companion", "arccompanion", "retinue-inspect")


def environment(flavor, target_dir):
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    for name in ("CFLAGS", "CXXFLAGS"):
        env[name] = f"{env.get(name, '')} -g".strip()
    baseline = env.get("RETINUE_CPU_BASELINE", "")
    if not baseline and flavor == "prod" and platform.machine() == "x86_64":
        baseline = "x86-64-v2"
    env["RETINUE_CPU_BASELINE"] = baseline
    if "CARGO_ENCODED_RUSTFLAGS" in env:
        flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f") if env["CARGO_ENCODED_RUSTFLAGS"] else []
    else:
        flags = shlex.split(env.get("RUSTFLAGS", ""))
    if baseline:
        flags += [f"-Ctarget-cpu={baseline}", f"-Clink-arg=-march={baseline}"]
    # Overrides stay last even when a local config normally strips debug info.
    flags += ["-Cdebuginfo=2", "-Cstrip=none", "-Csplit-debuginfo=off"]
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    env.pop("RUSTFLAGS", None)
    return env


def command(app, flavor):
    profile = "release" if flavor == "prod" else "dev"
    return [*shlex.split(os.environ.get("CARGO", "cargo")), "build", "--locked",
            "--profile", profile, "-p", app, "--bin", app, "--message-format=json",
            "--config", f"profile.{profile}.debug=2",
            "--config", f'profile.{profile}.strip="none"',
            "--config", f'profile.{profile}.split-debuginfo="off"']


def build(app, flavor, destination, target_dir):
    destination.mkdir(parents=True, exist_ok=False)
    env = environment(flavor, target_dir)
    args = command(app, flavor)
    print(f"Build log: {destination / 'build.log'}", flush=True)
    with (destination / "build.jsonl").open("x") as stdout, (destination / "build.log").open("x") as stderr:
        result = subprocess.run(args, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
    if result.returncode:
        raise ValueError(f"build failed; see {destination / 'build.log'}")
    with (destination / "build.jsonl").open() as stream:
        messages = [json.loads(line) for line in stream if line.startswith("{")]
    artifacts = [row for row in messages if row.get("reason") == "compiler-artifact"
                 and row.get("target", {}).get("name") == app and row.get("executable")]
    if len(artifacts) != 1:
        raise ValueError(f"expected one {app} executable, found {len(artifacts)}")
    artifact = artifacts[0]
    manifest = package(Path(artifact["executable"]), destination)
    manifest["build"] = {
        "app": app, "flavor": flavor, "command": args,
        "profile": artifact["profile"], "features": artifact["features"],
        "environment": {name: env.get(name, "") for name in (
            "CC", "CXX", "CFLAGS", "CXXFLAGS", "LDFLAGS", "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_TARGET", "RETINUE_CPU_BASELINE")},
        "rustc": output(["rustc", "-Vv"]).strip(),
    }
    for name, fallback in (("CC", "cc"), ("CXX", "c++")):
        manifest["build"][name] = output([*shlex.split(env.get(name) or fallback), "--version"]).strip()
    manifest["source"] = {
        "root": str(ROOT),
        "revision": output(["git", "-C", str(ROOT), "rev-parse", "HEAD"]).strip(),
        "dirty": bool(output(["git", "-C", str(ROOT), "status", "--porcelain"])),
        "submodules": output(["git", "-C", str(ROOT), "submodule", "status"]).strip(),
    }
    for row in messages:
        if row.get("reason") == "build-script-executed" and "retinue-render" in row["package_id"]:
            source = Path(row["out_dir"]) / "native" / "compile_commands.json"
            shutil.copy2(source, destination / "native-compile-commands.json")
    save_manifest(destination, manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", choices=APPS, default="wfgui")
    parser.add_argument("--flavor", choices=("dev", "prod"), default="prod")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "_build/profiling")
    args = parser.parse_args()
    try:
        build(args.app, args.flavor, args.output.resolve(), args.target_dir.resolve())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"profile-build: {error}", file=sys.stderr)
        return 1
    print(args.output.resolve() / "symbols.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
