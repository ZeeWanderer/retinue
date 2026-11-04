# Cost Ranking And Recording Overhead

The renderer now separates reusable work summaries, execution pricing and plan
selection. The objective is selecting the cheaper valid plan, not predicting
milliseconds. Relocation cancels common replay work and compares avoided work with
transfer traffic and setup, with a 10% margin. Validity proofs remain independent.

## Native Calibration

Intel i7-10700, CPUs 0-7, six fresh processes. Each process alternates replay and
independent-source copies for 112 samples/case, excluding the first 16. Five
workloads (fill, rounded fill, text, image, mixture), six sizes (64/128/192/384/512/
768 pixels) and 1/2/4/8 threads give 120 cases. Source images, glyphs and programs
are warm; materializations are disabled. No competing build or test ran during
measurement. Ordinary desktop activity was not controlled.

Simple primitives at 64/192/384/768 train the relative model. Mixtures and sizes
128/512 are held out. Candidate selection penalizes average and worst ranking
regret; a conservative screen rejects models choosing copies more than 10% slower
than replay in any training case. Held-out outcomes do not choose the parameters.
Regret is selected latency divided by the faster measured alternative, minus one;
case latency is the mean of process medians, not individual frames pooled together.

| Set | Cases | Mean Regret | Worst Regret |
| --- | ---: | ---: | ---: |
| Training | 64 | 1.04% | 44.06% |
| Held Out | 56 | 1.32% | 19.02% |

The training worst case is missed copy benefit on a large image, not a harmful copy.
Held-out mistakes include a 14.9% slower copy for one mixed workload. This is an
advisory ranking model, not a guarantee. Copy throughput does not scale linearly
with workers; replay pixel work is capped at four-way scaling in the model.
In-place copying retains a more conservative traffic score; it is not newly
calibrated by this independent-source experiment.

Solid/blended/sample pixel ratios were fixed at 1/2/1. The search varied command,
edge and glyph weights, copy traffic and worker scaling. Copy setup was tied to
twice the command weight; one region per case does not independently calibrate
fragmentation overhead. Related weights now live in `cost::WARM_NATIVE`; the
relocation margin remains local to the planner.

Coverage-weighted program summaries approximate partial repairs without walking
every command. They distinguish solid/blended/sample work, glyphs, edges and setup;
inset shadows account for the painter's outline bands. Cached programs, fragments
and layers substitute image work. Cache admission/cold preparation policy is not
calibrated here. No runtime calibration or production profiler was introduced.

## Complete Frames

The before/after GUI experiment uses the actual shell and Foundry with synthetic
cards and warmed captured artwork. Six alternating process pairs per configuration,
two and four workers, 100/125/200% scale, repair-only and independent-source modes.
Each process contributes 140 frames after 20 warmup frames; an initial process pair
is also excluded. Copy and repair coverage match in all 10,080 compared frames.

Mean process-median frame latency for the independent-source path, milliseconds:

| Workers | Scale | Before | Calibrated |
| ---: | ---: | ---: | ---: |
| 2 | 100% | 3.756 | 3.762 |
| 2 | 125% | 3.397 | 3.483 |
| 2 | 200% | 3.116 | 3.045 |
| 4 | 100% | 3.799 | 3.640 |
| 4 | 125% | 3.317 | 3.477 |
| 4 | 200% | 2.983 | 2.865 |

These are **inconclusive**, not accepted gains or regressions. A subsequent four-pair
identical-binary A/A control reported 3.540 versus 3.756 ms at two workers/100%, a
spurious 6.1% difference whose paired bootstrap interval excluded zero. Desktop
load was uncontrolled, including system-update activity during A/B. The control
demonstrates drift that the small paired interval does not remove. No baseline is
advanced; a quiet-system comparison remains necessary.

