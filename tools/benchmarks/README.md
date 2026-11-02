# Benchmarks

`make bench-list` separates maintained headless regression workloads from
experiments and checks requiring a compositor, daemon, game or local assets.
Native implementations remain with their owners; this directory supplies the
catalog, process runner and report comparison. Python 3.11+ is required.

## Maintained Workloads

```bash
make bench BENCH_SUITE=gui-rails BENCH_THREADS=4 BENCH_RUNS=7 \
  BENCH_OUTPUT=research/retinue/benchmarks/rails-candidate
make bench BENCH_SUITE=poe2-overlay BENCH_THREADS=4 BENCH_SAMPLES=120 \
  BENCH_OUTPUT=research/retinue/benchmarks/poe2-candidate
```

Also available: `wf-overlay`, `gui-foundry`, `gui-relics`, `gui-drag`.
The runner builds one release test binary, verifies an exact ignored-test selector,
saves it with source/binary/toolchain/fixture provenance, then runs independent
processes. It does not stage or reload applications. Each run gets fresh XDG
cache/config/state directories; OS file cache is **not** flushed. Fixture-only
GUI traces do not represent a fully populated live artwork cache.

The output contains `snapshot/`, `measurements/report.json`, per-run native JSON
and process/build logs. Direct collection defaults to seven runs; fewer are useful
smoke checks, not enough for a performance verdict. GUI traces have fixed lengths;
`BENCH_SAMPLES` controls overlay warm frames only.

Raw reports live in ignored `research/`. Selected compact measurements can be
archived in [history/](history/README.md). Each session has an ID, UTC timestamps
and source provenance. A commit identifies the measured code; it need not have
its own run or become a baseline.
The rail scenario measures window chrome with an unpopulated Foundry, not a
populated grid reflow. Use the scrolling/drag scenarios for populated grids.

For stronger A/B evidence, prepare each revision separately, then interleave the
saved executables without compiling during measurement:

```bash
python3 tools/benchmarks/run.py prepare gui-rails --output research/bench/before
# After implementing the candidate:
python3 tools/benchmarks/run.py prepare gui-rails --output research/bench/after
python3 tools/benchmarks/run.py pair research/bench/before research/bench/after \
  --output research/bench/paired --repeats 8 --threads 4
python3 tools/benchmarks/compare.py research/bench/paired/before/report.json \
  research/bench/paired/after/report.json --policy research/bench/policy.toml \
  --output research/bench/verdict.json
```

Each round measures both builds; alternating order gives ABBA blocks. Use the
same build environment for both preparations. All-else-equal is still the
operator's responsibility: stop unrelated builds, keep power mode stable and
record whether the machine is busy. Load averages are evidence, not a noise filter.
Copied binaries are local snapshots, not portable packages; keep their dynamic
runtime libraries installed. Native dependency revisions are recorded, not
assumed equivalent.

Use a reviewed policy based on repeatability and a practical latency/resource
budget. [policy.example.toml](policy.example.toml) is syntax, **not an approved
baseline**. No policy or baseline is overwritten automatically. Rules may target
individual case IDs (literal match) or use a glob; exactly one rule must cover
each case. Each rule selects one primary lower-is-better metric. Other phase,
CPU and resource metrics remain available in the report; use another policy to
evaluate them. Absolute tolerances use the selected metric's unit, not always ms.

The comparison bootstraps independent per-process summaries, not individual
correlated frames. It uses a reproducible two-sided 95% interval on the difference
of means and `max(absolute, baseline * relative)` as practical tolerance:

| Verdict | Meaning | Exit |
| --- | --- | --- |
| `pass` | Entire interval stays within allowed regression | 0 |
| `regression` | Entire interval exceeds tolerance | 1 |
| `invalid` | Incomplete session, incompatible metadata or missing cases/metrics | 2 |
| `inconclusive` | Too few samples or interval crosses tolerance | 3 |

These are per-case intervals, not a simultaneous confidence guarantee across all
cases. Repeat suspected failures. Seven processes is a minimum, not proof of
stable tails. Do not use the p99 of a 60-frame run as a tail-regression gate.
There is no calibrated hard timing gate yet; shared CI runs the tooling tests and
deterministic work-budget tests, not workstation timing thresholds.

