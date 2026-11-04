# Cold Raster Admission

Follow-up to [materialization versus replay](2026-11-04-materialization.md).
**Production admission remains unchanged.** Two demand-aware prototypes improve
scrolling but change the broad-repaint latency distribution. Neither is accepted
as an unconditional replacement under the current no-fallback-regression objective.

## Candidates

Both defer cold whole-instance rasters when their preparation work exceeds the
work justified by current repair demand. Equal programs aggregate demand across
consumers; actual direct painting accumulates bounded estimated savings. Ready or
scheduled child fragments count as blits. Existing rasters and repair donors keep
their previous policy. Deferred admission does not request another frame itself;
oversized allocations do not enter the demand retry queue.

The first candidate uses current demand and past replay only. The second also
interprets `Prefer` as anticipating one additional equivalent repaint, net of its
blit cost. Neither uses GUI-scale thresholds or page-specific scroll flags.
These are relative work heuristics, not calibrated cold-allocation timing models.

## Measurements

Actual shell and Foundry, fixed 2560x1440 framebuffer, desktop scale 1.0, warmed
captured artwork, Intel i7-10700, CPUs 4-7. Each candidate has six balanced fresh-
process A/B repetitions at two/four workers and 100/120/150% GUI scale. An initial
repetition and the first 20 of each case's 160 frames are excluded. Both repair-only
and independent-source relocation execute the same input trace. No builds/tests
run alongside measurement. Copy/repair coverage matches in all 10,080 paired
frames per experiment.

An earlier batch was stopped while Steam shader workers saturated the CPU. Even
the unchanged baseline rose to 9-15 ms. That incomplete batch is invalid and is
not included below. The user subsequently stopped Steam; ordinary desktop load
remains uncontrolled, so these are workstation results, not CI timing gates.

For the second candidate, arithmetic mean frame times in milliseconds:

| Workers | Scale | Relocate Before | Relocate After | Repair Before | Repair After | Repair Median Change |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 100% | 3.139 | 2.870 | 3.821 | 3.833 | +10.3% |
| 2 | 120% | 2.781 | 2.568 | 3.323 | 3.343 | -1.1% |
| 2 | 150% | 2.712 | 2.513 | 3.131 | 3.167 | -5.2% |
| 4 | 100% | 3.031 | 2.757 | 3.250 | 3.250 | +12.8% |
| 4 | 120% | 2.699 | 2.417 | 2.806 | 2.861 | -0.7% |
| 4 | 150% | 2.605 | 2.362 | 2.700 | 2.669 | -7.2% |

The broad-repair median slowdown is not a 10-13% increase in total work. At four
workers/100%, the mean is essentially unchanged while mean process p95 improves
from 5.006 to 4.246 ms. At 120%, p95 improves from 4.185 to 3.737 ms while mean
cost increases about 0.055 ms. Preparation moves between frames; median, mean and
tail latency must remain separate. At 100%, repair median is 2.962 -> 3.340 ms;
its paired bootstrap interval for the difference is +0.302..+0.449 ms.

The first candidate similarly improves 120% relocation mean by 8-12%, but has a
15% repair-median slowdown at 100%. Its repair means change by roughly -2..+3%.
Adding one predicted reuse reduces some costs but does not resolve the common-
case tradeoff. There is a plausible tail-latency benefit, not a universal win.
No baseline is advanced. The wider 90-130% sweep is deferred because acceptance
already fails at 100%; this is not evidence for those unmeasured scales.

## Correctness And Next Work

Five focused prototype tests cover narrow-to-broad demand without a scene change,
actual replay versus idle preparation, shared consumers, sparse damage holes and
impossible allocation retries. Renderer/SDK, GUI and both companion suites pass.
The prototype policy and its tests are archived rather than retained as another
shipping renderer mode.

One test correction remains: the recycled-buffer oracle now fully paints the
**same prepared execution plan**. Previously it requested fresh full-frame
admission, potentially changing representation mid-comparison. That produced an
observed one-level RGB difference at a translucent card edge. Repair equality is
still exact; independent fresh-tree and cached-versus-direct tests retain their
existing assertions and tolerances.

Next admission work needs measured decision quality over complete traces, including
preparation placement and remaining reuse, not just single-frame estimates. Broad
versus narrow repairs, changing visible cardinality, fragment substitution and
cache-state transitions must be separate synthetic cases. Validate against measured
legal alternatives before introducing a new default. Scale should affect actual
pixel/glyph work and instance multiplicity, not select a hardcoded policy.

## Evidence

`research/retinue/benchmarks/admission-20261104/` retains saved binaries, source
provenance, raw samples and analyses. `pairs/` is the history-only policy;
`lookahead-pairs/` includes one predicted reuse. `contended-pairs/` is invalid.
`prototype/` preserves the later implementation and tests. No application was
staged or reloaded, no commit was made, and no production instrumentation was added.
