#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]


class CacheHelpersTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.xdg = self.root / "cache with spaces"
        self.env = dict(os.environ, HOME=str(self.home), XDG_CACHE_HOME=str(self.xdg))
        for base, source in [(self.xdg, "xdg"), (self.home / ".cache", "home")]:
            cache = base / "retinue/games/warframe"
            cache.mkdir(parents=True)
            (cache / "ExportTest.json").write_text(json.dumps([{"name": source}]))
            (cache / "languages.json").write_text(json.dumps({"test_key": source}))
            (cache / "worldstate.json").write_text(json.dumps({"Origin": source}))
            (cache / "ignore.bin").write_text(source)

    def run_helper(self, script, *args, env=None):
        interpreter = "bash" if script.endswith(".sh") else sys.executable
        return subprocess.run(
            [interpreter, str(SCRIPTS / script), *args], cwd=self.root,
            env=env or self.env, capture_output=True, text=True, check=True,
        ).stdout.strip()

    def test_xdg_defaults_outside_checkout(self):
        for value, source in [(str(self.xdg), "xdg"), ("", "home"), ("relative", "home")]:
            with self.subTest(xdg=value):
                env = dict(self.env, XDG_CACHE_HOME=value)
                self.assertEqual(self.run_helper("export_keys.py", "ExportTest.json", env=env), source)
                self.assertEqual(self.run_helper("lang_lookup.py", "test_key", env=env), source)
                self.assertEqual(
                    self.run_helper("worldstate_inspect.py", "--values", source, env=env),
                    f"Origin = {source}",
                )
                matches = self.run_helper("priv_search.sh", source, "--files-with-matches", env=env)
                self.assertEqual(len(matches.splitlines()), 3)
                self.assertTrue(all(path.endswith(".json") for path in matches.splitlines()))

    def test_explicit_files(self):
        export = self.root / "ExportTest.json"
        export.write_text('[{"name": "explicit"}]')
        for path in [export.name, str(export)]:
            self.assertEqual(self.run_helper("export_keys.py", path), "explicit")
        directory = self.root / "other"
        directory.mkdir()
        data = directory / "data.json"
        data.write_text('{"test_key": "explicit"}')
        for path in [str(data.relative_to(self.root)), str(data)]:
            self.assertEqual(self.run_helper("lang_lookup.py", "test_key", "--file", path), "explicit")
            self.assertEqual(
                self.run_helper("worldstate_inspect.py", path, "--values", "explicit"),
                "test_key = explicit",
            )

    def test_missing_files_are_errors(self):
        for script, args in [
            ("export_keys.py", ["missing.json"]),
            ("lang_lookup.py", ["test_key", "--file", "missing.json"]),
            ("worldstate_inspect.py", ["missing.json", "--top-keys"]),
        ]:
            with self.subTest(script=script), self.assertRaises(subprocess.CalledProcessError) as error:
                self.run_helper(script, *args)
            self.assertIn("not found", error.exception.stderr)


if __name__ == "__main__":
    unittest.main()