Update `suites.toml` when adding a maintained workload or ignored native check.
Bump its workload version when input sequence or timing boundaries change; fixture
files are fingerprinted automatically. Normalized reports are intentionally
strict; changing their meaning requires a schema version change.

## Selected History

```bash
python3 tools/benchmarks/history.py archive research/bench/paired/after/report.json \
  --note 'Reviewed scrollbar repair change'
python3 tools/benchmarks/history.py baseline MEASUREMENT_ID \
  --reason 'Accepted after repeatability and correctness checks'
python3 tools/benchmarks/history.py compare research/bench/candidate/report.json \
  --policy research/bench/policy.toml
```

Archive meaningful changes, release checkpoints and reproduced regressions, not
every commit. Records are immutable; baseline selection is separate and explicit.
Git retains the selection reason and previous measurement. Series match workload,
fixtures, build settings and machine, not source commit. A dirty run can be kept
as evidence but cannot be an accepted baseline. Raw frames, traces, binaries and
logs stay outside Git. No baseline is selected until a clean run is reviewed.

## Exploratory Native Reports

The direct Make targets below retain the original detailed measurements and
experiment knobs. They are not accepted as regression reports without the
independent-run envelope above.

For shared target repair, run `make bench-repair REPAIR_REPORT=research/repair.jsonl`.
It alternates command-first regions (strategy 0), region-first replay (1),
128x32-pixel coverage tiles (2), bounding rectangles (3) and full repaint (4).
Cases 0..4 are separated corners, horizontal strips, vertical strips, a large
continuous region and 192 small rectangles. Reports retain raw nanosecond samples,
pixel coverage, logical commands/blits and requested/actual native thread counts.
Preparation runs once per frame in every strategy. These are headless renderer
measurements, not window presentation or game-impact measurements.

For the actual GUI rail animation, compare `make bench-gui
GUI_BENCH_CASE=rail_animation_benchmark
GUI_BENCH_OUTPUT=research/rails.json` against the same command with
`GUI_BENCH_BOUNDING_DAMAGE=1` and a different output path. Alternate run order;
keep unrelated builds and benchmarks stopped.

```bash
make bench-overlays
make bench-overlays BENCH_SAMPLES=120 BENCH_OUTPUT=research/retinue/benchmarks/render-after
make bench-overlays BENCH_THREADS=2 BENCH_OUTPUT=research/retinue/benchmarks/render-queued
make bench-overlays BENCH_THREADS=2 BENCH_HZ=60 BENCH_CASES=warm-repaint,scene-change
make bench-overlays BENCH_THREADS=4 BENCH_HZ=60 BENCH_CASES=hover,hover-partial
make bench-overlays BENCH_THREADS=2 BENCH_HZ=30 BENCH_CASES=static-frame-copy,scene-change,loading-buffer-cycle
python3 -m unittest discover -s tools/benchmarks
```

Default native reports: `research/retinue/benchmarks/<UTC time>/{warframe,poe2}.json`.
The target builds release test binaries, uses deterministic fixtures and needs
neither game nor daemon. It does not stage binaries or reload a running companion.
The helper is compiled only into tests; normal companions have no instrumentation.

PoE2 also reports description, scene update, preparation, submission and native
completion totals, plus section layout-update counts. Compare the same binary with
`RETINUE_BENCH_FULL_LAYOUT=1` and/or `RETINUE_BENCH_FULL_REPLAY=1` to disable local
reductions. These counters include cold/warmup frames; warm latency samples do not.
`RETINUE_BENCH_NO_LAYERS=1` disables optional subtree promotion while retaining
command reductions. Reports include promotions, demotions and peak layer bytes.

Each case records setup, first paint, warm p50/p95/p99/max, raw frame samples,
caller and process CPU time, process RSS/high-water RSS and known pixel-buffer sizes.
Reports include CPU, kernel, Rust version and Git/dirty identity. Keep the machine
similarly idle for comparisons; run multiple times before interpreting small changes.
No unstable timing threshold is enforced in CI.

