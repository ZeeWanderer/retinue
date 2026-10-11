# Retinue Inspector

Read-only process inspection and Warframe research using live memory, saved captures,
cache resources, and runtime events. Full builds stage it alongside the companions; repository links are
`retinue-inspectd` (development) and `retinue-inspect` (production). `wfinspectd` and `wfinspect`
remain aliases; examples below use the short name.

Build only the inspector, without the overlay renderer:

```bash
make inspect
_build/cargo/debug/retinue-inspect memory maps --pid 12345
_build/cargo/debug/retinue-inspect memory read 0x123000 64 --pid 12345
_build/cargo/debug/retinue-inspect memory read 0x123000 64 --capture research/capture
```

Generic `memory` commands require `--pid` or `--capture`. `native report` reads reports for
any executable hash. Warframe discovery and adapters live under `warframe` (`game` alias).

## Start Here

```bash
wfinspect doctor
wfinspect game adapter --list
wfinspect game adapter --exe /path/to/Warframe.x64.exe
wfinspect game inventory
wfinspect game memory --help
mkdir -p ~/.local/share/bash-completion/completions
wfinspect completion bash > ~/.local/share/bash-completion/completions/wfinspect
```

Every command has `--help`. Addresses accept decimal or `0x` hexadecimal.
Use `--` before literal arguments beginning with a dash or named `help`.

`game inventory` reads current resource stacks, blueprints and foundry jobs,
including their inventory sync ID, directly from the running game.

`game adapter --exe PATH` discovers HTTP, inventory, account, metadata and Scaleform bindings
without launching Warframe. The live probe checks each domain separately.
`--list` lists historical layouts retained for old capture replay.

## Native Weapon Equipment

Inspect an explicit native weapon instance from the current process or a saved
capture. This reads the weapon's attached upgrades, not cached account inventory.

```bash
wfinspect game equipment --weapon 0x123400 --pid 12345
wfinspect game equipment --weapon 0x123400 --capture research/equipment
```

Replace the example address with a weapon instance verified in that session.
Omitting `--pid` discovers the running game, not the local player's weapon.
Addresses become invalid when objects are replaced or the game restarts.

Output includes native asset paths, object/holder addresses and discovered
bindings. Attached upgrades can include holster customizations and generated
SubMods; they are not all installed cards. `raw_rank` is unavailable: reference
counters are not mod ranks.

The reader discovers virtual slots from the named native caller and decodes the
selected weapon's accessors. It validates weapon ancestry, executable identity,
code, pointers and vector bounds, with at most 128 attachments and 32 ancestry
links. Identity, vector contents and header are reread; this is not atomic.
Changed or unsupported layouts fail without guessed offsets.

Capture replay requires the matching executable at its recorded path and all
referenced object, vtable, accessor-code, descriptor and string pages. A root-only
memory capture may omit these; missing evidence produces an error, not an empty
equipment list.

## Memory and UI

Memory queries discover the running Warframe process by default. Add
`--capture DIRECTORY` to query saved evidence with the same reader.

```bash
wfinspect game memory maps
wfinspect game memory find 'LITH ERA' --scope readable
wfinspect game memory refs 0x123400 --length 128
wfinspect game memory path 0x123000 0x123400 --depth 24
wfinspect game memory capture research/capture --root 0x123000 --depth 6
wfinspect game memory capture research/range --range 0x123000:4096
wfinspect game memory read 0x123000 64 --capture research/range > bytes.bin
```

`--scope` selects research mappings, all readable mappings, heap, or executable
image. `--range START:LENGTH` narrows that selection and can be repeated.
Searches and walks report coverage, unreadable blocks, and exhausted limits.
Captured ranges describe available evidence, not the entire original process.
Captures are bounded, read while the game runs, and never overwrite an existing
capture directory.

```bash
wfinspect game ui movies
wfinspect game ui objects ProjectionReward
wfinspect game ui capture research/reward
wfinspect game ui objects ProjectionReward --capture research/reward
wfinspect game ui relic --capture research/reward
```

