#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path

from lib.warframe_cache import cache_file


def load_json(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return json.load(handle)


def main():
    parser = argparse.ArgumentParser(description="Lookup a key in languages.json.")
    parser.add_argument("key", help="language key to lookup")
    parser.add_argument("--file", help="path to languages.json")
    args = parser.parse_args()

    path = Path(args.file) if args.file else cache_file("languages.json")
    if not path.is_file():
        print("error: languages.json not found", file=sys.stderr)
        return 1

    data = load_json(path)
    key = args.key
    if key in data:
        print(data[key])
        return 0

    print("not found")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
