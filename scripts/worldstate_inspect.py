#!/usr/bin/env python3
import argparse
import json
import re
import sys
from pathlib import Path

from lib.warframe_cache import cache_file


def load_json(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return json.load(handle)


def iter_nodes(obj, path=None):
    if path is None:
        path = []
    if isinstance(obj, dict):
        for key, value in obj.items():
            yield (path + [key], value)
            yield from iter_nodes(value, path + [key])
    elif isinstance(obj, list):
        for idx, value in enumerate(obj):
            yield (path + [f"[{idx}]"] , value)
            yield from iter_nodes(value, path + [f"[{idx}]"])


def format_path(path):
    return ".".join(path)


def collect_matches(obj, key_re=None, value_re=None):
    matches = []
    for path, value in iter_nodes(obj):
        if key_re and path:
            key = str(path[-1])
            if key_re.search(key):
                matches.append((path, value))
                continue
        if value_re and isinstance(value, (str, int, float)):
            if value_re.search(str(value)):
                matches.append((path, value))
    return matches


def main():
    parser = argparse.ArgumentParser(description="Inspect a cached worldstate JSON file.")
    parser.add_argument("path", nargs="?", help="path to worldstate.json")
    parser.add_argument("--keys", help="regex to match key names")
    parser.add_argument("--values", help="regex to match scalar values")
    parser.add_argument("--top-keys", action="store_true", help="print top-level keys")
    parser.add_argument("--syndicate-tags", action="store_true", help="print SyndicateMissions tags")
    parser.add_argument("--calendar", action="store_true", help="print KnownCalendarSeasons summary")
    args = parser.parse_args()

    path = Path(args.path) if args.path else cache_file("worldstate.json")
    if not path.is_file():
        print("error: worldstate.json not found; provide a path", file=sys.stderr)
        return 1

    data = load_json(path)

    if args.top_keys:
        print("top_keys:", " ".join(sorted(data.keys())))

    if args.syndicate_tags:
        tags = sorted({m.get("Tag") for m in data.get("SyndicateMissions", []) if isinstance(m, dict) and m.get("Tag")})
        print("syndicate_tags:")
        for tag in tags:
            print(tag)

    if args.calendar:
        seasons = data.get("KnownCalendarSeasons", [])
        print(f"calendar_seasons: {len(seasons)}")
        for season in seasons:
            if not isinstance(season, dict):
                continue
            name = season.get("Season", "Unknown")
            version = season.get("Version", "?")
            days = season.get("Days", [])
            print(f"- {name} (version {version}) days={len(days)}")

    if args.keys or args.values:
        key_re = re.compile(args.keys) if args.keys else None
        value_re = re.compile(args.values) if args.values else None
        matches = collect_matches(data, key_re=key_re, value_re=value_re)
        for path, value in matches:
            print(f"{format_path(path)} = {value}")

    if not any([args.top_keys, args.syndicate_tags, args.calendar, args.keys, args.values]):
        print("no action specified; try --top-keys, --syndicate-tags, --calendar, --keys, or --values")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
