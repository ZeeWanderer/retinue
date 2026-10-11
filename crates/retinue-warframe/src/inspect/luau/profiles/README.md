# Script Profiles

Profiles describe offline bytecode only. They never enable native memory
adapters. `reviewed` means the listed assignments were reviewed, not that every
script or opcode is supported. Unknown layouts/opcodes remain explicit errors.

| Profile | Evidence |
| --- | --- |
| `d01b5cb5cff5` | Original VM dispatch audit, expanded with four operations during the September 2026 Archimedea comparison. |
| `45fa6ad0769c` | Matching old/current bodies plus native handler checks; 72 assignments. No uniform opcode rotation explains the change. |

Canonical IDs follow the [Luau bytecode definition](https://github.com/luau-lang/luau/blob/c0e346edd89066b44dca174c9f54ce84c746a540/Common/include/Luau/Bytecode.h)
used by the pinned decompiler, not heuristic opcode detection. AUX words are
not instructions. Only the documented prediction byte on GETGLOBAL, SETGLOBAL,
GETTABLEKS, SETTABLEKS and NAMECALL is ignored by structural comparison.
Inference also checks the [compiler's vararg prologue](https://github.com/luau-lang/luau/blob/c0e346edd89066b44dca174c9f54ce84c746a540/Compiler/src/Compiler.cpp):
PREPVARARGS uses the function's parameter count at entry. This remains candidate
evidence, subject to map conflicts and native review.

Current `random` and `randomseed` labels refer to native bindings `0x19517a64`
and `0x13aa4421`, verified at `0x141924950` and `0x141924bd0` in that exact
executable. Names affect source presentation only; comparison retains atom IDs.

Research artifacts are local, under
`research/warframe/build-stats-45fa6ad0769c/`: `opcode-candidates.json`,
`luau-vm.asm`, `archimedea-current-bindings.json`, and `archimedea-audit/README.md`.
Do not copy addresses or profiles to another build without validation.
