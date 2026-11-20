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

Also available: `wf-overlay`, `poe2-drag`, `gui-foundry`, `gui-relics`, `gui-drag`,
`gui-transfer`, and [optimizer quality](../../docs/developer/optimizer-quality.md) (`render-quality`).
`poe2-drag` compares full repaint, damage repair and automatic relocation through
the shared acquired-buffer renderer; it does not measure compositor or game FPS.
`text-layout` compares native paragraph reflow with fresh shaping and run resolution;
it excludes painting and platform presentation.
`text-view` measures production `Text` updates: fresh trees, width changes, color-only
changes and line-limit elision. It includes layout and command recording, not painting.
`view-ir` is a representation experiment: the same Cycle display through retained
ViewTree or handwritten template bindings, with pixel parity checked separately.
It measures no-op, timer, color, width and offset updates, with and without native
painting, at 100/120/150%. Templates fuse static wrappers manually; this is not a
compiler or full-app speedup. Instance byte counts exclude owned/backend storage.

`view-ir-observed` keeps 32 rows mounted and changes one observable row. Unlike
collection restaging, this measures dependency-driven updates with the same
observation registrar on both backends. It covers no-op and timer changes at
100/120/150%, with separate update-only and painted cases. Its `context` case
adds retained ViewContext and clock handling, but requests no ticks. The `compiled`
case uses the new typed authoring compiler instead of handwritten IR bindings.
Native pixels and materialized bounds are checked outside timing; no application
migration or compositor work is included.

`view-ir-input` changes one observed callback payload among 32 or 512 mounted
controls. It measures owner evaluation and published input updates, not pointer
dispatch or painting. The unchanged case checks that input support adds no recurring
work. Fixture tests compare target bounds/options and preserve recorded programs.

For allocation attribution, build with `make test-ui-sdk UI_TEST='--lib --no-run'`,
then run the emitted test executable under DHAT/heaptrack with the selector
`ir::collection_benchmark::allocation_profile --exact --ignored --test-threads=1`.
This reuses the 32-item workload at 120% without timing queries or native painting.
Select `RETINUE_ALLOC_BACKEND=graph|views`, `RETINUE_ALLOC_CASE=noop|timer|reorder|replace|filter-cycle`
and `RETINUE_ALLOC_SAMPLES=400`. Compare two sample counts to separate recurring
allocations from setup; totals include startup and shutdown. Debug allocation
profiles are attribution evidence, not release timing or total application memory.
The selector `ir::observation_benchmark::allocation_profile` instead exercises
observable invalidation, with backends `graph|context|compiled|views` and cases `noop|timer`.
Use `ir::input_benchmark::allocation_profile` for published callbacks, with backends
`graph|views` and cases `noop|handler`; it uses 32 controls and no timing queries.

`text-editor` measures a single field's input and native painting at 100/120/175%:
retained repaint, selection, viewport resizing and text insertion. No presentation.
`execution` compares serial work, individual DAG jobs and four-item batches for
native shaping, card rasters and row composition. `BENCH_THREADS` bounds the CPU
pool; each offscreen native pass uses its caller only. Pixel equality is a separate
correctness test. This workload does not represent a complete application frame.
`layout-batches` compares caller-serial, queued-serial and cooperative sibling
layout for real grid/stack/Text trees. It includes cold state allocation,
partitioning and completion, with warm font registrations. The reflow case keeps
shaping caches while alternating widths; the skewed case clusters long text to
test load balancing. It excludes body evaluation, paint recording and presentation;
it is not a full-frame speedup.
`gui-execution` compares direct owner painting, immutable packet painting on the
owner, queued native execution with inline layout, and queued layout plus native
execution. It includes fixture layout, recording, cache preparation and native
completion at 100/120%, separating owner work from layout waits and native work.
It excludes platform presentation and remote artwork. The queued-layout case is
also checked against inline layout for exact frame pixels.

