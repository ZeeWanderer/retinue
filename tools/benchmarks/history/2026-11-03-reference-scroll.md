# Relocation Versus Reference Architectures

The queued copy mechanism is competitive. Relocation is not universally cheaper
than painting, even in a small native prototype: fast fills can beat copies, and
already-rasterized content can make ordinary repair almost equivalent to relocation.
The remaining architectural weakness is admission based on original drawing cost,
without the optimizer's knowledge of resident rasters and effective replay work.

No shipping executor/policy change follows from this experiment. Keep native queued
copies, retain no-relocation as a real alternative, and improve admission evidence
before adding more storage or thread pools. Live game FPS remains unverified.

## Method

Two separate comparisons, both on an i7-10700 pinned to CPUs4-7, eight fresh processes
per configuration, alternating case/thread order. No builds/tests or other benchmark
ran during timing; ordinary desktop activity was not controlled. Values below are
means of process-level warm medians, not pooled independent frame observations.
Paired intervals resample processes. They are diagnostic, not calibrated CI gates.

1. **Production path:** real GUI Foundry and PoE Evaluate drivers, exact
   `surface::Buffered` with `Relocation::Disabled` versus its queued-copy path.
   Same scene, input, buffer history, cached resources and native completion.
   A test-only region-first alternative replays the prepared UI once per repair
   rectangle rather than restricting each command to the disjoint region.
2. **Native reference:** actual Qt6.11.1 QPainter and its exported
   `qt_scrollRectInImage`, plus direct Blend2D contexts over equivalent externally
   owned PRGB32 buffers. Explicit integer scroll offsets, changing hover holes,
   three cyclic targets, same font with pre-shaped text and pre-scaled artwork. Flat
   and richer card scenes, 100/125/200% UI scaling, Blend2D 1/2/4 threads. The viewport is
   2368x1248 within a 2560x1440 target. Each case has 176 frames, first 16 excluded.

The reference excludes application input/layout/recording and native presentation.
It is not a whole Qt application, Chromium or Windows compositor benchmark. Qt uses
its default raster implementation; no Qt worker-budget override was applied. One
Blend2D thread means asynchronous caller-only execution, not its synchronous path.
Qt's blocking draw API and Blend2D's queued API divide submission/completion work
differently; compare total completion time, not those phases across engines.
Antialiasing can differ across engines; each candidate is checked against fresh
painting by its own engine, not byte-identical Qt/Blend2D output.

## Actual GUI

1440p Foundry, warmed captured art in a synthetic inventory, eight pointer events
per frame, 160 frames with 20 warmups, static loaded right rail. Milliseconds:

| Threads | Scale | No Relocation | Queued Relocation | Region-First Replay |
| --- | --- | ---: | ---: | ---: |
| 2 | 100% | 4.524 | 3.756 | 5.003 |
| 2 | 125% | 3.792 | 3.352 | 4.317 |
| 2 | 200% | 3.105 | 2.984 | 4.154 |
| 4 | 100% | 3.839 | 3.646 | 4.817 |
| 4 | 125% | 3.371 | 3.260 | 4.195 |
| 4 | 200% | 2.720 | 2.794 | 3.853 |

At four threads, native wins about 5.0%/3.3% at 100/125%; the paired no-relocation
minus native intervals are +0.061 to +0.290 ms and +0.063 to +0.155 ms. At 200%, the
apparent 2.7% loss is inconclusive (-0.234 to +0.072 ms). At two threads, gains are
17.0%/11.6%/3.9%, with intervals favoring native at all three scales.

At 100%/four threads, the native path copies 2.447M pixels and repairs 0.184M instead
of repairing 2.630M. But input/layout/recording still has a roughly 2 ms median;
submission rises from 0.488 to 0.665 ms, and completion falls only from 1.251 to 1.122 ms.
These phase medians are not additive. Region-first submission rises further to
1.522 ms; repeatedly entering the prepared UI costs more than it saves here.
The independent-source modes have identical per-frame copy/repair coverage,
checked by the harness.

The baseline already prefers raster caching for Foundry card backgrounds/artwork;
it is not an uncached redraw. Aggregate CPU ms/frame at 100%/four threads is 6.933
without relocation versus 6.442 with it; at two threads 6.241 versus 4.895. Neither
those numbers nor headless latency measures peak-core interference or game FPS.

## Actual PoE Evaluate

1440p, two threads, 160 measured frames after six warmups:

| Scale | No Relocation | Queued Relocation | Region-First Replay |
| --- | ---: | ---: | ---: |
| 100% | 0.366 | 0.216 | 0.231 |
| 150% | 0.623 | 0.350 | 0.328 |

Native reduces latency by 40.9%/43.9%; paired intervals favor it by 0.144-0.157 ms
and 0.256-0.291 ms. CPU ms/frame falls from 0.515/0.973 to 0.264/0.529.

Region-first is mixed: about 14 us slower at 100% and 21 us faster at 150% in this
two-thread 1440p case. It also loses at both 1080p scales and both four-thread 1440p
scales. That does not justify a global replay-order change, especially given the
large GUI losses. Both relocation replay variants retain the same copied and
repaired areas.

## CPU Reference

100% scale, milliseconds. Blend2D uses four threads; Qt uses its default raster
implementation within the same CPU affinity:

| Engine / Content | Repaint | Previous-Buffer Copy | Qt In-Place Scroll | Retained Rows | Rows + Copy | Persistent Image + Flush |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Qt / flat | 1.642 | 1.195 | 1.076 | 1.167 | 1.172 | 1.899 |
| Qt / cards | 7.142 | 1.649 | 1.727 | 1.173 | 1.175 | 2.388 |
| Blend2D / flat | 0.681 | 1.004 | 1.045 | 0.964 | 1.041 | 1.678 |
| Blend2D / cards | 1.447 | 1.022 | 1.160 | 1.026 | 1.043 | 1.713 |

