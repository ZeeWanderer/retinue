from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class SccacheStartupTests(unittest.TestCase):
    def test_concurrent_builds_serialize_server_initialization(self):
        with tempfile.TemporaryDirectory(prefix="retinue-cache-test-") as temp:
            root = Path(temp)
            shutil.copy(ROOT / "Makefile", root)
            shutil.copy(ROOT / "VERSION", root)
            cache = root / "cache with spaces"
            mock = root / "mock-sccache"
            mock.write_text("""#!/usr/bin/env python3
import os
from pathlib import Path
import sys
import time

assert sys.argv[1:] == ["--dist-status"]
cache = Path(os.environ["SCCACHE_DIR"])
guard = cache / "initializing"
guard.mkdir()
try:
    time.sleep(0.05)
    with (cache / "calls").open("a") as output:
        output.write("ready\\n")
finally:
    guard.rmdir()
""")
            mock.chmod(0o755)
            command = [
                "make", "--no-print-directory", "-s", "sccache-setup",
                f"SCCACHE={mock}", f"SCCACHE_DIR={cache}",
                f"SCCACHE_SERVER_UDS={cache / 'server.sock'}",
            ]
            processes = [
                subprocess.Popen(command, cwd=root, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, text=True)
                for _ in range(3)
            ]
            try:
                for process in processes:
                    output, _ = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, 0, output)
            finally:
                for process in processes:
                    if process.poll() is None:
                        process.kill()
                    process.wait()
            self.assertEqual((cache / "calls").read_text(), "ready\n" * 3)

    def test_optional_cache_can_be_disabled(self):
        result = subprocess.run(
            ["make", "--no-print-directory", "-s", "sccache-setup", "SCCACHE="],
            cwd=ROOT, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
