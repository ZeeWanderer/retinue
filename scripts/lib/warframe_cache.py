import os
from pathlib import Path


def cache_file(name):
    base = Path(os.environ.get("XDG_CACHE_HOME", ""))
    if not base.is_absolute():
        base = Path.home() / ".cache"
    return base / "retinue" / "games" / "warframe" / name