The in-place column calls the actual Qt scroll primitive, even when Blend2D paints
the repairs. It uses the acquired target's older generation; the independent-source
column uses the latest completed image. This changes exposed/hover repair coverage,
so it is a storage-policy comparison, not just a copy-instruction comparison.

Retained rows are content-space rasters composed at their current positions. Row
miss allocation/rasterization and hover updates are timed; only visible rows and
one neighbor on either side remain resident. Persistent-image mode moves/repairs
a fourth image, then copies the changed viewport into the presentation target;
its final copy and completion are included.

Conclusions:

- Qt's large rich-content gain is real in this test, but its starting repaint cost
  is much higher. Blend2D's fast painter leaves less work for relocation to avoid.
- Native Blend2D copy is about 29% faster for rich cards and 47% slower for flat
  content. The respective paired deltas are -0.444 to -0.407 ms and +0.290 to +0.358 ms.
  A copy reads pixels as well as writing them; an opaque fill need not read a source.
- At 200%, fewer larger cards make Blend2D repaint 0.988 ms versus copy 1.065 ms. The
  +0.015 to +0.158 ms interval favors repaint. Saved pixel area alone is inadequate.
- Retained rows and relocation have similar 100% rich-scene latency. Combining
  them does not produce an obvious additional win. The three target buffers cost
  42.19 MiB in every base case; row retention adds 16.26 MiB at 100% and 18.07 MiB at 200%.
  Those are pixel allocations, not RSS. Repeated fragments can still merit caching.
- The fourth persistent image adds 14.06 MiB and a final copy. It is worse than both
  native relocation and repaint for the Blend2D 100% cases. Another image is not a
  free way to avoid ownership constraints.

## Reference Contracts

[Qt's public backing-store API](https://doc.qt.io/qt-6/qbackingstore.html#scroll)
receives an explicit scroll region and offset. The
[Qt6.11.1 Wayland implementation](https://github.com/qt/qtbase/blob/v6.11.1/src/plugins/platforms/wayland/qwaylandshmbackingstore.cpp)
moves valid back-buffer pixels in place and copies stale parts from the front
buffer. Its fractional device-pixel-ratio guard is separate from this experiment's
UI scaling; all tested moves are integral buffer pixels. Its
[scroll primitive](https://github.com/qt/qtbase/blob/v6.11.1/src/gui/painting/qbackingstore.cpp)
uses directional row copies for vertical movement and overlap-safe movement for
horizontal overlap. It does not require a bespoke SIMD copier.

[Chromium's compositor](https://github.com/chromium/chromium/blob/main/docs/how_cc_works.md)
retains rasterized tiles and can update scroll/property state without rebuilding
Blink layout. The row-cache prototype tests that general separation of raster
content from placement, not Chromium's exact tile implementation or GPU performance.
[DXGI scroll presentation](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-1-2-presentation-improvements)
can delegate reconstruction to the compositor. Its cost cannot be inferred from
CPU SHM copying on Linux; no Windows/GPU timings were measured here.

## Remaining Admission Gap

`surface::Snapshot` stores `program.summary().work`; `repair_cost` multiplies its
clamped weight by intersecting pixel area. `relocate::Plan` compares that with a
copy-area/row-overhead estimate. It does not consume the renderer's effective
cached draw plan or execution budget. Consequently it can overestimate the work
avoided when an expensive source program is already a cheap resident-image blit.
This is a code-established limitation, not a proven explanation for every delta.

Keep logical content/version evidence for reuse validity and cache dependencies.
Add separate compact effective-work evidence from lowering/residency to admission;
do not keep images in historical snapshots or materialize every candidate merely
to estimate it. Account for copy traffic, existing blits, sparse replay overhead
and execution budget. Do not tune thresholds to these particular fixture scales.
The measured GUI frontend cost also keeps the existing input/hit-testing audit
relevant; relocation cannot remove work already spent before repair planning.

## Verification And Evidence

Native prototypes passed 144 engine/thread/scale/content/policy pixel-oracle cases,
including changed hover holes, reverse scrolling and jumps. Shared surface tests
cover disabled and region-first paths, failures and target generations. Nested GUI
page and PoE hover/drag oracles cover the new replay control. GUI comparison settles
deferred raster preparation before its oracle, rather than loosening pixel tolerance.
CT, shared composition tests, affected GUI/PoE checks and tool tests passed.

Raw sources/samples/manifests: `research/rendering/reference-scroll-20261103/`.
The native executable SHA-256 is
`22028b1b5aeadb6516f5c69c076f4f0ea9dd4aabac5d38d6860a2d4ea72252ea`;
its manifest records Qt/Blend2D library, source, font and artwork hashes.
Production snapshots/matrices remain under
`research/retinue/benchmarks/independent-source-20261103/`:

| Snapshot | Binary SHA-256 |
| --- | --- |
| gui-architecture-fixed-snapshot | `3c908aa679625d04baa3ad083df1c8d71d70728853e6e4572401db56be4a3dfd` |
| poe-architecture-final-snapshot | `ea1f29280169c234b34ae9985b5aaad1a93624ca6043d10923f8dae6f6904ef0` |

Snapshots retain matching tracked patches/untracked sources. The earlier
`*-architecture-snapshot` and `poe-architecture-fixed-*` are rejected setup attempts,
not timing evidence. Region replay needed scoped clipping, and the PoE harness
must restore its already-prepared frame token before each region. These are
test-only changes; production consumes that token once. No app/game/service was
replaced or restarted. No baseline or shipping optimization policy was changed.
