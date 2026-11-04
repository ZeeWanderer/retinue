# Incremental Input Routing

The target list was already flat and paint-ordered. The avoidable work was after
lookup: duplicate hit tests, rebuilding unchanged hover ancestry, cloning capture
maps, and publishing every target's observed state for every pointer event.

The router now retains each pointer's current hit and publishes changed interaction
states only. Same-position host queries share the result. Layout changes recheck
stationary pointers and ancestry; target replacement fully reconciles observed
state. Capture, hover, focus and event propagation retain separate semantics.
No spatial index, shipping instrumentation or frame-rate-dependent input filtering
was added.

## Method

Saved optimized before/after executables, eight fresh alternating process pairs,
one unmeasured warm-up pair. Input-only tests use CPU4; GUI tests CPUs4-7 with four
rendering threads, on an i7-10700. No concurrent builds/tests; ordinary desktop
activity was not controlled. Intervals bootstrap process pairs, not events/frames.

The public-API input experiment tests 64/512/2048 synthetic targets with and without
observed interaction. Each process measures 16 batches of 4096 events per case,
after a discarded batch. These target counts are test loads, not claims about the
number of visible controls in each application.

## Input Cost

Mean of process medians, nanoseconds per event, with 512 targets:

| Case | Unobserved Before | After | Observed Before | After |
| --- | ---: | ---: | ---: | ---: |
| Stationary motion | 890 | 43 | 3962 | 44 |
| Moving inside one target | 893 | 319 | 3981 | 329 |
| Moving between targets | 932 | 432 | 4098 | 480 |
| Captured drag | 970 | 439 | 4403 | 491 |
| Motion with periodic reflow | 1027 | 540 | 4341 | 684 |

All 30 input-only case intervals favor the candidate. At 2048 targets, crossing
targets falls from 4.56 to 1.51 microseconds without observed state and from 19.45
to 1.60 with it. This is routing cost, not a corresponding gain in application FPS.

## Foundry Trace

1440p, warmed artwork, eight drag-over-grid pointer events per frame. Same renderer,
cache-aware admission and repair/copy coverage in both builds. Mean process medians:

| Scale | Mode | Before, ms | After, ms | Paired Delta Interval, ms |
| --- | --- | ---: | ---: | --- |
| 100% | Relocate | 3.530 | 3.472 | -0.179 to +0.061 |
| 100% | Repair | 3.750 | 3.702 | -0.076 to -0.014 |
| 125% | Relocate | 3.086 | 3.037 | -0.079 to -0.017 |
| 125% | Repair | 3.238 | 3.193 | -0.160 to +0.059 |
| 200% | Relocate | 2.604 | 2.578 | -0.052 to -0.002 |
| 200% | Repair | 2.594 | 2.567 | -0.087 to +0.026 |

Whole-frame gains are small; three comparisons remain inconclusive. Preparation
improves by roughly 16-71 microseconds, with the 125% relocation interval crossing
zero. CPU comparisons establish small reductions in two repair cases; the others
are inconclusive. No interval establishes a regression. These are headless timings,
not compositor/game FPS or proof that all host-side input costs are resolved.

## Evidence And Scope

Ignored evidence: `research/retinue/benchmarks/input-dispatch-20261103/` contains
input executables/source, paired raw batches, scripts, provenance and GUI reports.
GUI snapshots are `gui-cost-inline-snapshot` and `gui-input-snapshot` under the
adjacent `independent-source-20261103/` directory. Copy and repair coverage match
exactly in every paired frame.

Regression tests cover bounded status publication, same-position hits, stationary
reflow, reparenting, observed-state replacement, multi-pointer hover/capture,
capture transfer, focus visibility and window focus loss. Test counters are absent
from benchmark libraries and application builds.

Final checks passed: shared renderer/SDK/macros, 228 GUI unit tests and seven
isolated native-window tests, 75 PoE presentation tests, 12 Warframe renderer tests,
Common Test, tooling, formatting and whitespace checks. Ignored benchmarks/private
capture checks are not counted as correctness passes. Catalog validation now scans
integration-test directories as well as implementation tests.

This completes the router refinement, not the whole input audit. Host-side layout
preparation, nested page/rail event ownership and live drag behavior remain separate
checks. No applications were staged/reloaded and no game/service state was changed.
