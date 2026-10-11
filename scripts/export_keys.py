#!/usr/bin/env python3
import json
import sys
from pathlib import Path

from lib.warframe_cache import cache_file


def load_json(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return json.load(handle)


def iter_entries(obj):
    if isinstance(obj, list):
        for item in obj:
            if isinstance(item, dict):
                yield item
    elif isinstance(obj, dict):
        for value in obj.values():
            if isinstance(value, list):
                for item in value:
                    if isinstance(item, dict):
                        yield item


def main():
    if len(sys.argv) < 2:
        print("usage: export_keys.py ExportFile.json [key ...]", file=sys.stderr)
        return 1

    export_file = sys.argv[1]
    keys = sys.argv[2:] or ["uniqueName", "name", "locName"]
    path = Path(export_file)
    if path.parent == Path(".") and not path.exists():
        path = cache_file(export_file)

    if not path.is_file():
        print(f"error: not found: {path}", file=sys.stderr)
        return 1

    data = load_json(path)
    for entry in iter_entries(data):
        for key in keys:
            val = entry.get(key)
            if val:
                print(val)
                break

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
