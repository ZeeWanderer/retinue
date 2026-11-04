# Relocation Copy Executors

Keep queued Blend2D copying. Neither serial libc copies nor a separate persistent
copy pool is a general latency improvement. The alternatives remain test/benchmark
code, not another shipping rendering mode or worker pool.

This report corrects the uncontrolled GUI loading animation in the earlier
[native integration measurements](2026-11-03-native-relocate.md). It does not claim
that the remaining live-game FPS dips are fixed.

## Comparison

All three executors use the same immutable previous presentation buffer, relocation
plan, copy rectangles and repair regions:

- Queued: integer, same-format `SRC_COPY` and repairs in one Blend2D context.
- Serial: `copy_from_slice` into the target before opening its context; contiguous
  full-width copies are coalesced, partial-width copies use rows.
- Row-parallel: disjoint horizontal target chunks, caller plus N-1 persistent Rayon
  workers, completion before painting. The active copy budget matches the requested
  Blend2D budget. No per-frame thread creation or extra pixel buffer.

The older in-place control uses overlap-safe row movement and the acquired target's
older content. It is a useful policy control, but not an executor-only comparison.

An i7-10700, CPUs 4-7, eight fresh processes for each of 1/2/4 threads. Policy and
thread order alternate. No builds/tests ran during timing; desktop load remained
uncontrolled. One thread means asynchronous caller-only Blend2D. GUI uses the real
Foundry shell, eight pointer events per frame, warmed captured artwork in a
synthetic inventory, 160 frames with 20 warmups. PoE uses its Evaluate drag fixture
and 160 timed frames after six warmups. Presentation waits and game FPS are excluded.

Numbers are means of process-level warm medians. GUI CPU uses process CPU medians;
PoE CPU uses aggregate process CPU divided by frame count. Intervals resample eight
paired process medians, not individual frames; they are diagnostic, not CI gates.

### Benchmark Correction

The first GUI matrix left the right rail unloaded. Its wall-clock loading pulse
repaired another 557,600 pixels on different frames depending on executor speed.
That invalidated a supposedly identical-work comparison. The corrected workload
uses a loaded, static rail. All independent-source modes now have identical per-frame
copy and repair coverage; the harness checks this before accepting a report.

The prior GUI percentages should not be treated as isolated copy-execution gains.
Use the corrected GUI controls below. PoE already had a fixed animation phase and
equal coverage. Its measurements did not have this problem.

## GUI

2560x1440, four requested threads, milliseconds:

| Scale | Ordinary Repair | Old In-Place | Queued | Serial Copy | Row-Parallel Copy |
| --- | ---: | ---: | ---: | ---: | ---: |
| 100% | 3.749 | 4.001 | 3.734 | 4.025 | 3.655 |
| 125% | 3.286 | 3.330 | 3.146 | 3.504 | 3.356 |
| 200% | 2.653 | 2.769 | 2.651 | 2.865 | 2.738 |

At 100%, row-parallel's apparent 2.1% gain is inconclusive: paired delta interval
-0.210 to +0.048 ms. It loses at 125% (+0.106 to +0.304 ms) and 200% (+0.052 to
+0.125 ms). Serial loses at all three scales. Two-thread queued/serial/parallel:
3.804/4.176/3.869 ms at 100%, 3.339/3.649/3.405 at 125%, and 2.931/3.069/3.009
at 200%. The separate pool provides no consistent improvement.

Queued relocation beats the old in-place policy at all three four-thread scales.
Against ordinary repair it is neutral at 100/200% and about 4.3% faster at 125%.
At two threads it is about 15.7%, 14.4% and 2.7% faster than ordinary repair in the
means; the corresponding paired intervals favor relocation, including the smaller
200% difference (0.035 to 0.127 ms for ordinary repair minus queued).

At 100%/four threads, relocation copies 2.447M pixels and repairs 0.184M versus
ordinary repair's 2.630M. Yet input, layout and recording still cost about 1.97 ms,
repair submission 0.65 ms, and native completion 1.13 ms. These phase medians are
not additive. Copying 9.33 MiB of pixels still requires both reads and writes;
93% fewer repainted pixels does not mean 93% less frame time.

CPU ms/frame at four threads:

| Scale | Ordinary Repair | Queued | Serial Copy | Row-Parallel Copy |
| --- | ---: | ---: | ---: | ---: |
| 100% | 6.893 | 6.649 | 4.431 | 6.587 |
| 125% | 6.140 | 5.580 | 3.962 | 6.053 |
| 200% | 4.739 | 4.592 | 3.896 | 4.588 |