For exclusive owner-thread attribution, use the same `gui-execution` suite with
`BENCH_FEATURES=profiling`. `known_resources.owner_exclusive_ns` partitions each
frame across all UI roots; its per-phase means are retained in numerical reports.
Use `BENCH_FEATURES=ir-profiling` for the finer (more expensive) IR trace.
`known_resources.owner_ir_ns` subdivides only `ir` into observation, typed
output resolution, retained composite handling, staging, keyed children,
topology, publication (including the name index), retirement and the remaining
dispatch time. Corresponding `work_counts.ir_*` count entries, not nodes.
Detailed reports also retain means separately for frames with a topology walk
(`ir_structural_*`) and without one (`ir_stable_*`). Do not
add these to the parent again. A shared helper called from another phase
stays charged to that phase.
`body` covers application view builders; `record` covers paint callbacks (including
native text preparation); `programs` covers recorded-program compilation. `ir`,
`bindings`, `reconcile`, `projection` and `input` cover framework work. `layout`
is owner-side solving/admission/publication, not concurrent worker CPU time.
`BENCH_FEATURES=layout-profiling` adds queued-job timing. In
`known_resources.layout_job_ns`, dispatch, worker wall time and pickup are
sums over completed jobs. Only the four `layout_wait_*` columns partition
owner wait: when jobs overlap, solving takes precedence over dispatch, then
pickup. Serial CPU excludes the parallel work scope; batch CPU sums the
executing child groups, including the caller's share. Split and merge CPU
are subsets of serial CPU, not extra work. Slot-map and gather CPU are parts
of split; gather lends disjoint mutable geometry to the groups. Batch wall
measures the whole parallel scope, not the sum of worker CPU. The layout
computation count excludes solver cache hits. Use work counts to distinguish
the per-frame means from costs per job. All worker probes compile out by default.
`host` is residual application/host work, not proven pure framework overhead.
Preparation, submission and completion waits have separate fields. Native elapsed
time overlaps native wait: do not add it again to this exclusive partition.
Use uninstrumented runs for performance verdicts. Features are part of snapshot
identity, so instrumented and ordinary builds cannot form a paired comparison.

`poe2-overlay` compares inline and worker-eligible layout at 100/120/150%; both use the
existing caller-side final paint path. The `-queued` cases offer the executor
without forcing admission: small trees may run inline. Raw results report actual
`layout_waits`. It does not measure platform presentation
or the production host's final native-pass queue.
The runner builds one release test binary, verifies an exact ignored-test selector,
saves it with source/binary/toolchain/fixture provenance, then runs independent
processes. It does not stage or reload applications. Each run gets fresh XDG
cache/config/state directories; OS file cache is **not** flushed. Fixture-only
GUI traces do not represent a fully populated live artwork cache.

The output contains `snapshot/`, `measurements/report.json`, per-run native JSON
and process/build logs. Direct collection defaults to seven runs; fewer are useful
smoke checks, not enough for a performance verdict. GUI traces have fixed lengths;
`BENCH_SAMPLES` controls overlay warm frames or complete optimizer-quality traces.

