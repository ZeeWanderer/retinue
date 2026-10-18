# Native Ghidra Exporter

Generic, build-keyed native analysis for `retinue-inspect`. Install Ghidra and its
required JDK. `run` finds `GHIDRA_HOME`, `analyzeHeadless` on PATH, or Homebrew's
Ghidra installation; it forwards headless arguments and adds this script directory.

## Import

Keep a separate project per executable build. Import without whole-program
analysis, then prepare known function entries as needed:

```bash
mkdir -p research/native/projects
tools/ghidra/run research/native/projects BUILD \
  -import /path/to/executable -noanalysis

tools/ghidra/run research/native/projects BUILD \
  -process PROGRAM -noanalysis \
  -postScript PrepareFunctions.java 0x140001000 0x140002000
```

Preparation disassembles selected entries and repairs placeholder function
bodies. It changes only the analysis project, never the executable or running
process. It rejects entries inside other functions or outside executable
memory. Omit `-noanalysis` for normal Ghidra analysis; use
`-analysisTimeoutPerFile SECONDS` and `-max-cpu N` to bound that work.

## Type Annotations

Apply verified types when guessed signatures obscure decompiled arithmetic:

```json
{
  "sha256": "EXECUTABLE_SHA256",
  "functions": [
    {"address": "0x140001000", "signature": "float scalar(float value)"}
  ]
}
```

```bash
tools/ghidra/run PROJECT_DIR PROJECT_NAME \
  -process PROGRAM -noanalysis -postScript ApplyFunctionTypes.java types.json
```

Supply complete signatures, including parameters. The executable hash must
match, functions must already exist, and invalid input
rolls back the batch. These annotations change the analysis project, not the binary;
keep their instruction evidence alongside the input file.

## Export

```bash
tools/ghidra/run PROJECT_DIR PROJECT_NAME \
  -process PROGRAM -noanalysis -readOnly \
  -postScript ExportWfinspectReport.java REPORT.json \
  function=0x140001000 graph=0x140001000:2:64 \
  xrefs=0x140002000 'string=SyncInventoryFromDB'

wfinspect game report verify REPORT.json
wfinspect game report list REPORT.json
wfinspect game report get REPORT.json 0
```

Queries:

- `function=ADDRESS`: disassembly and decompilation of the containing function.
- `graph=ADDRESS:DEPTH:MAX_FUNCTIONS`: direct calls and tail jumps, with unresolved
  indirect calls/jumps and an unvisited frontier. Depth 0..16, at most 1,024 functions
  and 100,000 instructions. Operates on defined functions; does not guess virtual
  dispatch targets or define functions automatically.
- `xrefs=ADDRESS`: analyzed references to an address.
- `lea=ADDRESS`: x86-64 RIP-relative LEA candidates in executable blocks, including
  unanalyzed code. `preceding_function` is a navigation hint, not proven containment.
  Validate candidate instruction boundaries before using them as evidence.
- `branches=ADDRESS`: x86-64 rel32 CALL/JMP candidates, including unanalyzed code;
  the same boundary checks apply. Does not resolve indirect branches.
- `string=TEXT`: exact UTF-8 occurrences and analyzed references.
- `bytes=HEX`: exact byte occurrences, useful for pointer/atom records even
  where Ghidra has not created references yet. Executable matches include
  instruction-boundary and preceding-function hints.
- `scalar=VALUE`: instructions containing an exact scalar.
- `pointers=START:COUNT:WIDTH`: 4-byte image-relative or 8-byte absolute pointer table.

Reports retain the `wfinspect.ghidra-report` schema and producer identifiers so
existing evidence stays readable. They contain executable SHA-256. Pass
`--adapter BUILD` to require a specific registered adapter or executable hash.

`-readOnly` discards project changes made during export. Static graphs are not
complete runtime call graphs: unresolved virtual calls, undefined functions and
reported limits need further investigation. `incomplete_functions` and each
node's `unexplored_flow` flag incomplete analyzed bodies; prepare those first.
Ghidra can log a script failure without a failing process exit code. Use a fresh
output path and verify the report before consuming it.

## Test

The x86-64 integration test needs a C compiler (`CC` or `cc`) and `nm`. It
compiles a small synthetic program and exercises the real
headless exporter, including indirect calls, recursion and graph limits:

```bash
python3 tools/ghidra/tests/test_report.py
```
