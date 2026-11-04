# Projection And Card Preparation

Follow-up to the [cost calibration and overhead profile](2026-11-04-cost-calibration.md).
This pass removes redundant work, without adding a cache or changing the calibrated
cost coefficients.

## Changes

- Scene projection passes the parent's geometry index through traversal instead of
  chasing parent nodes and looking up their keys again for every child.
- Foundry retains common visible card layouts and draw descriptions across virtual
  row changes. Model, grid and scale changes still reconstruct them; image updates
  refresh artwork. New rows, hover and actions remain current.
- PaintFrame owns scene-patch damage. Aggregate subtree geometry no longer adds
  duplicate enclosing damage rectangles before repair planning. Exact regions,
  buffer history and relocation validity remain separate from cost ranking.

Structural row insertion still collects the whole projected scene. This is not an
incremental-topology implementation. The three changes are measured together;
their individual contributions have not been isolated.

## Measurements

Intel i7-10700, affinity CPUs 4-7, two/four render threads, optimized GUI binaries.
Actual shell and Foundry, synthetic cards with warmed captured artwork. Eight
alternating process pairs; each case has 140 frames after 20 warmup frames, with
an initial process pair excluded. Both repair-only and independent-source modes
run at 100/125/200%. No competing builds or tests; ordinary desktop load remains
uncontrolled. All 13,440 paired frames have identical copy and repair areas.

Independent-source path with three target buffers, mean of process medians in ms:

| Threads | Scale | Recording Before | Recording After | Frame Before | Frame After |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 100% | 1.833 | 1.468 | 3.988 | 3.753 |
| 2 | 125% | 1.297 | 1.143 | 3.556 | 3.455 |
| 2 | 200% | 0.779 | 0.727 | 3.199 | 3.136 |
| 4 | 100% | 1.816 | 1.468 | 3.780 | 3.541 |
| 4 | 125% | 1.420 | 1.216 | 3.696 | 3.542 |
| 4 | 200% | 0.798 | 0.775 | 3.048 | 3.048 |

Recording includes application description construction and SDK evaluation/layout/
projection; it is not entirely library bookkeeping. These phase medians must not
be added together to estimate a frame.

**Recording improves convincingly at 100%; whole-frame gains remain inconclusive.**
At 100%, recording falls 18-20% across both modes/thread counts. A subsequent
four-pair identical-binary control varies only 0-2.3% in that phase at 100%, but
reports false whole-frame gains of 4.5% at two threads and 4.7% at four threads
for independent-source rendering. Both false gains have paired bootstrap intervals
excluding zero. At 125%, control recording drift reaches 7.4%. Small sample
intervals do not remove desktop/system drift; no timing baseline is advanced.

The damage fix preserves holes in a dedicated sparse-subtree test. This scrolling
workload's copy/repair areas do not change, so its recording benefit is not from
painting fewer pixels. Preparation-task and replay counters can still vary slightly
with asynchronous scheduling. No compositor/game FPS measurement is claimed.

## Correctness And Native Clipping

Shared renderer/SDK and GUI tests pass, including fresh-versus-retained Foundry
rendering across virtual rows, snapshot changes, delayed artwork and 100/125/200%
scale. The native desktop suite, both companion presentation suites (41 Warframe,
75 PoE tests), Common Test and workspace formatting check also pass.

Tighter damage exposed two RGB bytes differing by one in a recycled-buffer test.
A pure Painter reproduction, without UI projection or the optimizer, reproduces
the difference with one/two/four threads when a rounded edge meets the clip.
Expanding that clip by one pixel produces exact equality. Blend2D's edge builder
uses different clipped/unclipped curve-flattening paths.

The native regression and that scene test therefore allow RGB delta one while
requiring exact alpha. Geometry, sparse coverage and other pixel tests remain
strict. No production damage padding or vendor change was added.

## Remaining Work

1. Incremental keyed projection is covered by the
   [subsequent measurement](2026-11-04-incremental-projection.md), including
   clipping, painter order, render-context changes and reparenting.
2. Compare materialization preparation against replay before changing cache policy;
   it includes real rasterization/allocation, not only library overhead.
3. Obtain repeatable whole-frame measurements before accepting a baseline.

## Evidence

`research/retinue/benchmarks/projection-overhead-20261104/` retains before/after
binary snapshots, alternating runs in `paired/`, same-binary runs in `control/`,
bounded analysis, test logs, native clipping reproduction and source annotations.
Binary SHA-256 prefixes: before `2b9b3f156a4bc796`, after `50b560c59a807fe58`.
`analyze.py` retains mean and median phase results plus paired bootstrap intervals;
raw samples remain available. These are dirty-worktree measurements, not a release
or live acceptance check. The final delayed-artwork test and module-list formatting
do not change the measured production behavior. No app was staged/reloaded and no
commit was made.
