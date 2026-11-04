# Completed-Target Reuse Experiment

Benchmark-only prototype, not enabled in applications. It translates completed
CPU target pixels, then paints exposed strips and changed-content holes. No extra
raster is allocated for reuse. Painter completion precedes copying; no queued
Blend2D self-blit or native bridge change is involved.

## Measurement

Eight fresh processes; reversed policy/A-B order on alternate runs. Four rendering
threads pinned to CPUs 4-7 on an i7-10700. The same 32-frame Relics trace contains
forward/reverse scrolling, a changed label and reordered items. First cycle excluded
from warm summaries. Physical target: 1800x1360, at 100/125/200% UI scale.

One and three destination buffers were tested. Three means rotating A/B/C, each
with its own prior content, not three threads or three added caches. Each target
is 9.34 MiB; reuse adds 43-152 KiB of metadata per target, depending on visible
content. It does not retain source fonts, images or paint programs.

Mean of eight per-process warm medians, milliseconds, **three buffers**:

| Scale | Execution Policy | Ordinary Repair | Target Reuse |
| --- | --- | ---: | ---: |
| 100% | Compiled, no raster materialization | 5.171 | 1.438 |
| 100% | Explicit caching | 1.653 | 1.305 |
| 100% | Layer admission enabled | 1.632 | 1.312 |
| 125% | Compiled, no raster materialization | 4.372 | 1.350 |
| 125% | Explicit caching | 1.310 | 1.127 |
| 125% | Layer admission enabled | 1.287 | 1.128 |
| 200% | Compiled, no raster materialization | 3.776 | 1.124 |
| 200% | Explicit caching | 0.992 | 0.902* |
| 200% | Layer admission enabled | 1.023 | 0.975* |

`*` Improvement is inconclusive: the bootstrap 95% difference interval crosses zero.
Across all 18 comparisons, median improvement was supported in 16, p95 improvement
in 12, process-CPU reduction in all 18. These are exploratory per-case intervals,
not a simultaneous confidence guarantee or a calibrated regression gate.

At 100% with three buffers and explicit caching, mean process CPU fell from 4.21
to 1.60 ms/frame. Mean repaired area fell from 2.32M to 0.27M pixels. Planning took
0.15 ms and copying 0.71 ms: copying is now the largest phase. With one buffer,
cached-policy median improvements were 33-39% across the three scales.

## Limits And Decision

- Keep this as an experiment until broader reuse/cost evaluation is complete.
  It supports combining target reuse with existing caches, not replacing all caches.
- Only 5-21 KiB of live raster storage was observed in the Relics fixture. The
  additional cache-heavy control below covers substantial row/viewport allocations.
- Fixture glyphs/shapes/embedded icons only; no remote item artwork, input/layout,
  compositor timing or game FPS. Desktop load was uncontrolled. System I/O pressure
  remained elevated after linking; no compiler jobs ran during measurement.
- Pixel checks passed at all scales/policies/buffer counts. Fresh-render differences
  reached two channel units only in known glow regions; identically clipped repairs
  stayed within one. Integer-geometry/hover tests require exact pixels.
- Next compare whole-overlap copying plus holes against fragmented copying, with
  changing hover, richer cache workloads and actual moving-overlay traces. Preserve
  painter order, buffer ownership, clipping and unchanged presentation damage.
  Also evaluate reading a different completed target if host ownership APIs allow
  it; disjoint images need not use the overlap-safe in-place copy path.

## Cache-Heavy Control

A second experiment uses dense rounded shapes at 100/200%, with the same physical
target, one/three buffers and four threads for preparation and final painting.
Twenty-four stationary repaints deliberately prime row or viewport caches before
96 frames of forward/reverse movement. This is a favourable prewarmed-cache
control, not a model of idle application behaviour. Eight fresh processes alternate
ordering; warm summaries exclude the first three motion frames.

The viewport has an explicit fixed backdrop, keeping its allocation at 1760x1320.
An earlier diagnostic control lacked that backdrop; tightening visible bounds made
the viewport cache resize/demote, invalidating a steady-cache comparison. That run
is retained separately and is not used in the following results.

Mean warm median milliseconds, three buffers; raster memory is mean live optional
pixel storage **with reuse**, excluding the unchanged presentation pool:

| Scale | Coverage / Policy | Ordinary Repair | Target Reuse | Raster MiB |
| --- | --- | ---: | ---: | ---: |
| 100% | Row grouping, no materialization | 1.986 | 0.912 | 0 |
| 100% | Materialized rows | 1.415 | 0.956 | 7.12 |
| 100% | Viewport grouping, no materialization | 1.976 | 0.925 | 0 |
| 100% | Materialized viewport | 3.299 | 3.194 | 8.86 |
| 200% | Row grouping, no materialization | 1.036 | 0.919 | 0 |
| 200% | Materialized rows | 1.351 | 1.082 | 5.78 |
| 200% | Viewport grouping, no materialization | 1.044 | 0.907 | 0 |
| 200% | Materialized viewport | 2.992 | 2.938* | 8.86 |

