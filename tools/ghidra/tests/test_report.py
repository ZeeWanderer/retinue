import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]


class ReportTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="wfinspect-ghidra-")
        cls.addClassCleanup(cls.directory.cleanup)
        root = Path(cls.directory.name)
        executable = root / "fixture"
        subprocess.run(
            shlex.split(os.environ.get("CC", "cc"))
            + ["-O0", "-g", "-fno-pie", "-no-pie", str(TOOLS / "tests/fixture.c"),
               "-o", str(executable)], check=True,
        )
        cls.sha256 = hashlib.sha256(executable.read_bytes()).hexdigest()
        cls.symbols = {}
        for line in subprocess.check_output(["nm", "-n", str(executable)], text=True).splitlines():
            fields = line.split()
            if len(fields) == 3:
                cls.symbols[fields[2]] = int(fields[0], 16)
        address = lambda name: hex(cls.symbols[name])
        report = root / "report.json"
        partial = root / "partial.json"
        typed = root / "typed.json"
        rejected = root / "rejected.json"
        annotations = root / "types.json"
        stale = root / "stale-types.json"
        invalid = root / "invalid-types.json"
        annotations.write_text(json.dumps({"sha256": cls.sha256, "functions": [
            {"address": address("leaf"), "signature": "unsigned int leaf(unsigned int value)"},
            {"address": address("wrapper"), "signature": "unsigned int wrapper(int value)"},
        ]}))
        stale.write_text(json.dumps({"sha256": "0" * 64, "functions": [
            {"address": address("leaf"), "signature": "double leaf(unsigned int value)"},
        ]}))
        invalid.write_text(json.dumps({"sha256": cls.sha256, "functions": [
            {"address": address("leaf"), "signature": "double leaf(unsigned int value)"},
            {"address": "0x1", "signature": "int missing(void)"},
        ]}))
        result = subprocess.run([
            str(TOOLS / "run"), str(root), "fixture", "-import", str(executable),
            "-analysisTimeoutPerFile", "60", "-max-cpu", "2",
            "-postScript", "tests/SeedPartialFunction.java", address("main"),
            "-postScript", "ExportWfinspectReport.java", str(partial),
            "graph=" + address("main") + ":3:20",
            "-postScript", "PrepareFunctions.java", address("main"), address("leaf"),
            "-postScript", "ExportWfinspectReport.java", str(report),
            "function=" + address("main"),
            "graph=" + address("main") + ":3:20",
            "graph=" + address("main") + ":0:20",
            "graph=" + address("main") + ":3:1",
            "string=wfinspect native graph fixture",
            "scalar=0x12345678",
            "xrefs=" + address("leaf"),
            "pointers=" + address("table") + ":2:8",
            "bytes=" + cls.symbols["leaf"].to_bytes(8, "little").hex(),
            "lea=" + address("marker"),
            "branches=" + address("leaf"),
            "branches=0x7fffffffffff",
            "bytes=78563412",
            "-postScript", "ApplyFunctionTypes.java", str(annotations),
            "-postScript", "ExportWfinspectReport.java", str(typed),
            "function=" + address("leaf"), "function=" + address("wrapper"),
            "-postScript", "ApplyFunctionTypes.java", str(stale),
            "-postScript", "ApplyFunctionTypes.java", str(invalid),
            "-postScript", "ExportWfinspectReport.java", str(rejected),
            "function=" + address("leaf"), "function=" + address("wrapper"),
        ], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
        if result.returncode != 0 or not report.exists():
            raise AssertionError(result.stdout)
        cls.report = json.loads(report.read_text())
        cls.partial = json.loads(partial.read_text())
        cls.typed = json.loads(typed.read_text())
        cls.rejected = json.loads(rejected.read_text())

    def test_identity_and_function(self):
        self.assertEqual(self.report["program"]["sha256"].lower(), self.sha256)
        function = self.report["queries"][0]
        self.assertTrue(function["found"])
        self.assertTrue(function["instructions"])
        self.assertIn("decompilation", function)
        self.assertFalse(function["instructions_truncated"])

    def test_graph_preserves_indirect_calls_and_cycles(self):
        graph = self.report["queries"][1]
        entries = [int(node["entry"], 16) for node in graph["nodes"]]
        self.assertEqual(len(entries), len(set(entries)))
        for name in ("main", "wrapper", "leaf", "invoke", "invoke_jump", "recursive"):
            self.assertIn(self.symbols[name], entries)
        self.assertTrue(any(edge["kind"] == "indirect_call" for edge in graph["edges"]))
        self.assertTrue(any(edge["kind"] == "indirect_jump" for edge in graph["edges"]))
        self.assertGreater(graph["unresolved_edges"], 0)
        self.assertFalse(graph["truncated"])
        self.assertEqual(graph["incomplete_functions"], 0)
        recursive = self.symbols["recursive"]
        self.assertTrue(any(int(edge["from"], 16) == recursive
                            and int(edge.get("target", "0"), 16) == recursive
                            for edge in graph["edges"]))

    def test_partial_function_is_detected_and_repaired(self):
        graph = self.partial["queries"][0]
        self.assertEqual(graph["incomplete_functions"], 1)
        self.assertTrue(graph["nodes"][0]["unexplored_flow"])
        self.assertEqual(self.report["queries"][1]["incomplete_functions"], 0)

    def test_limits_report_frontier(self):
        for query, reason in ((2, "depth_limit"), (3, "function_limit")):
            graph = self.report["queries"][query]
            self.assertEqual(len(graph["nodes"]), 1)
            self.assertTrue(graph["truncated"])
            self.assertTrue(graph["frontier"])
            self.assertTrue(all(item["reason"] == reason for item in graph["frontier"]))

    def test_references_scalars_and_pointer_table(self):
        queries = self.report["queries"]
        self.assertTrue(queries[4]["matches"])
        self.assertTrue(queries[5]["matches"])
        self.assertTrue(queries[6]["references"])
        targets = [int(entry["target"], 16) for entry in queries[7]["entries"]]
        self.assertEqual(targets, [self.symbols["leaf"], self.symbols["wrapper"]])
        matches = [int(match["address"], 16) for match in queries[8]["matches"]]
        self.assertIn(self.symbols["table"], matches)
        self.assertTrue(queries[9]["candidates"])
        self.assertTrue(all(match["analyzed_boundary"] for match in queries[9]["candidates"]))

    def test_relative_branches_find_calls_and_jumps(self):
        query = self.report["queries"][10]
        self.assertFalse(query["truncated"])
        self.assertFalse(query["skipped_blocks"])
        self.assertEqual({match["operation"] for match in query["candidates"]}, {"call", "jump"})
        jump = next(match for match in query["candidates"] if match["operation"] == "jump")
        self.assertEqual(int(jump["address"], 16), self.symbols["tail_jump"])
        self.assertTrue(jump["analyzed_boundary"])
        self.assertFalse(self.report["queries"][11]["candidates"])

    def test_byte_matches_include_code_navigation_hints(self):
        matches = self.report["queries"][12]["matches"]
        leaf_matches = [match for match in matches
                        if int(match.get("preceding_function", "0"), 16) == self.symbols["leaf"]]
        self.assertTrue(leaf_matches)
        self.assertTrue(all(not match["analyzed_boundary"] for match in leaf_matches))

    def test_function_annotations_include_parameters(self):
        leaf, wrapper = self.typed["queries"]
        self.assertIn("uint leaf(uint value)", leaf["decompilation"])
        self.assertIn("uint wrapper(int value)", wrapper["decompilation"])

    def test_stale_and_partial_annotations_do_not_change_types(self):
        self.assertEqual(self.typed["queries"], self.rejected["queries"])


if __name__ == "__main__":
    unittest.main()
