# Host Input Ownership And Paint Deferral

This follows the [shared router refinement](2026-11-03-input-routing.md).
It fixes GUI host behavior, not the renderer or compositor.

## Findings

Three tests failed before the changes:

- Page navigation canceled child input but retained the shell's press flags.
- A new central modal could swallow a rail-owned release, leaving its capture active.
- Foundry hover/cursor queries reconciled paint-only changes before painting.

The shell now retains one press owner and routes release to it before considering
the current pane. Navigation cancels ownership; a blocking modal cancels a rail
gesture instead of activating the background. A new click works without an
intervening motion event, and navigation preserves stationary chrome hover.

Foundry separates layout inputs from hover/artwork inputs. Queries update stale
geometry but defer paint-only changes until preparation. A deterministic test
checks that repeated hover/cursor queries leave paint generation unchanged and
that painting consumes the changes. Existing drag coalescing remains intact;
this is not a new scrollbar optimization or a measured hover-latency claim.

## Regression Measurement

Eight fresh alternating process pairs after one discarded warm-up pair. Optimized
saved executables, i7-10700, CPUs4-7, four rendering threads, 2560x1440, warmed
fixture artwork. Each frame includes eight drag-over-grid pointer events, cursor
queries, preparation, repair planning and native completion. No concurrent builds
or tests; ordinary desktop activity was uncontrolled. No compositor/game FPS was
measured. Intervals bootstrap process pairs, not individual frames.

Mean of process medians, milliseconds:

| Scale | Mode | Before | After | Paired Delta Interval |
| --- | --- | ---: | ---: | --- |
| 100% | Relocate | 3.439 | 3.478 | -0.011 to +0.089 |
| 100% | Repair | 3.726 | 3.740 | -0.056 to +0.092 |
| 125% | Relocate | 3.090 | 3.097 | -0.055 to +0.076 |
| 125% | Repair | 3.161 | 3.244 | -0.053 to +0.233 |
| 200% | Relocate | 2.610 | 2.648 | -0.032 to +0.095 |
| 200% | Repair | 2.601 | 2.575 | -0.082 to +0.037 |

Every whole-frame and process-CPU interval crosses zero. This establishes neither
a throughput gain nor a regression. The 100% relocate preparation phase is slower:
1.921 to 1.963 ms, paired delta +0.022 to +0.068 ms. Keep that approximately 42-us
cost visible; its cause is not isolated, and it is not evidence of faster dragging.
All paired frames have identical copied and repaired pixel coverage.

## Verification

Final optimized GUI checks: 233 unit tests and seven isolated native-window tests
passed. The preceding debug suite passed 232 unit and seven native-window tests;
five focused navigation tests then covered the final stationary-hover adjustment.
Common Test, formatting and whitespace checks passed. Ignored benchmarks are not
counted as correctness tests. No new shipping instrumentation was added.

Evidence: `research/retinue/benchmarks/host-input-20261103/` contains the failing
reproduction, verification logs, candidate executable/source, paired samples,
script and summary. Its baseline is the adjacent
`independent-source-20261103/gui-input-snapshot/`. No application was staged or
reloaded, no game/service state changed, and live drag acceptance remains separate.