Across the 16 within-policy reuse comparisons, 15 median and 12 p95 improvements
have bootstrap difference intervals below zero; all 16 process-CPU reductions do.
The marked viewport comparison is inconclusive. These remain exploratory,
per-case intervals. Cross-policy table differences are descriptive, not a separate
calibrated significance claim.

At 100%/three buffers, viewport cache preparation still consumes 2.30 ms/frame
with reuse enabled. Target movement alone cannot remove that work because current
cache preparation does not know the remaining repair demand. Removing this
movement-only materialization in the control eliminates its 8.86 MiB allocation;
target reuse adds about 8 KiB metadata per target. This is renderer allocation
accounting, not a measured application RSS saving.

The chosen [production design](../../../docs/developer/render-relocate.md)
therefore selects target repairs **before** materialization preparation and evaluates
caches against the remaining work. This control supports replacing movement-only
rasters; the Relics experiment supports retaining useful shared small caches.
Neither justifies universally forcing or disabling raster caching.

Correctness checks exercise simultaneous admitted caches and copied target pixels
at both scales, against direct rendering with at most one channel-unit difference.
This control has no artwork, changing hover, layout/input, compositor or game work.
Actual overlay movement and whole-copy versus fragmented-copy evaluation remain.

## Evidence

Run through the [benchmark guide](../README.md#completed-target-reuse).
Raw reports, exact binary/source provenance and the collection script are retained
under `research/retinue/benchmarks/target-reuse-20261103/`; measured snapshot is
`snapshot-clang`, binary SHA-256
`d31f29c14016d82136fa543536d19ddea9e2a33c1dbec654b09083a9874d4647`.
Source was dirty against `cb0a0ce5a4ddbca88fb40a11184ece2693342447`; no baseline was
replaced. Local Blend2D docs matched vendored revision
`58ca9460b4138af6e793184682e9324e0a8053b2`.

The cache-heavy control uses `materialized-fixed-snapshot` and
`materialized-fixed-measurements`, binary SHA-256
`08a3d81c79810517feac7a4154c5d77954fac35faae3a4aef9f98d8ebb01802a`.
Use `make bench-gui GUI_BENCH_CASE=target_materialization_benchmark
GUI_BENCH_OUTPUT=<report.json>` to reproduce one process; the archived collector
provides balanced independent runs. The earlier `materialized-snapshot` is the
diagnostic tight-bounds control, not the reported fixed-viewport comparison.

`make test-ui`, release GUI tests, benchmark tooling tests, build-system tests and
`./scripts/test-quiet ct` passed. No staging, service/game changes or live acceptance.

## Repair-Demand Integration Check

`Renderer::prepare_for` now separates current repair demand from retained cache
consumers, including the GUI and shared overlay callers. Tests preserve a shared
glow across 64 move-only updates without raster preparation, reactivate preparation
without a scene change, and release removed consumers. This is not surface-wide
relocate enablement or cost-aware demotion of intersecting viewport caches.

The maintained `gui-relics` workload was compared against the fixed-control binary
above: eight balanced independent processes per build, four threads, CPUs 4-7,
unchanged fixtures/toolchain. No builds ran during collection. Candidate binary:
`d0cb686a88a21123e03b19bf03eb781144f95fabb34cb1d3ddb618291a774724`.
The existing exploratory p95 tolerance is the larger of 5% or 0.1 ms.

Mean per-process p95 milliseconds:

| Scale | Input | Before | After | Verdict |
| --- | --- | ---: | ---: | --- |
| 100% | Wheel | 6.031 | 5.516 | Pass |
| 100% | Drag | 7.108 | 6.906 | Inconclusive |
| 125% | Wheel | 4.089 | 4.183 | Inconclusive |
| 125% | Drag | 5.178 | 5.569 | Inconclusive |
| 200% | Wheel | 1.573 | 1.725 | Inconclusive |
| 200% | Drag | 2.569 | 2.724 | Inconclusive |

An eight-pair identical-candidate-binary control was also inconclusive in four of
six cases. This workstation run cannot certify the overhead at that tolerance;
do not interpret it as a general improvement or regression-free acceptance.
No baseline was replaced. Raw runs, intervals, source patch/untracked archive,
binary and test logs are in `research/retinue/benchmarks/relocate-demand-20261103/`.
Shared SDK feature tests, release GUI tests, both companion presentation suites,
tool/documentation tests and Common Test passed. No application was staged/reloaded.
