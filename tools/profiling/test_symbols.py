import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import build
import symbols


class BuildSettingsTests(unittest.TestCase):
    def test_flavors_preserve_optimization_and_assertion_defaults(self):
        for flavor, profile in (("dev", "dev"), ("prod", "release")):
            args = build.command("poe2companion", flavor)
            self.assertIn(profile, args)
            self.assertIn(f"profile.{profile}.debug=2", args)
            self.assertIn(f'profile.{profile}.strip="none"', args)
            self.assertNotIn("--features", args)
            self.assertFalse(any("opt-level" in arg or "debug-assertions" in arg for arg in args))

    def test_symbols_use_isolated_output_and_preserve_native_flags(self):
        with patch.dict(os.environ, {"CXXFLAGS": "-fPIC", "RUSTFLAGS": "-Copt-level=2"}, clear=True):
            env = build.environment("dev", Path("/isolated"))
        self.assertEqual(env["CARGO_TARGET_DIR"], "/isolated")
        self.assertEqual(env["CXXFLAGS"], "-fPIC -g")
        self.assertEqual(env["CFLAGS"], "-g")
        self.assertEqual(env["WFCLI_CPU_BASELINE"], "")
        self.assertIn("-Copt-level=2", env["CARGO_ENCODED_RUSTFLAGS"])
        self.assertNotIn("RUSTFLAGS", env)

    def test_encoded_flags_take_precedence_and_default_production_baseline_matches(self):
        with patch.dict(os.environ, {"CARGO_ENCODED_RUSTFLAGS": "-Clink-arg=path with spaces",
                                     "RUSTFLAGS": "ignored"}, clear=True), \
                patch("build.platform.machine", return_value="x86_64"):
            env = build.environment("prod", Path("/isolated"))
        flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
        self.assertEqual(flags[0], "-Clink-arg=path with spaces")
        self.assertNotIn("ignored", flags)
        self.assertIn("-Ctarget-cpu=x86-64-v2", flags)
        self.assertEqual(env["WFCLI_CPU_BASELINE"], "x86-64-v2")

    def test_explicit_baseline_is_respected(self):
        with patch.dict(os.environ, {"WFCLI_CPU_BASELINE": "native"}, clear=True):
            env = build.environment("prod", Path("/isolated"))
        self.assertEqual(env["WFCLI_CPU_BASELINE"], "native")

    def test_existing_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileExistsError), patch("build.subprocess.run") as run:
                build.build("wfgui", "prod", Path(directory), Path(directory) / "cargo")
            run.assert_not_called()


@unittest.skipUnless(platform.system() == "Linux" and shutil.which("cc"), "requires Linux and a C compiler")
class SymbolBundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "input app"
        self.destination = self.root / "symbol bundle"
        self.destination.mkdir()
        self.objcopy = symbols.tool("OBJCOPY", "llvm-objcopy", "objcopy")
        self.readelf = symbols.tool("READELF", "llvm-readelf", "readelf")

    def compile(self, *flags):
        subprocess.run(["cc", "-x", "c", "-O2", *flags, "-o", str(self.binary), "-"],
                       input="int main(void) { return 0; }\n", text=True, check=True,
                       capture_output=True)

    def text_section(self, binary, name):
        path = self.root / name
        subprocess.run([self.objcopy, f"--dump-section=.text={path}", str(binary)],
                       check=True, capture_output=True)
        return path.read_bytes()

    def test_separation_preserves_code_identity_and_input(self):
        self.compile("-g", "-Wl,--build-id")
        before = symbols.digest(self.binary)
        manifest = symbols.package(self.binary, self.destination)
        symbols.save_manifest(self.destination, manifest)
        binary = self.destination / manifest["executable"]["path"]
        debug = self.destination / manifest["debug"]["path"]
        self.assertEqual(before, symbols.digest(self.binary))
        self.assertEqual(before, manifest["source_sha256"])
        self.assertEqual(self.text_section(self.binary, "original-text"), self.text_section(binary, "split-text"))
        self.assertEqual(symbols.build_id(self.readelf, binary), symbols.build_id(self.readelf, debug))
        identity = manifest["build_id"]
        self.assertEqual((self.destination / ".build-id" / identity[:2] / f"{identity[2:]}.debug").resolve(), debug)
        sections = symbols.output([self.readelf, "-SW", str(binary)])
        self.assertIn(".gnu_debuglink", sections)
        self.assertNotIn(".debug_info", sections)
        self.assertIn(".debug_info", symbols.output([self.readelf, "-SW", str(debug)]))
        self.assertEqual(json.loads((self.destination / "symbols.json").read_text()), manifest)
        subprocess.run([str(binary)], check=True)

    def test_stripped_input_is_rejected(self):
        self.compile("-Wl,--build-id", "-s")
        with self.assertRaisesRegex(ValueError, "no debug information"):
            symbols.package(self.binary, self.destination)
        self.assertFalse((self.destination / "symbols.json").exists())

    def test_missing_build_id_is_rejected(self):
        self.compile("-g", "-Wl,--build-id=none")
        with self.assertRaisesRegex(ValueError, "missing ELF build ID"):
            symbols.package(self.binary, self.destination)

    def test_non_elf_is_rejected(self):
        self.binary.write_bytes(b"not executable")
        with self.assertRaisesRegex(ValueError, "expected an ELF"):
            symbols.package(self.binary, self.destination)

    def test_cli_refuses_output_collision(self):
        self.compile("-g", "-Wl,--build-id")
        marker = self.destination / "keep"
        marker.write_text("unchanged")
        result = subprocess.run(["python3", str(Path(symbols.__file__)), str(self.binary),
                                 "--output", str(self.destination)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(marker.read_text(), "unchanged")


if __name__ == "__main__":
    unittest.main()