Raw reports live in ignored `research/`. Selected compact measurements can be
archived as [JSON records](history/records/). The [performance history](https://zeewanderer.github.io/retinue/research/benchmarks/) tracks the same test setup across code, compiler and system changes. Each session has an ID, UTC timestamps
and source provenance. A commit identifies the measured code; it need not have
its own run or become a baseline.
In the history chart, session order separates runs made seconds apart; elapsed
time shows their actual spacing. The source link identifies the measured commit,
the binary hash identifies what ran, and the data link identifies one session.
Repeated measurements of the same binary are useful controls. Vertical bars show the
middle 50% of independent runs; they are not confidence intervals. Use **Compare against**
to see absolute and relative changes from the previous session or a specific run.
Leave **Configuration** on **All** to see end-to-end trends; narrow it to isolate recorded
conditions. Only the regression test requires an exact configuration match.
The rail scenario measures window chrome with an unpopulated Foundry, not a
populated grid reflow. Use the scrolling/drag scenarios for populated grids.
`gui-rail-updates` exercises the production compiled Events rail with 18 fissures
at 100/120/150%: idle, one countdown, all visible countdowns and warm repaint.
Inline and queued layout share the same fixture. Reports separate owner work,
layout waits, preparation and native painting, plus layout/recording counts.
Native `known_resources.work_counts` arrays become named count metrics in the
normalized history; each array must cover exactly the measured frames.
Use at most 3,000 samples so countdowns remain in their seconds-resolution range.
Fresh-layout pixel checks run outside measurement; network and remote artwork
are excluded.

For stronger A/B evidence, prepare each revision separately, then interleave the
saved executables without compiling during measurement:

```bash
make bench-prepare BENCH_SUITE=gui-rails BENCH_OUTPUT=research/bench/before
# After implementing the candidate:
make bench-prepare BENCH_SUITE=gui-rails BENCH_OUTPUT=research/bench/after
python3 tools/benchmarks/run.py pair research/bench/before research/bench/after \
  --output research/bench/paired --repeats 8 --threads 4
python3 tools/benchmarks/compare.py research/bench/paired/before/report.json \
  research/bench/paired/after/report.json --policy research/bench/policy.toml \
  --output research/bench/verdict.json
```

Each round measures both builds; alternating order gives ABBA blocks. Use the
same build environment for both preparations. `bench-prepare` shares compiler and
sccache settings with normal Make builds; invoking the Python preparer directly
requires supplying that environment yourself. All-else-equal is still the
operator's responsibility: stop unrelated builds, keep power mode stable and
record whether the machine is busy. Load averages are evidence, not a noise filter.
Copied binaries are local snapshots, not portable packages; keep their dynamic
runtime libraries installed. Native dependency revisions are recorded, not
assumed equivalent.

Check repeatability before interpreting small differences: use the same snapshot
for both arguments to `pair`. Inconclusive identical-binary comparisons indicate
that the run cannot reliably resolve the chosen tolerance. Keep that evidence;
do not enlarge tolerances simply to pass a candidate.

On Linux, `taskset --cpu-list <CPUs> python3 tools/benchmarks/run.py pair ...` can
reduce migration and SMT variability. Use `lscpu -e=CPU,CORE,SOCKET` to choose
distinct physical cores, with enough CPUs for the requested thread budget. Apply
the same mask to both builds and repeat the identical-binary check. This affects
only benchmark processes; it neither isolates those cores from desktop activity
nor recommends an application scheduling policy. Affinity is recorded and forms
part of the comparison/history identity.

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
every commit. Only measurements built from a clean checkout of a commit in the
repository's history can be archived. To publish a working-tree experiment,
commit the change and rerun the benchmark; do not relabel its old result.
Records are immutable; baseline selection is separate and explicit.
For an LLVM LNT instance, export a record with
`python3 tools/benchmarks/timeline.py lnt MEASUREMENT_ID > run.json` and use
`lnt submit SERVER_URL run.json`. The LNT `nts` export includes timing metrics
in seconds, one sample per process. Memory and counter metrics remain available
in the static browser. See [LNT's import format](https://llvm.org/docs/lnt/importing_data.html).
Git retains the selection reason and previous measurement. Series match workload,
fixtures, build settings and machine, not source commit. Uncommitted candidates
can still be compared locally against a compatible baseline. Raw frames, traces, binaries and
logs stay outside Git. No baseline is selected until a clean run is reviewed.

## Profiling

Use [external profilers and matching debug symbols](../../docs/developer/profiling.md)
to explain regressions. There is no embedded capture service. Profiled runs are
diagnostic evidence, not timing baselines; keep their outputs separate from the
unprofiled reports above.

## Exploratory Native Reports

The direct Make targets below retain the original detailed measurements and
experiment knobs. They are not accepted as regression reports without the
independent-run envelope above.

`gui-transfer` compares repair, in-place relocation, independent previous buffers,
full/partial scratch snapshots and an offscreen pair with final presentation copies.
It rotates model buffers at 2560x1440 with desktop scale 1.0. GUI scale defaults
to 90-130% in 5% steps, plus 150% as a higher-scale reference; 100% and 120% are
the primary tuning cases. Each report records these dimensions separately.
It does not measure compositor waits or present latency. The native test accepts
`GUI_BENCH_SCALES=1,1.2,1.5` to select scale ratios (75-200%, rounded to whole
percentages like the GUI),
`GUI_BENCH_ARTWORK=<json>` with `{id,path,digest}` asset descriptors and
`GUI_BENCH_REVERSE=1` to reverse candidate order. These extra experiment variables
are not forwarded by the standard runner; use `make bench-gui` or a saved benchmark
binary directly and retain its source manifest, exact environment and per-process output. Artwork
is reused on synthetic cards and components, not presented as a live player dataset.
All art must be warmed before timing; the test asserts this throughout the trace.
`previous-3`, `previous-cpu` and `previous-cpu-parallel` compare queued native,
serial and persistent row-parallel copies using the same source and plan. Workload
7 uses the scrollbar's actual travel, verifies that every trace frame moves content,
and retains the loaded static rail and equal per-frame copy/repair coverage checks.
`GUI_BENCH_CASES` filters these experiment cases. The copy workers and overrides
are test/benchmark-only.
`GUI_BENCH_MATERIALIZATIONS=explicit|fragments|none` compares Foundry's authored
instance rasters, automatic fragments only, or no optional rasters. Asset and glyph
caches stay enabled. Reports include preparation work and raster residency;
diagnostic collection occurs after the frame timer stops. These are experimental
overrides, not application settings. `GUI_BENCH_FEATURES=profiling` on `make bench-gui`
adds test phase attribution; keep those runs separate from unprofiled comparisons.

For shared target repair, run `make bench-repair REPAIR_REPORT=research/repair.jsonl`.
It alternates command-first regions (strategy 0), region-first replay (1),
128x32-pixel coverage tiles (2), bounding rectangles (3) and full repaint (4).
Cases 0..4 are separated corners, horizontal strips, vertical strips, a large
continuous region and 192 small rectangles. Reports retain raw nanosecond samples,
pixel coverage, logical commands/blits and requested/actual native thread counts.
Preparation runs once per frame in every strategy. These are headless renderer
measurements, not window presentation or game-impact measurements.

`make bench-reuse REUSE_REPORT=research/reuse.jsonl` compares automatic discovery
against direct replay of the same unhinted drawing. Cases cover one moving instance
and 72 independent consumers at 100/125/200% scale with two/four rendering threads.
It records preparation, submission, completion, process CPU, fragment blits and
selected pixel bytes. Modes alternate within each process; repeat in independent
processes before drawing conclusions. Pair the production `gui-transfer` snapshots
separately to detect overhead outside this intentionally reuse-heavy experiment.

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
completion totals, plus changes to the retained pixel layers. Compare the same binary with
`RETINUE_BENCH_REBUILD_LAYERS=1` and/or `RETINUE_BENCH_FULL_REPLAY=1` to disable local
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
PoE2 reports chrome cache hits, misses, evictions, cached/live bytes and
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
An update belongs to the frame whose rendering was prepared with that revision,
not whichever frame happened to commit after the update arrived.
CPU totals exclude warmup and include producer and final feedback drain; 100%
means one logical CPU. Recording is bounded
in RAM and written between cases, not per frame. Normal builds exclude it.

Keep display refresh, game/background load and scale unchanged during a run.

For physical pointer input use `make bench-overlay-pointer BENCH_SECONDS=20 BENCH_SCALE=1.2`.
Move the pointer between controls on the temporary Evaluate panel. This fixture
accepts pointer input only inside the panel; it never requests keyboard focus.
The report measures handler entry to compositor presentation for changed hover
states, not hardware-to-screen latency. It counts adjacent motions within each
Wayland pointer batch (discrete events break a run) and motions received during
raster work. It records neither positions nor key events. No timed hover
changes is a missing sample, not zero latency.
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
premultiplied intermediate-image rounding, or three only inside known glow regions.
A mismatch outside those bounds fails the run.
The captured region must not depend on live callbacks. Reports exclude layout,
presentation and game impact; use the end-to-end scroll benchmarks alongside them.

### Completed-Target Reuse

```bash
GUI_BENCH_THREADS=4 make bench-gui GUI_BENCH_CASE=target_reuse_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/target-reuse.json
```

Uses the same Relic trace to compare ordinary viewport repair with translating
completed target pixels, then repairing exposed strips and changed-content holes.
Runs compiled commands, item caches and layer caches at 100/125/200%, with one or
three backing buffers. There is no additional pixel allocation for reuse; metadata
and existing renderer caches are reported separately. Each buffer tracks its own
source state. Incompatible content/order/target falls back to repaint.

This is benchmark-only, not a production admission policy. The scope must be an
isolated recorded viewport over a constant backdrop. Copying happens after native
completion, before creating the next context; it does not queue a self-blit.
Planning, copying, preparation, submission and completion are timed separately.
Pixel checks precede timing, including same-clip repair checks and the existing
glow-only rounding tolerance. Current-frame presentation damage is not reduced by
target reuse. Repeat in fresh processes and alternate `GUI_BENCH_REVERSE=1` before
drawing conclusions; this measures neither input/layout nor compositor/game cost.

For a synthetic control with substantial admitted row/viewport rasters:

```bash
GUI_BENCH_THREADS=4 make bench-gui GUI_BENCH_CASE=target_materialization_benchmark \
  GUI_BENCH_OUTPUT=research/retinue/benchmarks/target-materialization.json
```

This deliberately primes caches before scrolling at 100/200%, then reports both
target-reuse and ordinary-repair paths, with and without materialization. Exclude
the first three motion frames from warm statistics. It isolates representation
costs, not production admission frequency. The normal GUI tests verify pixels
while target copying and substantial cached rasters coexist.

### Native Cost Ranking

```bash
make bench-cost COST_REPORT=research/retinue/benchmarks/native-cost.csv
```

Alternates warm native replay and independent-source copies for fills, rounded
fills/outlines, stroked circles, text, images and mixed programs at six sizes and
1/2/4/8 requested threads. CSV rows
include work descriptors, submission and completion times. Repeat in fresh
processes; validate models on held-out sizes and mixtures. Compare selected-plan
regret against the faster measured alternative, not predicted-time accuracy.
This isolates native work. Whole-frame tests must still account for planning,
remaining repairs, cache state and UI overhead.

### Native Updates And Coefficients

`cost::WARM_NATIVE` in [cost.rs](../../crates/retinue-render/src/cost.rs) holds the
relative warm replay/transfer coefficients. It is not an exact timing table or
the fragment-admission rule. `cost::cache::NATIVE` separately prices cold caller-only
construction against warm replay/blit savings, including the target allocation and
initialization. Native outline glyphs and cached masks are distinct work terms.
Recheck their decisions after Blend2D, AsmJit,
bridge or native build-option changes; do not scale every coefficient by an overall
speedup. Rounded geometry, native glyphs, images and transfers can change differently.

`promotion-cost` measures construction, replay and blit separately for solid and
rounded fills, rounded outlines, stroked circles, glows, native glyph runs and mixed
content. It covers three sizes/counts at 100/120%; reports include actual participant
counts and the model's required reuse count. Compare that count against construction
divided by measured replay-minus-blit, and inspect wrong promotion/rejection choices.
Treat a nonpositive or noisy savings difference as inconclusive, not infinite speedup.
The curved cases exercise the native geometry paths used by the app, not arbitrary
conic paths or conic-gradient paints that the command API does not expose.

Save the baseline **before** updating dependencies, then prepare the candidate:

```bash
make bench-prepare BENCH_SUITE=promotion-cost BENCH_OUTPUT=research/bench/native-before
# Update native dependencies, then:
make bench-prepare BENCH_SUITE=promotion-cost BENCH_OUTPUT=research/bench/native-after
python3 tools/benchmarks/run.py pair research/bench/native-before research/bench/native-after \
  --output research/bench/native-pair --threads 4 --repeats 7 --samples 180
```

Snapshots record the binary, source/build identity and native submodule revisions.
Use clean native checkouts or retain their local patches separately. Repeat at one
participant and the relevant host budget, with the same CPU affinity and no competing
builds. Run an identical-binary A/A control. New workload versions intentionally
require a new baseline; do not compare different fixtures as a library speedup.

Fit relative choices on primitive cases; validate on held-out sizes/mixtures and the
`render-quality`, GUI and overlay workloads. Minimize selected-plan regret against
measured alternatives, not predicted-time error. Include construction payback,
partial repair, CPU and memory. If one work category no longer distinguishes native
paths with different costs, refine that descriptor instead of disguising the gap
with another global constant. Update coefficients only with reviewed evidence;
neither shipping auto-tuning nor a performance failure is inferred from a new pin.

For opt-in recording-phase detail, `make bench-gui GUI_BENCH_FEATURES=profiling
GUI_BENCH_CASE=independent_source_benchmark GUI_BENCH_OUTPUT=...` includes the
existing SDK phase counters. Those results are diagnostic, not uninstrumented
acceptance timings. Normal builds do not enable the profiling feature.

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