`ui objects` exposes known text-object addresses, instance names and values for
a movie-path substring. Typed queries require validated executable bindings.
UI captures retain those bindings and the typed reader's first-seen bytes for replay
after game updates. Each movie reports text completeness. Reads are bounded, not atomic.
Supplying search terms selects a broader heap/text graph capture; this is also the
fallback when typed discovery fails. Graph captures can omit deeper objects.
`ui movies` recomputes discovery from available bytes; `ui replay` also includes
the recorded acquisition snapshot for comparison.

## Memory Recording

```bash
wfinspect game memory watch research/fields.jsonl --range 0x123000:64 --seconds 60
wfinspect game memory replay research/fields.jsonl
wfinspect game memory replay research/fields.jsonl --sample 20
```

Watch explicit ranges without rescanning the process. Files contain timestamped
byte changes, read errors and a final summary. Replay selects the last retained
sample at or before the requested poll index. Reads across ranges are not atomic;
changes between polls can be missed.

## Combat Recording

```bash
wfinspect game combat probe
wfinspect game combat watch research/combat.jsonl --seconds 180
wfinspect game combat replay research/combat.jsonl
wfinspect game combat replay research/combat.jsonl --events
```

Combat recording requires a supported executable, an active mission HUD and
modern damage numbers. Enter a mission or Simulacrum before starting it.
Discovery runs once; `--owner ADDRESS` bypasses scanning, while `--scope`,
`--range` and `--max-bytes` constrain it. Wait for `READY` before the experiment.
Raw display records preserve amounts, critical tiers and glyph tokens. Replay
uses the same decoder and reports detected gaps, revisions and unsettled records.
These are HUD observations, not lossless damage packets or verified target IDs.

Both recorders default to 50 ms sampling and a 64 MiB file limit. Adjust with
`--interval-ms` and `--record-bytes`; duration is capped at 30 minutes. Files are
private and never overwritten. Ctrl+C leaves retained records readable, but an
interrupted recording has no final summary and replay reports it as incomplete.

## Resources and Scripts

```bash
wfinspect game cache locate
wfinspect game cache packages
wfinspect game cache paths CACHE_DIR Font Conquest
wfinspect game cache extract CACHE_DIR Font /Lotus/Scripts/Libs/ConquestLib.lua research/ConquestLib
wfinspect game cache find CACHE_DIR Font 'Conquest' --split B --stream
wfinspect game cache read CACHE_DIR Font /Lotus/Scripts/Libs/ConquestLib.lua B \
  | wfinspect game script info - --adapter BUILD
```

Cache matches include resource split and byte offset. `--path` filters resource
names; `--continue-on-error` retains later results after decoding failures.
Failures remain in the report and produce a nonzero exit status.
Installation discovery reads Steam library metadata; `WFINSPECT_CACHE_DIR`
overrides the discovered cache directory.

Oodle extraction uses the bundled, pinned `unoodle` helper in `libexec/`.
Builds download its checksum-verified source and compile it alongside the other
tools; extraction needs no runtime download or Cargo installation.

```bash
wfinspect game cache decoder
```

`WFINSPECT_OODLE_LIBRARY` selects an official Linux Oodle library;
`WFINSPECT_OODLE_COMMAND` selects another helper. Explicit overrides take
precedence over the bundled decoder. Validate new game streams against an
official decoder before relying on their contents for semantic research.

Script `info` preserves raw instructions/constants and marks uncertain opcode
boundaries. `normalize`, `disassemble`, and `decompile` require complete opcode
mapping. Decompilation uses the bundled `wf-luau-decompiler` helper.

## Track Game Updates

Retain a baseline before updating the game, then take another snapshot afterward:

```bash
wfinspect game script snapshot research/scripts before --executable /path/to/Warframe.x64.exe
wfinspect game script snapshot research/scripts after --executable /path/to/Warframe.x64.exe
wfinspect game script infer research/scripts before after --output-profile candidate.json > inference.json
wfinspect game script compare research/scripts before after --profile reviewed.json > changes.json
```

Snapshots contain all `.lua` B splits, deduplicated by content hash. Names are
immutable; extraction failures are recorded and exit nonzero. Unknown bytecode
is retained for later analysis. `--package` and `--path` permit smaller captures.
Keep extracted resources in an ignored research workspace, outside Git.
Snapshot reads use the captured profile; `--profile` rechecks unchanged raw
bytes with a newer profile for the same executable.

