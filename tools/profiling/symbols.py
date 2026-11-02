"""Package an ELF executable and its matching separate debug information."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def tool(variable, *names):
    if path := os.environ.get(variable):
        return path
    for name in names:
        if path := shutil.which(name):
            return path
    raise ValueError(f"install {names[0]} or set {variable}")


def output(command):
    return subprocess.check_output(command, text=True, env={**os.environ, "LC_ALL": "C"})


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build_id(readelf, binary):
    match = re.search(r"Build ID:\s*([0-9a-fA-F]{4,})", output([readelf, "-n", str(binary)]))
    if not match:
        raise ValueError(f"{binary}: missing ELF build ID; link with --build-id")
    return match[1].lower()


def package(binary, destination):
    """Write into an exclusively owned output directory; never mutate the input."""
    binary = binary.resolve(strict=True)
    objcopy = tool("OBJCOPY", "llvm-objcopy", "objcopy")
    readelf = tool("READELF", "llvm-readelf", "readelf")
    executable = destination / binary.name
    if executable.exists() or binary.name in {"symbols.json", "build.jsonl", "build.log"}:
        raise ValueError(f"output file already exists or is reserved: {executable}")
    with binary.open("rb") as stream:
        if stream.read(4) != b"\x7fELF":
            raise ValueError(f"{binary}: expected an ELF executable")
    shutil.copy2(binary, executable)
    sections = output([readelf, "-SW", str(executable)])
    if not re.search(r"\.(?:debug|zdebug)_info\b", sections):
        raise ValueError(f"{binary}: no debug information; rebuild with debug enabled and strip disabled")
    identity = build_id(readelf, executable)
    original_hash = digest(executable)
    debug = destination / f"{binary.name}.debug"
    subprocess.run([objcopy, "--only-keep-debug", str(executable), str(debug)], check=True)
    subprocess.run([objcopy, "--strip-all", str(executable)], check=True)
    subprocess.run([objcopy, f"--add-gnu-debuglink={debug}", str(executable)], check=True)
    if build_id(readelf, executable) != identity or build_id(readelf, debug) != identity:
        raise ValueError("ELF build ID changed while separating debug information")
    index = destination / ".build-id" / identity[:2] / f"{identity[2:]}.debug"
    index.parent.mkdir(parents=True)
    index.symlink_to(Path("../..") / debug.name)
    return {
        "schema": "retinue.symbols.v1",
        "created_at": datetime.now(timezone.utc).isoformat(),
        "source_binary": str(binary),
        "source_sha256": original_hash,
        "build_id": identity,
        "executable": {"path": executable.name, "sha256": digest(executable)},
        "debug": {"path": debug.name, "sha256": digest(debug)},
    }


def save_manifest(destination, manifest):
    with (destination / "symbols.json").open("x") as stream:
        json.dump(manifest, stream, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        destination = args.output.resolve()
        destination.mkdir(parents=True, exist_ok=False)
        save_manifest(destination, package(args.binary, destination))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"symbols: {error}", file=sys.stderr)
        return 1
    print(destination / "symbols.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
