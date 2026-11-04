# Cache-Aware Relocation Admission

Admission now distinguishes resident-image replay from the original commands that
produced those images. This corrects a missing input to the planner; it is not a
new drawing-throughput win in the measured Foundry and Evaluate workloads.

Whole-item caches, selected fragments and still-valid cached layers/rows/tiles
contribute their effective cost. Content, scale, clip and context changes fall back
to replay. Queries neither prepare rasters nor acquire ownership, refresh expiry
or change logical damage. GUI and both overlays provide this evidence.

Simple costs stay inline on existing paint records. Additional rectangles describe
fragment or layer coverage only. Immutable program summaries and existing indexed
dependencies avoid rescanning commands and unnecessary cache-owner lookups. No new
pixel buffer, execution backend or production profiling switch was added.

## Method

Same optimized binary per application, with benchmark-only
`RETINUE_BENCH_RAW_REPAIR_COST` selecting the previous model. Native relocation and
relocation-disabled controls run in each process. Eight fresh process pairs per
thread budget, alternating policy/case order, after one unmeasured warm-up round.

Intel i7-10700, affinity CPUs4-7, no concurrent build/test/benchmark jobs. Ordinary
desktop activity was not controlled. Foundry uses warmed captured artwork in the
existing synthetic inventory, eight pointer events per frame, 160 frames with the
first 20 excluded. Evaluate uses the existing 1080p/1440p drag workload and 160 warm
samples. These are headless application timings, not compositor or game FPS.

Values are means of process-level warm medians, in milliseconds. Paired intervals
resample processes, not individual frames. Phase medians are not additive.

## Foundry

1440p. The disabled column uses the new cost evidence but disables relocation.

| Threads | Scale | Raw-Cost Relocation | Effective-Cost Relocation | Disabled |
| --- | --- | ---: | ---: | ---: |
| 2 | 100% | 3.593 | 3.588 | 4.423 |
| 2 | 125% | 3.189 | 3.168 | 3.757 |
| 2 | 200% | 2.825 | 2.820 | 2.957 |
| 4 | 100% | 3.468 | 3.508 | 3.782 |
| 4 | 125% | 3.080 | 3.136 | 3.190 |
| 4 | 200% | 2.633 | 2.639 | 2.602 |

Copy and repair coverage is identical between cost models in every final pair.
The new evidence therefore does not change admission for this fixture. At four
threads/100%, its measured cost is +1.1%, with a paired interval of +0.007 to
+0.073 ms. Other model-comparison intervals cross zero; do not present their point
estimates as established gains or regressions.

At 100%, snapshot recording rises from about 41 to 66 microseconds, with another
3-4 microseconds in planning. CPU medians are 4.657/4.653 ms per frame at two threads
and 6.287/6.259 at four threads, raw/effective respectively. No aggregate CPU increase
is established by those point estimates. This does not measure peak-core contention.

Existing relocation remains about 19% faster than disabled at two threads/100%,
and 7% at four threads/100%. Four threads/200% remains near break-even: the observed
1.4% relocation loss has a -0.032 to +0.124 ms interval. Cache-aware inputs alone do
not make this a calibrated, execution-budget-aware latency model.

## Evaluate

1440p:

| Threads | Scale | Raw-Cost Relocation | Effective-Cost Relocation | Disabled |
| --- | --- | ---: | ---: | ---: |
| 2 | 100% | 0.215 | 0.215 | 0.366 |
| 2 | 150% | 0.321 | 0.340 | 0.627 |
| 4 | 100% | 0.206 | 0.199 | 0.312 |
| 4 | 150% | 0.277 | 0.286 | 0.457 |

Relocation coverage remains identical. All raw/effective relocation latency
intervals cross zero at both resolutions; the 150% point differences are not
established regressions. The disabled two-thread/150% control does show a small
positive interval, +0.001 to +0.058 ms, so this is not a claim of zero overhead in
every path. Production relocation retains a clear benefit: about 41%/46% at
two threads and 100%/150% scale.

## Rejected And Refined Candidates

The first GUI matrix had large timing drift, including supposedly faster results
with relocation disabled. It is retained as evidence, not accepted as a speedup.
Repeats exposed a 6.2% two-thread/100% regression in the first implementation.
Separate snapshot timing and existing draw counters localized a consistent
metadata cost; the full-frame 6.2% magnitude did not reproduce consistently.

An indexed cache lookup alone was insufficient. The final representation removes
the duplicate per-paint rectangle list. Snapshot time fell from roughly 94 to
67 microseconds at two threads/100% across the diagnostic/final batches. Those
cross-binary values are descriptive, not a controlled causal effect size. The
final same-binary table above is the acceptance comparison. No copy-cost constants
were tuned to make these particular scales pass.

## Correctness And Evidence

Regression cases cover cached versus raw admission, unchanged damage evidence,
read-only residency, program/scale changes, reordering, frame identity, fragment
clipping, translated layers, newly exposed row pixels and nested surface offsets.
Shared native oracles cover rotating targets and partial repair. App oracles cover
GUI scrolling, Evaluate drag/hover, and Warframe tooltip movement/removal.

Final checks passed: shared renderer/SDK/macro suites, 31 GUI shell tests,
75 PoE presentation tests, 12 Warframe renderer tests, and Common Test. Ignored
benchmarks and live checks are not counted as correctness passes.

Evidence root: `research/retinue/benchmarks/independent-source-20261103/`.
Final snapshots/matrices: `gui-cost-inline-*` and `poe-cost-inline-*`. Each includes
binary/source provenance, raw samples, run manifests and paired summaries.
Earlier `effective-cost`, `cost-phases` and `cost-indexed` runs retain the investigation.
`measure-cost.py` and `analyze-cost.py` reproduce the paired comparison.

No apps were staged or reloaded. No game/service state was changed. Live game FPS,
execution-budget calibration and the separate input/hit-testing audit remain open.
