#!/usr/bin/env python3
"""Fetch the author-supplied font for program embedding, not redistribution."""

import fcntl
import hashlib
import io
from pathlib import Path
import sys
import tempfile
import urllib.request
import zipfile


URL = "https://www.exljbris.com/dl/fontin2_pc.zip"
ARCHIVE_SHA256 = "d5dae334a8a01fe0ee817bb94a1a567d0128827818f72418735e1f1fc0738d62"
FONT_SHA256 = "b63c2c7923d6f72348f5293d47909f0f3d5726a338935ad5897512c4e2a8481a"
FONT = "Fontin-SmallCaps.ttf"
CACHE = Path(__file__).resolve().parent.parent / ".cache" / "fonts" / "fontin"


def verified(data, expected):
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("Fontin checksum mismatch; refusing an unverified font")
    return data


def download():
    with urllib.request.urlopen(URL, timeout=30) as response:
        return response.read(1024 * 1024)


def prepare(cache=CACHE, fetch=download):
    cache.mkdir(parents=True, exist_ok=True)
    font = cache / FONT
    with (cache / ".lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if font.exists() and hashlib.sha256(font.read_bytes()).hexdigest() == FONT_SHA256:
            return font
        archive = verified(fetch(), ARCHIVE_SHA256)
        with zipfile.ZipFile(io.BytesIO(archive)) as package:
            data = verified(package.read(FONT), FONT_SHA256)
        with tempfile.NamedTemporaryFile(dir=cache, delete=False) as output:
            pending = Path(output.name)
            try:
                output.write(data)
                output.close()
                pending.replace(font)
            finally:
                pending.unlink(missing_ok=True)
    return font


if __name__ == "__main__":
    try:
        print(prepare())
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        sys.exit(f"Fontin setup failed: {error}")
