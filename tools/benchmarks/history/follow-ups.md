# Rendering Follow-Ups

One status list for unresolved rendering findings. Reports retain evidence; this
list retains decisions and closure criteria. Keep IDs when adding measurements.
Do not label a noisy estimate a regression or close an issue merely because a
build, pixel test or unrelated benchmark passes.

## RENDER-01: Foundry Full-Repaint Cost

**Status:** suspected, unconfirmed. **Priority:** next reproduction/profile target.

Last commit to current: full-window median estimates rise 4-6% at 100/125%, but
intervals cross zero and identical-binary controls drift. See the
[checkpoint comparison](2026-11-04-checkpoint.md) for records and reproduction.
This is the original fixture/full-repaint workload, not the artwork-heavy
relocation benchmark. No cause is established.

**Next:** repeat saved A/B and A/A workloads under quieter conditions; attribute
any repeatable increase to recording, preparation, submission or native completion
before changing policy. External profiles stay separate from uninstrumented runs.

**Close when:** repeatable evidence isolates and removes a source regression with
pixel/workload parity, or supports an explicitly reviewed acceptance decision.
Preserve contrary/control results; do not widen limits to pass.

## RENDER-02: Non-Paint Library Overhead

**Status:** measured residual cost, not wholly an introduced regression.

The [materialization breakdown](2026-11-04-materialization.md#ui-framework-cost)
attributes about 0.45 ms at 120% to SDK structure outside view evaluation, plus
0.32 ms mixing framework traversal and application bodies. Projection and
evaluation remain targets. Cache preparation also contains real rasterization;
do not count it all as bookkeeping or sum overlapping spans.

**Next:** isolate remaining costs after RENDER-01; retain held-out page/overlay
regression checks. No wholesale layout rewrite without a measured cause.

**Close when:** justified fixes have same-trace uninstrumented evidence and
correctness coverage, and remaining overhead is explicitly accepted. A faster
paint-only microbenchmark is not closure.

## RENDER-03: Cache Admission And Optimizer Quality

**Status:** measurement pending; production admission remains unchanged.

[Demand-aware candidates](2026-11-04-admission.md) were rejected for mixed
median/mean/tail tradeoffs. The [quality harness](../../../docs/developer/optimizer-quality.md)
passes pixel/trace checks, but its first one-process run overlapped an unrelated
CUDA build and cannot rank policies. Evidence:
`research/retinue/benchmarks/quality-20261104/`.

**Next:** repeated quiet full-trace measurements, then test admission against
measured legal alternatives, including cold preparation and future reuse.
Application-host scheduling and per-frame policy search are not yet modeled by
the standalone harness. Expand coverage where a decision needs it.

**Close when:** a reviewed policy wins on held-out traces without unacceptable
tail/resource tradeoffs, or evidence justifies retaining the current policy.
Do not infer cost from GUI scale or treat the cost model as its own oracle.

## RENDER-04: PoE Dragging And Game FPS

**Status:** deferred by user. Do not resume live captures without coordination.

Bounded compositor damage improved dragging, but the user still observed two
large FPS dips. The capture lacked frame timestamps. Stable focus and cgroup
settings do not identify their cause; headless timings cannot close this.
Evidence: `research/retinue/poe-drag-findings-20261102.md` and adjacent captures.

**Next:** synchronized game/compositor frame timing and companion workload during
the same idle/drag/idle sequence, when live investigation is resumed.

**Close when:** the dips are reproduced, attributed and fixed or explicitly
accepted, with a comparable live trace. Do not infer game FPS from copied pixels.

## Acceptance Boundary

Correctness checks pass, but these entries do not amount to full performance
acceptance. The [latest comparison](2026-11-04-checkpoint.md) covers three unchanged
GUI suites; changed drag/WF workloads and newer relocation traces lack a directly
matching clean-commit counterpart. Re-run affected overlays for any subsequent
shared-renderer fix. No automatic baseline advancement.