`BENCH_THREADS=1` (the benchmark default) queues work on the caller; values from
`2` through `8` include native workers. Zero is rejected. The count includes
the caller, not just workers. Reports record requested mode and
actual native thread count. Completion is inside the timed frame. Compare counts
in both execution orders; lower wall time can still consume more total CPU.
Production uses the same queued renderer, with preferred budgets of two for
Warframe and four for PoE2, capped by available CPUs. Benchmarks override the
budget to measure diminishing returns.

`BENCH_HZ=0` runs without idle gaps to measure throughput. A positive value caps
repaint cadence by sleeping between frames. `BENCH_CASES` selects exact case
names separated by commas; an empty value runs all cases.
Warframe's `loading-buffer-cycle` rotates three buffers through the same cached
frame repair and transient paint path as its Wayland host.

Latency excludes the sleep; CPU utilization includes the entire measured interval.
`process_cpu_percent` uses 100% for one logical CPU and includes all native workers.
`process_cpu_percent_available` divides that value by the process's available
logical CPU count. Neither is a measure of processor throughput capacity: SMT,
CPU frequency and shared resources matter. Reports include achieved repaint rate
and frames whose render duration exceeds the requested interval.
Compare utilization at the same cadence and workload. A continuous-throughput
run is not normal companion CPU usage; a paced headless run still excludes
observation, input, compositor and game activity. A live idle view can skip paint.

| Companion | Cases |
| --- | --- |
| Warframe | Static-frame reuse/copy; changed scene |
| PoE2 | Forced warm repaint; full/partial hover; listing scroll; alternating 5% scale |

PoE2 uses 1080p and 1440p at 100%, 125%, 150% and 200% UI zoom. Warframe uses
1080p, 1440p and 2160p; its scale field is buffer scale, not independent UI zoom.
Its fixture has embedded chrome but no
daemon-delivered artwork. Warm-repaint deliberately calls paint even though a
clean live view would skip it.
`hover` and `hover-partial` exercise the same Search hover changes, with full or
clipped painting. `repair_pixels` records the selected raster area. They use one
buffer; rotating-buffer correctness is covered by unit tests, not these timings.

RSS covers the whole test process, not just caches. High-water RSS is cumulative
across cases; known resources are partial accounting, not total retained memory.
PoE2 reports chrome/glyph cache hits, misses, evictions, cached/live bytes and
bytes retained by callers. These exclude source decoding and font geometry.
Native image counters include pixels retained by queued operations, overlap cache
totals, and exclude native metadata. Process CPU includes native workers; caller
CPU alone does not. Background image preparation is not exercised. These headless
results exclude compositor uploads, frame scheduling, game impact and network I/O.

Extend cases alongside shared cache/damage changes: decoded/raster bytes and
evictions, preparation misses, partial-versus-full damage, caret, remap and
asynchronous asset replacement. Live idle wakeups, input latency and KWin/game
frame pacing require separate captures, not extrapolation from these reports.

## Mapped Scheduling

```bash
make bench-overlay-scheduling
make bench-overlay-scheduling BENCH_SAMPLES=120 BENCH_SCALES=1,1.25,1.5
make bench-overlay-scheduling BENCH_CASES=async-hover BENCH_SCALES=1
```

Requires a Wayland session with layer-shell and presentation-time support. Shows
a temporary, click-through Evaluate fixture on the real compositor. It takes no
keyboard focus, sends no game input and makes no network requests. The game and
daemon are optional. Existing companion processes are not replaced or reloaded.

Compares the former 16 ms mapped/100 ms hidden polling policy without explicit
worker wakes against current wake/deadline scheduling, in ABBA order. Both use
identical current rendering, caches and surface code: this isolates scheduling,
not the cumulative difference from an older application build. Four rendering
threads by default; `BENCH_THREADS` overrides this for both policies.

Cases: mapped idle, asynchronous hover changes, asynchronous listing scroll and
the current 70 ms loading animation deadline. Scripted updates model worker-ready
notifications, not physical mouse/keyboard latency. Cold mapping and 600 ms of
buffer/cache warmup are retained in the trace but excluded from latency summaries.

