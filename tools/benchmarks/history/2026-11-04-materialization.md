# Materialization Versus Replay

Follow-up to [incremental projection](2026-11-04-incremental-projection.md).
**Retain the production cache policy.** Removing full-card raster requests helps
relocation but substantially regresses full repaint. The next optimization needs
paint-demand-aware admission, not a blanket cache removal.

## Experiment

Actual shell and Foundry, synthetic cards with warmed captured artwork, fixed
2560x1440 framebuffer and desktop scale 1.0. Intel i7-10700, CPUs 4-7, two/four
render threads. Six repetitions in fresh processes with rotated policy order,
excluding an initial repetition; each case measures 140 frames after 20 warmups.
Scales are 100%, 120% and a 150% reference. No competing builds/tests; normal
desktop load remains uncontrolled.

Three policies run in the same saved binary:

- `explicit`: existing whole-background/artwork requests plus automatic fragments.
- `fragments`: no whole-instance rasters; retain automatic reusable drawing.
- `none`: no optional Foundry rasters. Asset/glyph caches and command optimization
  remain enabled, as do the shell's independent renderer and caches.

Both relocation and repair-only paths run the same scrollbar trace. All compared
frames have identical copied/repaired areas. Cache state is sampled after the frame
timer stops. Residency bytes below are Foundry materializations, not process RSS.

## Results

Mean process-median frame latency in milliseconds:

| Threads | Scale | Path | Explicit | Fragments | None |
| ---: | ---: | --- | ---: | ---: | ---: |
| 2 | 100% | Relocate | 3.331 | 3.026 | 3.621 |
| 2 | 100% | Repair | 3.993 | 5.485 | 12.347 |
| 2 | 120% | Relocate | 3.030 | 2.703 | 3.314 |
| 2 | 120% | Repair | 3.675 | 4.260 | 9.658 |
| 4 | 100% | Relocate | 3.194 | 2.797 | 3.015 |
| 4 | 100% | Repair | 3.335 | 3.967 | 7.825 |
| 4 | 120% | Relocate | 3.040 | 2.367 | 2.658 |
| 4 | 120% | Repair | 3.210 | 3.080 | 5.999 |

At 150%, fragments-only improves relocation 23-28%, while its repair-only outcome
depends on thread count. At 100%, repair-only regresses 19-37%. No-cache full repair
is consistently much worse. Shared fragments remain valuable.

At 120%, explicit preparation spends about 0.59 ms/frame drawing new rasters;
fragments-only spends essentially zero after warmup in this trace. Mean resident
rasters fall from 2.73 MiB to 0.24 MiB. This avoids work on narrow scroll-edge
repairs, but replaying uncached artwork repeatedly becomes expensive on broad
repairs. The preparation duration was real drawing/allocation, not entirely SDK
bookkeeping.

A second six-pair experiment retains background rasters and removes only artwork
rasters. Existing strict pixel tests pass, unlike the broader removal. Relocation
improves 4-24%, but repair-only still regresses up to 40%. Its 10,080 paired frames
also match copy/repair coverage. This smaller candidate is rejected too; both
production hints are restored. No timing baseline is advanced.

## UI Framework Cost

An optimized test build with phase reporting measures the restored production
policy separately. Four rendering threads, CPUs 4-7, the same warmed trace; the
following values are arithmetic means of the same 140 frames at 120%, not sums
of independent phase medians. The complete frame averages 2.985 ms.

| Nonoverlapping Wall-Time Component | ms |
| --- | ---: |
| Foundry declarative update pipeline | 0.765 |
| Other UI description/update work | 0.138 |
| Pointer input, surface snapshot and repair planning | 0.139 |
| Foundry renderer preparation excluding raster drawing | 0.121 |
| Drawing/preparing cached rasters | 0.574 |
| Remaining native completion wait | 1.142 |
| Other context setup, submission and bookkeeping | 0.106 |

Inside the declarative update pipeline:

| Nested Component | ms |
| --- | ---: |
| View evaluation, including application callbacks and observation | 0.319 |
| Lowering to layout/scene descriptions | 0.050 |
| Scene reconciliation | 0.096 |
| Layout | 0.048 |
| Projection and frame publication | 0.223 |
| Input-target synchronization | 0.027 |

The approximately 0.45 ms outside evaluation is clearly SDK structural work.
The remaining 0.32 ms mixes framework traversal with executing application view
bodies. Together, the measured Foundry update pipeline takes about 26% of this
frame; other shell/rail work is included in the residual UI-update row. This does
not prove all of that is avoidable or newly introduced by declarative authoring:
an imperative implementation would still need some layout, input and update work.
There is no matched imperative baseline in this study.

At 100%, the measured Foundry update pipeline takes 0.888 ms of a 3.191 ms frame;
at 150%, 0.704 ms of 2.829 ms. View evaluation and projection remain the largest
framework targets, not layout mathematics. Native drawing overlaps submission;
completion wait is not total native execution time. Cache preparation itself is
largely real rasterization/allocation, not a framework tax.

These are scrolling-workload phase measurements, not idle cost, compositor latency,
game impact or a timing baseline. No production profiler was added.

## Next Policy Work

Use actual repair demand and expected repeated replay to rank new materializations
against direct drawing. Price new preparation separately from already-resident
rasters; relocation must not discard reusable fragments just because no blit was
needed this frame. Keep admission in the shared renderer, not game/page-specific
scroll flags. Validate narrow repairs, broad repairs and transitions between them.

Changing representations must also preserve rendering correctness. Removing both
hints failed strict fresh/aged pixel tests. Those assertions were not relaxed to
accept the experiment. New coverage compares the retained production path with
direct painting during scrolling, hover and delayed artwork at 90/100/120/130/
150/200%, with exact alpha and the existing three-level RGB compositing tolerance
for cached-versus-direct output. Unchanged repeated painting remains exact.

## Evidence

`research/retinue/benchmarks/materialization-20261104/` retains saved binaries,
source provenance, commands/environments, raw samples and analyses. `policies/`
contains the three-policy study; `background-pairs/` contains the narrower rejected
candidate. `profile/` records phase attribution for that candidate, not a production
performance baseline; `profile-production/summary.json` attributes the restored
production policy. Experimental toggles and phase reporting are test-only.
GUI tests (237 plus seven native-window checks), Common Test, tool/documentation
tests and formatting checks pass. The benchmark workloads also complete.
No application was staged/reloaded and no commit was made.
