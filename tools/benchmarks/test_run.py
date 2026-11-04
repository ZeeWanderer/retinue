import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from reports import ROOT, digest, fixture_digest, write_json
from run import balanced_order, catalog, check_selector, clean_environment, executable, measured_config, open_report


class RunnerTests(unittest.TestCase):
    def test_make_snapshot_preserves_compiler_and_cache_environment(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            runner = root / "tools/benchmarks/run.py"
            runner.parent.mkdir(parents=True)
            runner.write_text('import json, os, sys\nprint(json.dumps({"args": sys.argv[1:], "env": dict(os.environ)}))\n')
            (root / "VERSION").write_text("0.0.0\n")
            cache = str(root / "sccache")
            env = {k: v for k, v in os.environ.items() if k not in (
                "RUSTC_WRAPPER", "CMAKE_C_COMPILER_LAUNCHER", "CMAKE_CXX_COMPILER_LAUNCHER", "MAKEFLAGS", "MFLAGS")}
            result = subprocess.check_output([
                "make", "--no-print-directory", "-s", "-f", str(ROOT / "Makefile"), "bench-prepare",
                "BENCH_SUITE=gui-relics", "BENCH_OUTPUT=out", "LLVM_ROOT=",
                "CC=/test/clang", "CXX=/test/clang++", f"SCCACHE={cache}",
                f"SCCACHE_DIR={root / 'cache'}", f"SCCACHE_SERVER_UDS={root / 'cache.sock'}",
            ], cwd=root, env=env, text=True)
            data = json.loads(result)
            self.assertEqual(data["args"], ["prepare", "gui-relics", "--output", "out"])
            for key in ("RUSTC_WRAPPER", "CMAKE_C_COMPILER_LAUNCHER", "CMAKE_CXX_COMPILER_LAUNCHER"):
                self.assertEqual(data["env"][key], cache)
            self.assertEqual(data["env"]["CC"], "/test/clang")
            self.assertEqual(data["env"]["CXX"], "/test/clang++")

    def test_balanced_order(self):
        self.assertEqual(balanced_order(4), [0, 1, 1, 0, 0, 1, 1, 0])

    def test_ambient_experiment_knobs_do_not_leak(self):
        with patch.dict(os.environ, {"GUI_BENCH_LIVE": "1", "RETINUE_BENCH_NO_LAYERS": "1",
                                     "PATH": "keep"}):
            env = clean_environment()
            self.assertEqual(env["PATH"], "keep")
            self.assertNotIn("GUI_BENCH_LIVE", env)
            self.assertNotIn("RETINUE_BENCH_NO_LAYERS", env)

    def test_cargo_artifact_selection_not_filename_guessing(self):
        row = {"reason": "compiler-artifact", "profile": {"test": True}, "executable": "/test-bin"}
        self.assertEqual(executable([row, row]), Path("/test-bin"))
        for rows in ([], [row, {**row, "executable": "/different"}]):
            with self.assertRaises(ValueError):
                executable(rows)

    def test_exact_test_selector_and_no_silent_zero_tests(self):
        with patch("run.subprocess.check_output", return_value="module::case: test\nmodule::case_extra: test\n"):
            check_selector(Path("/binary"), "module::case", {})
            with self.assertRaises(ValueError):
                check_selector(Path("/binary"), "case", {})

    def test_invalid_configuration_and_unsupported_knobs(self):
        args = argparse.Namespace(threads=4, samples=120, repeats=7, hz=0, cases="", timeout=600)
        self.assertEqual(measured_config(args, {"adapter": "gui"})["threads"], 4)
        for field, value in (("threads", 0), ("samples", 1), ("repeats", 0), ("timeout", 0), ("hz", 60), ("cases", "hover")):
            bad = argparse.Namespace(**vars(args))
            setattr(bad, field, value)
            with self.assertRaises(ValueError):
                measured_config(bad, {"adapter": "gui"})

    def test_snapshot_is_verified_and_output_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "benchmark").write_bytes(b"binary")
            snapshot = {"schema": 1, "suite": "example", "spec": {"workload": 1, "adapter": "gui", "test": "module::bench"},
                        "fixture_sha256": "fixture", "build": {},
                        "provenance": {"binary_sha256": digest(root / "benchmark")}}
            write_json(root / "snapshot.json", snapshot)
            with patch("run.machine", return_value={}), patch("run.check_selector"):
                job = open_report(root, root / "results", {}, 7)
                self.assertEqual(job["report"]["state"], "recording")
                self.assertEqual(len(job["report"]["id"]), 32)
                self.assertTrue(job["report"]["created_at"].endswith("+00:00"))
                with self.assertRaises(FileExistsError):
                    open_report(root, root / "results", {}, 7)
                (root / "benchmark").write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    open_report(root, root / "new", {}, 7)

    def test_overlay_case_filters_cannot_silently_drop_unknown_cases(self):
        args = argparse.Namespace(threads=4, samples=120, repeats=7, hz=60, cases="hover", timeout=600)
        spec = {"adapter": "overlay", "cases": ["hover", "scroll"]}
        self.assertEqual(measured_config(args, spec)["cases"], "hover")
        for cases in ("hover,typo", "hover,hover", "unknown"):
            args.cases = cases
            with self.assertRaises(ValueError):
                measured_config(args, spec)


class CatalogTests(unittest.TestCase):
    def test_every_ignored_native_test_is_classified(self):
        entries = list(catalog()["suite"].values()) + catalog()["manual"]
        expected = {(row["source"], row["test"].split("::")[-1]) for row in entries}
        self.assertEqual(len(entries), len(expected), "duplicate catalog entry")
        actual = set()
        for base in ("apps", "crates"):
            paths = [path for folder in ("src", "tests")
                     for path in (ROOT / base).glob(f"*/{folder}/**/*.rs")]
            for path in paths:
                source = path.read_text()
                for name in re.findall(r'#\[ignore[^\]]*\]\s*(?:async\s+)?fn\s+(\w+)', source):
                    actual.add((str(path.relative_to(ROOT)), name))
        self.assertEqual(expected, actual, "classify new ignored tests; do not silently skip them")

    def test_maintained_fixtures_exist_and_are_fingerprinted(self):
        for name, suite in catalog()["suite"].items():
            with self.subTest(suite=name):
                self.assertEqual(len(fixture_digest(suite["fixtures"])), 64)
                self.assertGreater(suite["workload"], 0)

    def test_missing_fixture_pattern_is_an_error(self):
        with self.assertRaises(ValueError):
            fixture_digest(["no-such-fixture/*.json"])


if __name__ == "__main__":
    unittest.main()