`poe2-scheduling.json` records ready-to-paint, completed paint, commit-to-present
and ready-to-present p50/p95/p99, presentation intervals, discarded/unresolved
frames, coalesced updates, dispatch counts, CPU utilization and memory. Presentation
uses the compositor's advertised clock; timestamps from different clock domains
are never directly subtracted. Unsupported presentation timing fails explicitly.
CPU totals exclude warmup and include producer and final feedback drain; 100%
means one logical CPU. Recording is bounded
in RAM and written between cases, not per frame. Normal builds exclude it.

Keep display refresh, game/background load and scale unchanged during a run.
No noisy performance threshold is enforced. This measures overlay scheduling and
presentation only, not game impact. Game frame-time recording remains separate.

## GUI Memory And Motion

### Paint Replay

`make bench-gui GUI_BENCH_CASE=foundry_drag_benchmark
GUI_BENCH_OUTPUT=research/drag.json` exercises scrollbar dragging with eight cursor
events per paint, both over the track and across the grid, at 100%, 125% and 200%.
It includes the native host's cursor queries and release routing; reports separate
input, submission, completion and process CPU time. Fixture-only, no presentation
timing or live account changes. Wheel/jump-only traces do not cover this workload.

```bash
make bench-gui GUI_BENCH_CASE=paint_replay_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/paint-replay.json
GUI_BENCH_REVERSE=1 make bench-gui \
  GUI_BENCH_CASE=paint_replay_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/paint-replay-reverse.json
```

Captures 32 immutable paint frames from a fixture Relic grid: forward/reverse
scroll, a changed label and reordered items. Drops the producing page before
replaying identical frames through raw commands, compiled commands, item caching
and current layer admission. This does not compare row/tile policies yet.

Capture, resource preparation and pixel oracles are outside timing. Each policy
starts with an empty renderer cache; fixture glyphs and embedded icons are already
prepared. Remote item artwork is absent, so this trace alone cannot choose the
best cache policy for fully populated grids.
Reports include the first cycle plus two subsequent cycles, preparation/submission/
native completion, caller/process CPU, cache work/bytes and pixel error. Raw versus
compiled must be exact; cached paths permit at most one channel step from existing
premultiplied intermediate-image rounding. A mismatch fails the run.
The captured region must not depend on live callbacks. Reports exclude layout,
presentation and game impact; use the end-to-end scroll benchmarks alongside them.

### Memory And Motion

```bash
GUI_BENCH_CYCLES=6 make bench-gui \
  GUI_BENCH_CASE=interaction_residency_benchmark GUI_BENCH_LIVE=1 \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/gui-memory.json
make bench-gui GUI_BENCH_CASE=subtree_motion_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/gui-motion.json
make bench-gui GUI_BENCH_CASE=adaptive_motion_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/gui-adaptive-motion.json
```

Memory cycles exercise filters, queries, scrolling, category/page changes, context
menus, rails and scales. They use the real layout/cache paths but no desktop input
or account mutations. Compare matched end states across several cycles, not a
single open/close pair. Output is streamed so measurements do not grow the heap.
Preparation/paint timing excludes the memory diagnostic snapshot itself. It still
includes asset preparation and any required daemon reads, so use the focused
render cases to isolate allocator or raster performance.

For Valgrind, first build the test through Make, then run the resulting `wfgui`
test executable under Memcheck or Massif with the same environment and
`shell::benchmark::interaction_residency_benchmark --exact --ignored --test-threads=1`.
Store logs under `research/`. Valgrind memory overhead and timing are not application
measurements; use it to attribute allocations and find invalid accesses/leaks.

An isolated `LD_PRELOAD` allocator comparison covers Rust's system allocator and
native Blend2D/AsmJit heap allocations. A Rust-only allocator replacement does not.
Keep allocator options explicit and compare RSS, live resources and latency;
lower retained memory may cost more page faults. Never change the user's global
allocator configuration for a benchmark.

The motion case compares existing card rasters with an additional isolated grid
layer at 100%, 125% and 200%, including creation and content invalidation. Reports
include the first 12/20 frames, full-run latency, layer bytes and pixel equivalence.
The adaptive case includes admission delay, promotion and content-change demotion.
Use `GUI_BENCH_ADAPTIVE_FIRST=1` to reverse execution order in repeated comparisons.
The GUI benchmark accepts `RETINUE_BENCH_NO_LAYERS=1` for other page traces too.
