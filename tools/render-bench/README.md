# Overlay Benchmarks

```bash
make bench-overlays
make bench-overlays BENCH_SAMPLES=120 BENCH_OUTPUT=research/retinue/benchmarks/render-after
make bench-overlays BENCH_THREADS=2 BENCH_OUTPUT=research/retinue/benchmarks/render-queued
make bench-overlays BENCH_THREADS=2 BENCH_HZ=60 BENCH_CASES=warm-repaint,scene-change
make bench-overlays BENCH_THREADS=4 BENCH_HZ=60 BENCH_CASES=hover,hover-partial
make bench-overlays BENCH_THREADS=2 BENCH_HZ=30 BENCH_CASES=static-frame-copy,scene-change,loading-buffer-cycle
python3 tools/render-bench/compare.py research/retinue/benchmarks/render-before research/retinue/benchmarks/render-after
python3 -m unittest discover -s tools/render-bench
```

Default reports: `research/retinue/benchmarks/overlays/<UTC time>/{warframe,poe2}.json`.
The target builds release test binaries, uses deterministic fixtures and needs
neither game nor daemon. It does not stage binaries or reload a running companion.
The helper is compiled only into tests; normal companions have no instrumentation.

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