Inference proposes mappings from unique matching bodies. It retains anchors,
ambiguities and conflicts; it does not certify native handler equivalence.
Review candidates before marking a profile `reviewed`. Comparison distinguishes
encoding changes, atom rebinding, metadata changes and changed code/constants.
Missing/undecodable dependencies and changed executable-native dependencies need
review. `--check` makes that result usable as a failing update check.
The example uses `reviewed.json` for the completed, verified candidate profile.

```bash
wfinspect game script snapshots research/scripts
wfinspect game script show research/scripts after --path Archimedea
wfinspect game script coverage research/scripts after --profile reviewed.json > coverage.json
wfinspect game script decompile --snapshot research/scripts after /Lotus/Scripts/Libs/ConquestLib.lua --profile reviewed.json
wfinspect game script features > features.json
wfinspect game script profiles BUILD > profile.json
```

`compare --features FILE` selects a feature manifest. `depends_on` links shared
mechanics; their review requirements propagate to dependents. Script dependencies
include possible literal module references, not a complete dynamic call graph.
Profiles can supply atom names in `symbols` (decimal hash keys); raw IDs remain
in `info` and comparison. `script import` accepts `{ "profile": {...}, "scripts":
{ "/resource.lua": "bytecode-file" } }`; file paths are relative to the manifest.
`coverage` indexes opcode/atom usage and incomplete prototypes, with example
resource paths. Opcode counts stop at unknown boundaries; AUX bytes are not
guessed to be instructions.

### Property And Native Evidence

Attach retained property and [Ghidra](../tools/wfinspect-ghidra/README.md) reports
to each snapshot with `wfinspect game script evidence WORKSPACE NAME bindings.json`:

```json
{
  "schema": 1,
  "executable_sha256": "<snapshot executable SHA-256>",
  "packages": "Packages.bin.raw",
  "properties": ["properties.json"],
  "native": {"upgrade.accumulator": {"report": "functions.json", "query": 0}}
}
```

Paths are relative to the bindings file. Property reports use version `46`,
`stats.aligned`, `packages_sha256`, `decoder_sha256`, `requested`, `duplicate_paths`
and `types` keyed by resource path. Each type retains `parent` and `ownText`, or
`missing: true`. Supply freshly extracted Packages data from the same game build.
Import checks its hash, native executable identity and extraction completeness;
retained evidence is immutable. Truncated native queries are rejected.

Feature `properties` entries select exact roots or captured paths under a trailing
`/` prefix. Comparison includes parent records and candidate `Script=` references.
New fields remain part of the fingerprint. Missing records, changed decoders and
dependency cycles cannot pass as unchanged. Prefixes cover captured records, not
the entire catalog; other resource references require explicit roots.

Native fingerprints retain instruction addresses and operands. Matching bodies
across executable updates still require review of globals, callees and dispatch.
Reports detect drift; they do not certify semantic equivalence or update formulas.
`changed` distinguishes observed drift from existing coverage gaps;
`review_required` includes both. Keep unresolved coverage in the feature's `data` list.

## Runtime Evidence

```bash
wfinspect game events watch --seconds 30
wfinspect game gep state --payload-dir research/payloads
wfinspect game gep watch --seconds 60 --payload-dir research/payloads
wfinspect daemon get player
wfinspect daemon subscribe player --seconds 30
```

DBWIN has one reader per Proton prefix; additional consumers subscribe to its
feed. Slow subscribers disconnect without stalling companion. Payload files
contain private account data; inspect them before sharing.

GEP watches sample independently of hashing, file output and stdout. Their summary
includes queue gaps and timing; a stalled consumer can lose samples. UI and memory
captures require a new directory and save owner-only files after acquisition.
`game gep state` includes account-seed availability and discovered bindings;
`game adapter --exe FILE` checks those bindings without a running game.

Snapshots emit JSON, watches emit timestamped NDJSON, and raw read/normalize
commands emit bytes only. Diagnostics go to stderr. Watches are bounded by
`--seconds` and `--limit`; daemon inspection does not start or mutate the daemon.

For offline executable analysis, the [Ghidra exporter](../tools/wfinspect-ghidra/README.md)
produces reports consumed by `game report verify`, `list`, and `get`.