PoE headless smoke runs still favor relocation. At four workers, 1080p/100%
repair-only takes 0.346 ms versus relocation 0.206 ms; 1440p/150% takes 0.494 versus
0.319 ms. A separate two-worker run gives 0.437 versus 0.274 ms and 0.683 versus
0.357 ms respectively. These check the existing path, not a repeated before/after
or worker-count comparison, and do not measure game FPS.

## Recording Overhead

An optimized GUI benchmark with opt-in test phase metrics separates existing
profiling counters and materialization task durations. The table gives arithmetic
means of the same 140 warmed frames at four workers, not sums of phase medians.
It is a diagnostic run, not a performance acceptance comparison. Larger scale
shows fewer logical cards in the fixed-size viewport.

| Phase, ms | 100% | 125% | 200% |
| --- | ---: | ---: | ---: |
| Complete frame | 3.397 | 2.938 | 2.633 |
| Input dispatch, eight events | 0.052 | 0.049 | 0.048 |
| Shell and page recording | 1.334 | 1.027 | 0.706 |
| Foundry resolve, inside recording | 1.270 | 0.965 | 0.641 |
| SDK evaluation, inside resolve | 0.287 | 0.275 | 0.247 |
| SDK lowering, inside resolve | 0.048 | 0.044 | 0.039 |
| Scene update, inside resolve | 0.635 | 0.442 | 0.251 |
| Input-target sync, inside resolve | 0.040 | 0.024 | 0.009 |
| Foundry render preparation | 0.659 | 0.706 | 0.972 |
| Materialization, inside preparation | 0.499 | 0.600 | 0.915 |
| Foundry command submission | 0.027 | 0.024 | 0.023 |
| Surface snapshot | 0.053 | 0.037 | 0.021 |
| Repair planning | 0.048 | 0.034 | 0.021 |
| Native completion wait | 1.089 | 0.987 | 0.808 |

Nested rows must not be added again. Native work can execute before completion;
the completion wait is not all native rendering time. Materialization includes
allocation, rasterization and task overhead, not just bookkeeping. No compositor
presentation timing is measured by this headless workload.

Within the 100% scene update, reconciliation takes 0.087 ms, layout 0.048 ms and
projection 0.499 ms. Within projection, geometry/paint collection takes 0.325 ms
and frame replacement 0.080 ms. Resolve minus SDK update leaves approximately
0.260 ms of application-side work. Render preparation minus materialization is
approximately 0.159 ms. The earlier roughly 2 ms cannot be labeled entirely
library bookkeeping.

Prioritized follow-up:
1. Structural projection: changing the virtual row set invokes a full projected
   scene collection. Evaluate incremental keyed insertion/removal before changing
   layout math or input routing.
2. Foundry content construction: `sync_cards` regenerates all visible card
   descriptions on visible-range changes before equality can reuse results.
3. Materialization: distinguish necessary new rows from avoidable cache preparation
   with a controlled replay comparison. Do not remove caches on cost alone.

External `perf` records retain startup/JIT/image warming; raw symbol percentages
from that recording are not steady-state attribution. No overhead optimization is
claimed in this report; the new work is cost calibration and diagnosis.

## Correctness Checks

`make test-composition`, `make test-gui`, focused presentation tests for both
companions, `make test-tools`, `./scripts/test-quiet ct`, targeted Rust formatting
and diff checks passed. Rendering tests include scaled partial repairs and buffer
age transitions. No application was staged/reloaded for these measurements; live
interaction and quiet-system performance acceptance remain separate.

## Evidence

`research/retinue/benchmarks/cost-calibration-20261104/` retains raw CSVs, binary
snapshots, hashes, the ranking script, profiles and complete-frame comparisons.
Native data: `native-matrix/`; selection: `ranking-conservative.json` inside it.
GUI A/B: `calibrated-pairs/`; A/A: `identical-control/`; phase breakdown:
`phases-materialization.json`; external samples: `profile-run/`; reference survey:
`references/assessment.md`. These are dirty-worktree measurements. Saved binaries
and their hashes identify the measured revisions; final coefficient-table and
policy-function moves preserve their model. No historical baseline is advanced.
