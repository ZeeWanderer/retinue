# Incremental Structural Projection

Follow-up to [projection overhead](2026-11-04-projection-overhead.md). Structural
changes now reuse clean projected subtree ranges rather than revisiting every
layout node. Dirty ancestry includes child order, reparenting, visibility, cache
hints and render context. Reused geometry inherits current placement, clips and
input gates. PaintFrame remains the authority for visual damage.

The previous compact projection is held temporarily while indices are rebased.
Publication still copies compact arrays in O(N); this is not allocation-free or
sublinear. There is no persistent second scene or raster cache. Resize, scale
changes, layout-failure recovery and fully dirty scenes retain the full path.

This also fixes custom render-context changes being missed when child layout
values change without moving geometry.

## Measurements

Intel i7-10700, affinity CPUs 4-7, two/four render threads, optimized binaries.
Actual shell/Foundry with synthetic cards and warmed captured artwork. Framebuffer
2560x1440, desktop scale 1.0. Eight alternating process pairs, excluding an initial
pair; 140 measured frames after 20 warmup frames per case. All 13,440 paired frames
retain identical copied/repaired pixel areas. No concurrent builds or tests;
ordinary desktop load remains uncontrolled.

Independent-source rendering with three target buffers, mean of process medians
in milliseconds:

| Threads | GUI Scale | Recording Before | Recording After | Frame Before | Frame After |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 100% | 1.433 | 1.156 | 3.801 | 3.769 |
| 2 | 125% | 1.174 | 1.040 | 3.549 | 3.497 |
| 2 | 200% | 0.741 | 0.721 | 3.211 | 3.194 |
| 4 | 100% | 1.438 | 1.144 | 3.530 | 3.344 |
| 4 | 125% | 1.223 | 1.010 | 3.403 | 3.205 |
| 4 | 200% | 0.739 | 0.718 | 2.951 | 2.917 |

Recording includes application construction and SDK evaluation/layout/projection,
not just library bookkeeping. At 100%, it improves 17-21% across both rendering
modes and thread counts; the independent-source gain at 125% is 11-17%. A four-pair
identical-binary control varies much less in these recording cases. Keep the change.

Whole-frame effects are smaller and noisier. Independent-source totals improve
0.5-5.8%, but control totals drift up to 3.5%. Repair-only 2-thread/200% median time
increases 2.0%; its mean falls 0.5%, and the identical-binary control reports a
similar 2.1% median increase. This is not a reproduced source regression or proof
of uniform whole-frame improvement. No timing baseline is advanced. No compositor
or game FPS measurement is claimed.

## Scale Coverage

The paired results above retain the original 100/125/200% matrix. They are not
measurements at 120%. Workload 5 defaults to 90-130% in 5% steps, with
100%/120% primary and 150% as a higher-scale reference. `GUI_BENCH_SCALES` permits
focused direct runs and reproducing the old matrix. Framebuffer and desktop scale
are now explicit report fields; workload version changes accordingly.

200% remains useful for correctness stress, not primary performance tuning.
Larger GUI scale at fixed framebuffer size also reduces visible logical content;
cross-scale timings are different workloads, not a pure resampling comparison.

A separate descriptive sweep ran the new default matrix in three fresh processes
per thread count, with warmed artwork, 20 warmup frames and 140 measured frames per
case. Both repair-only and independent-source modes completed at every scale.
There is no before binary at the newly added scales. Independent-source results,
mean of process medians in milliseconds:

| GUI Scale | Recording, 4 Threads | Frame, 2 Threads | Frame, 4 Threads |
| ---: | ---: | ---: | ---: |
| 90% | 0.564 | 3.216 | 2.905 |
| 95% | 1.134 | 3.838 | 3.343 |
| 100% | 1.135 | 3.541 | 3.226 |
| 105% | 1.087 | 3.837 | 3.315 |
| 110% | 0.501 | 3.581 | 2.826 |
| 115% | 1.090 | 3.696 | 3.279 |
| 120% | 0.878 | 3.703 | 2.999 |
| 125% | 0.930 | 3.577 | 3.074 |
| 130% | 1.030 | 3.383 | 3.277 |
| 150% | 0.948 | 3.786 | 3.168 |

Do not infer a smooth cost curve from these values: different scales change row
geometry and the frequency of structural edits in this scrollbar trace. This
sweep establishes coverage and a current reference, not a causal thread-count
comparison or a regression baseline.

## Verification And Next Work

Tests compare fresh and retained geometry, hit testing, collection/group metadata
and aged-buffer pixels across structural edits at 100/125/200%. They cover changed
ancestors, clipping, input gates, insertion, removal, reordering and reparenting.
Separate regressions check unchanged-descendant work and render-context-only
updates. Shared SDK/renderer, GUI, native-window, both companion presentation
suites and Common Test passed, as did formatting and whitespace checks.
The scale-override parser, benchmark/tooling checks and Common Test also pass
after the matrix change.

Next compare materialization preparation with direct replay. Incremental projection
reduces one measured cost; it does not establish that raster preparation is wasted
or that more cache eviction would help.

## Evidence

`research/retinue/benchmarks/incremental-projection-20261104/` retains binary/source
snapshots, eight-pair runs in `paired/`, four-pair same-binary runs in `control/`,
analysis and test logs. Binary SHA-256 prefixes: before `f12b2fd789dd426a`, after
`e94df63fcacbae57`. All 6,720 control frames also match copy/repair coverage.
These are dirty-worktree measurements. No app was staged or reloaded, and no
commit was made.

`scale-matrix/` retains the new-workload binary/source snapshot; `scale-sweep.py`
records exact invocation/environment overrides and per-process reports under
`scale-sweep/`. The final override-parser validation change does not affect the
measured default matrix.