Serial copying saves aggregate CPU but loses latency. Parallel copying gives up
most of that CPU saving without a reliable latency benefit. These are not measured
utilization or peak-core/game interference. At a hypothetical 60 repaints/s,
6.649 CPU ms/frame is about 40% of one logical CPU, not 40% of the whole machine.

## PoE Evaluate

2560x1440, the normal two-thread budget, milliseconds:

| Scale | Queued | Serial Copy | Row-Parallel Copy |
| --- | ---: | ---: | ---: |
| 100% | 0.220 | 0.254 | 0.236 |
| 150% | 0.337 | 0.445 | 0.354 |

Serial loses 15.7% and 32.0%; both intervals exclude zero. Row-parallel's mean is
7.2%/4.9% slower, with inconclusive intervals (-0.001 to +0.037 ms and -0.012 to
+0.047 ms). That is not evidence to add a shipping copy pool. CPU rises from
0.269 to 0.292 ms/frame at 100% and from 0.505 to 0.528 at 150%.

At four threads, queued/serial/parallel are 0.198/0.260/0.211 ms at 100%, and
0.297/0.455/0.299 at 150%. At 1080p, the two-thread means likewise favor queued
execution: 0.213/0.250/0.234 and 0.427/0.548/0.466 ms. One-thread results have no
consistent winner across resolutions/scales; they do not justify a global switch.

## Interpretation

This is not an accidental scalar copy in the native bridge. It uses `bl_context_blit_image_i`
with `BL_COMP_OP_SRC_COPY`, not a scaled blit or an alpha blend. In the checked-out
Blend2D, `CompOpPart::should_just_copy_opaque_fill` accepts aligned same-format
source copies, and `c_mask_memcpy_or_memset_loop` emits `inline_copy_span_loop`.
Workers process cyclic horizontal bands. The opaque flag here describes coverage,
not a requirement that every source alpha byte be opaque.

A separate copy phase imposes a completion boundary before repair submission;
the queued path can execute both through one worker batch. Cache locality and
worker coordination are plausible contributors, not measured causal attribution.
The experiment does not establish that concurrent raw writes into a live Blend2D
target are safe or faster. That would also require proving all read/write footprints
and preserving Rust and native image lifetimes, not merely comparing clip rectangles.

Keep the integrated native path, existing overlap-safe fallback and test controls.
Do not add a thread pool, manual SIMD routine, scratch image or runtime tuning knob
without a measured benefit. Remaining GUI work is also in input/layout/recording
and repair-command submission. Plan admission should eventually account for
execution budget and repair fragmentation; it must not assume saved pixels equal
saved time or be calibrated to one fixture's percentages.

## Verification

- Shared renderer/SDK tests passed with headless, overlay/benchmark and desktop
  feature sets. Both copy executors pass pixel and invalid-buffer checks.
- GUI: 228 tests and seven native-window tests passed. The new coverage guard also
  passed a reversed-order, artwork-backed three-executor run.
- PoE presentation: 74 passed. Warframe presentation: 43 passed.
- CT, tool tests, formatting and diff checks passed.
- Normal GUI/PoE dependency graphs contain no Rayon. Copy selection and its pool
  compile only in tests/benchmarks; ordinary apps still use native queued copying.

## Evidence

Root: `research/retinue/benchmarks/independent-source-20261103/`.
`gui-copy-fixed-matrix/` and `poe-copy-matrix/` retain raw reports, manifests,
summaries and paired intervals. `gui-copy-matrix/` retains the rejected loading-pulse
experiment. `measure-integrated.py` and `analyze-copy.py` collect/analyze them.

| Snapshot | Binary SHA-256 |
| --- | --- |
| gui-copy-fixed-snapshot | `5dbd5944bf6bb964ac68dffd26fa3684e4631363f28af0664ee7b2c9854d25d5` |
| poe-copy-snapshot | `add024696a36fd91606ca5ebc2e29a6f03a5510625f25fdb3f81fed1a6bf28e4` |

Each snapshot archives the dirty tracked patch and matching untracked sources.
Later assertion/API-isolation changes do not alter the measured execution. Pixel
oracles cover both executors, including nested GUI pages, translucent source pixels,
uneven row partitions, preserved untouched regions and rejected invalid buffers.
No live app, companion, service or game was replaced for this experiment.
