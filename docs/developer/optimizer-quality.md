# Optimizer Quality

Timing a production page tells us whether it improved. It does not tell us whether
the optimizer chose well. `render-quality` executes the same synthetic input trace
under multiple legal policies, checks the resulting pixels, then measures each
alternative through native completion.

## Run

```bash
make bench BENCH_SUITE=render-quality BENCH_THREADS=4 BENCH_SAMPLES=10 \
  BENCH_RUNS=7 BENCH_OUTPUT=research/retinue/benchmarks/quality
python3 tools/benchmarks/quality.py \
  research/retinue/benchmarks/quality/measurements/report.json \
  --output research/retinue/benchmarks/quality/analysis.json
```

`BENCH_CASES=decorated-scroll,short-lived` selects scenarios. `BENCH_SAMPLES` means
complete traces per policy, not frames. The existing runner preserves binary,
source and environment provenance and repeats in fresh processes. Do not compile
or run other tests while collecting timings. This does not launch or change apps.

No timing tolerance is implicit. Supply reviewed `--relative` and/or `--absolute`
(milliseconds) to obtain an advisory verdict. Without them results are
`unreviewed`. Fewer than seven independent processes remain `inconclusive` even
when one process contains thousands of frames. `--metric frame_p95_ms` evaluates
frame tails instead of total-trace means; CPU and memory remain separate metrics.
`--max-raster-bytes` optionally restricts alternatives by the maximum retained
raster bytes sampled after completed frames. It does not measure transient
allocation peaks or process RSS. Each scenario also enforces its residency budget.

## Coverage

Four execution configurations combine with full repaint, damage repair and
automatic relocation:

- Original-command replay without rasters.
- Compiled-command replay without rasters.
- Compiled replay with automatic fragments and eligible layer promotion.
- Compiled replay with those optimizations and preferred item rasters, the tested
  default configuration.

These twelve alternatives permit pass-group ablation and interaction checks. They
are not twelve shipping modes. All harness code is under `cfg(test)`; existing
renderer controls are used without adding production tracing or policy switches.
Original replay still constructs validated source programs, so this is execution
ablation, not a claim to skip all compiler construction work.

Scenarios vary cheap versus expensive drawing, visible cardinality independently
of pixel scale, translucent overlap, stationary sparse changes, late artwork,
reversal, removal, short lifetimes, two/three acquired buffers, stable rows and
memory pressure. Ordinary Cargo tests check semantic pass properties and complete
trace equivalence. Seeded inputs and trace reduction produce bounded, reproducible
failures. Existing focused per-pass tests remain with their implementations.

The raw full-paint oracle ignores optimized playback and cached surfaces. Solid
scenes are exact. Native curve clipping may differ by one RGB level; intermediate
raster compositing allows the existing three-level RGB tolerance. Alpha is exact.
Independent solid-color/placement assertions guard against two paths agreeing on
the same error. Existing exact same-plan damage tests remain necessary.

## Read Results

Per-run `quality.json` contains:

- Scenario parameters, explicit policy alternatives and pixel-oracle tolerances.
- Whole-trace samples including cold renderer/program/surface setup and all frames.
- Frame update, snapshot, preparation, planning, submission and native completion
  timings; copied/repaired pixels, command work and live raster bytes.
- Separate, untimed validation traces with frame-indexed source changes, semantic
  execution plans, materialization outcomes and effective replay-work terms.

The `passed` remarks indicate observed transformations. `analysis` records describe
the result without inventing a reason for a missed opportunity. They do not expose
every internal rejection branch. Diagnostics and pixel comparisons run before,
not inside, timed traces. Native pipelines/fonts are warmed by validation, while
each timed trace starts with fresh renderer residency and presentation buffers.
The synthetic runner uses standalone preparation scheduling: application-host
per-frame preparation budgets and background analysis are not modeled here.

`quality.py` compares the default against each measured policy, using independent
process summaries as the statistical unit. Paired intervals are adjusted across
alternatives within each scenario. `near-best-tested` means no measured eligible
alternative beats the reviewed tolerance within these intervals. It is not proof
of a global optimum. The observed best and regret are exploratory because the
same data selects the best policy; preserve held-out cases when tuning costs.

The measured space is fixed policies over complete traces, not all possible
per-frame cache schedules, repair partitions or transformations. A compiler-style
pass/decision bisection tool, exhaustive tiny-scene scheduling oracle and production
SDK/page traces are further extensions. The harness does not measure SDK layout,
compositor presentation or game FPS. Use existing application suites and external
profiles for those costs.

## Method

The separation of small pass regressions from whole-workload measurements follows
[LLVM's testing model](https://llvm.org/docs/TestingGuide.html). Structured outcomes
take inspiration from [optimization remarks](https://llvm.org/docs/Remarks.html);
[optimization bisection](https://llvm.org/docs/OptBisect.html) is a useful next
diagnostic, not something this harness already implements. The measured
alternative comparison is our bounded oracle, independent of the cost model's
numerical prediction.
