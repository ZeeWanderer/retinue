# Accepted Measurements

Keep selected compact reports here, not every run. Measure relevant changes and
release checkpoints. Raw frame samples, traces and executable snapshots stay in
ignored `research/` or a separate artifact store.

Use the [follow-up list](follow-ups.md) for unresolved findings, next actions and
closure criteria. The [last-commit comparison](2026-11-04-checkpoint.md) records
the latest matched GUI results and locates the older direct-renderer timings.

Reviewed references: [2026-11-03 rendering matrix](2026-11-03-rendering.md).
These are workstation measurements, not hard CI timing gates.
The [native relocation report](2026-11-03-native-relocate.md) records the integrated
host path, thread/scale comparisons, CPU tradeoffs and memory costs.
The [copy-executor comparison](2026-11-03-copy-executors.md) corrects its GUI workload
and compares identical native, serial and row-parallel copies.
The [reference-architecture study](2026-11-03-reference-scroll.md) compares exact
no-relocation controls with native Qt/Blend2D scroll and retained-raster prototypes.
The [effective-cost follow-up](2026-11-03-effective-cost.md) records cache-aware
admission, its metadata overhead and the final same-binary comparisons.
The [input-routing follow-up](2026-11-03-input-routing.md) separates router costs
from end-to-end Foundry dragging and preserves the remaining audit scope.
The [host-input follow-up](2026-11-04-host-input.md) fixes pane ownership and
paint-only hover work, with paired rendering regression measurements.
The [automatic-reuse follow-up](2026-11-04-automatic-reuse.md) measures unhinted
discovery, shared raster admission and full-GUI regression checks.
The [cost-calibration follow-up](2026-11-04-cost-calibration.md) separates work
description from decision policy and measures ranking regret on held-out cases.
The [projection-overhead follow-up](2026-11-04-projection-overhead.md) removes
redundant recording work and coarse scene damage, with an identical-binary control.
The [incremental-projection follow-up](2026-11-04-incremental-projection.md) reuses
clean projected subtrees during structural changes and narrows the GUI scale matrix.
The [materialization study](2026-11-04-materialization.md) separates cache drawing
from UI bookkeeping and rejects cache removals that regress full repaint.
The [cold-admission study](2026-11-04-admission.md) evaluates demand and reuse
heuristics, separating median, mean and tail-latency tradeoffs.

```bash
python3 tools/benchmarks/history.py archive path/to/report.json --note 'Reason for retaining this run'
python3 tools/benchmarks/history.py baseline MEASUREMENT_ID --reason 'Reason for accepting this baseline'
python3 tools/benchmarks/history.py compare path/to/candidate.json --policy path/to/policy.toml
```

Reports are immutable. Series match workload, fixtures, cases, machine and build
configuration; the source commit is provenance, not the series identity.
`baselines.json` changes only through explicit selection. Review that diff with
the report; Git preserves prior baseline decisions. Do not advance baselines
automatically or accept a sequence of small regressions unnoticed.

Dirty-worktree runs can be archived but cannot be selected as baselines. Select
only clean, representative measurements after reviewing their repeatability.
