# Automatic Reusable Drawing

Paint compilation now finds expensive primitives and dense structural blocks
without authored cache boundaries. Repeated consumers or actual repaint history
admit them through the existing preparation graph and residency store. The GUI
component glow no longer names its fragment or supplies a cache hint.

Discovery preserves original commands and weakly interns equivalent candidates
across independent views. Cached children discount parent cost; parent preparation
can consume those children and release redundant selected storage. Relocation does
not invalidate a still-needed reusable source or count as another repaint.

This is bounded structural discovery, not arbitrary command-subsequence matching
or a calibrated native cost model. Text stays in the existing glyph-mask path;
another colored-alpha raster introduced quantization differences in GUI oracles.

## Method

Intel i7-10700, CPUs 4-7, optimized builds. Eight fresh process pairs, alternating
before/after order, plus a discarded warm-up pair for the GUI. Values are means
of warm process medians. Paired 95% intervals resample processes, not frames.
No competing build, test or Steam shader-compilation jobs in the accepted runs.
Ordinary desktop activity was not controlled. These are headless timings, not
compositor or game-FPS measurements.

GUI: saved pre-change binary versus corrected automatic-discovery binary; 1440p,
four threads, warmed artwork, eight pointer events/frame, 160 frames/case with
20 excluded. The previous binary already explicitly cached the glow. This measures
the change as a whole, not an isolated speedup attributable to discovery alone.

## Foundry

Frame latency in milliseconds:

| Scale | Previous Relocate | Automatic Relocate | Previous Repair | Automatic Repair |
| --- | ---: | ---: | ---: | ---: |
| 100% | 3.420 | 3.235 | 3.726 | 3.526 |
| 125% | 3.047 | 2.956 | 3.140 | 3.074 |
| 200% | 2.575 | 2.567 | 2.572 | 2.582 |

Relocate improves 5.4% at 100% (paired difference -0.268 to -0.105 ms), and
3.0% at 125% (-0.132 to -0.058 ms). Repair-only improves 5.4% and 2.1% respectively.
Both 200% intervals cross zero: no established gain or regression there.
Per-frame copy and repair coverage matches in all 6,720 compared samples.

At 100%, preparation falls from 1.925 to 1.743 ms with relocation. Process CPU
time falls from 6.082 to 5.935 ms/frame. This is cumulative CPU time across threads,
not a utilization percentage or peak-core measurement. The 125% CPU improvement
is also supported; 200% remains inconclusive.

## Isolated Reuse

Same binary, materializations enabled versus disabled. Unhinted drawings retain
their programs while moving. Full repair isolates raster reuse from relocation;
layout is outside timing. Each process alternates modes for 240 samples/case,
excluding the first 16. Timings include preparation, submission and completion.

72 independently identified cards with equivalent glow drawing, milliseconds:

| Scale | Direct, 2 Threads | Automatic, 2 Threads | Direct, 4 Threads | Automatic, 4 Threads |
| --- | ---: | ---: | ---: | ---: |
| 100% | 1.982 | 0.212 | 1.106 | 0.168 |
| 125% | 2.436 | 0.246 | 1.484 | 0.226 |
| 200% | 5.760 | 0.480 | 3.783 | 0.364 |

All paired latency intervals favor automatic reuse. One moving card also improves:
53.5 to 27.1 microseconds at 100%/four threads, 45.5 to 16.9 at 125%, and 102.6 to
25.6 at 200%. Promotion follows three actual repaint observations, not idle time.

Both one-card and 72-card cases select one raster: 6,400/10,000/25,600 pixel bytes
at 100%/125%/200%. These figures exclude metadata, source assets and native scratch;
they are not RSS measurements. At 100%/72 cards/four threads, cumulative CPU time
falls from 3.670 to 0.347 ms/frame.

## Rejected Runs And Verification

The first implementation lost discovered children at authored boundaries and
coupled optimized-stream playback to command culling. Materialization also replayed
raw source. GUI submission regressed by up to 50%. Separate compilation modes,
independent compiled/cull flags, and compiled materialization playback fix those
paths; regression tests assert actual child-image use, not just scheduled tasks.

A later run collided with Steam shader compilation (about 77% nice CPU and 2%
idle). Timed preparation deadlines even changed repair coverage in 124 samples.
Those results remain recorded but are not acceptance evidence. The final quiet
run retains all samples without outlier trimming and has matching coverage.

Passed: 235 renderer tests, shared SDK/macro feature matrix, 233 GUI unit and
seven native-window tests, 75 PoE and 42 Warframe presentation tests, Common Test,
27 benchmark-tool tests, formatting and diff checks. Oracles cover fractional
coordinates, clipping, scaling, reuse across independent views, resource versions,
parent takeover, relocation-only frames, removal and weak program lifetime.

Evidence: `research/retinue/benchmarks/automatic-reuse-20261104/`. Accepted runs:
`paired-quiet/` and `reuse-quiet/`; GUI snapshots `before/` and `fixed/`; native
binary `reuse-benchmark` with hash/source archives. `measure.py` reproduces the
comparisons. Initial and loaded-system runs remain alongside them. Worktree was
dirty; no historical baseline was advanced. No apps were staged or reloaded.

Still deferred: calibrated native cost thresholds, general subsequence discovery,
text-raster equivalence, live game FPS, and the separate host-boundary/Core review.
