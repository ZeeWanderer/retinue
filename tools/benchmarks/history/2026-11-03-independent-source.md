# Independent-Source Relocation

Correction: the [copy-executor study](2026-11-03-copy-executors.md) fixes an
uncontrolled GUI loading animation and provides deterministic repair/in-place/native
controls. Treat small differences here cautiously; these historical results remain
unchanged, not promoted to regression baselines.

The queued independent-source experiment works correctly, without overlapping
self-blits or a full-frame pre-copy. These measurements precede native-host
integration; they do not measure that integration or compositor acquisition.
This follows the [in-place investigation](2026-11-03-relocate-production.md).

## Method

Twenty-four fresh release processes: eight each at 1/2/4 requested render threads,
CPUs 4-7 on an i7-10700. Each process tests seven policies at 100/125/200% scale on
a 2560x1440 target. Alternate rounds reverse policy order. No builds or tests ran
concurrently; desktop load was uncontrolled. One thread is asynchronous caller-only
Blend2D execution, not its zero-thread synchronous mode.

The actual GUI shell and Foundry process eight pointer events per frame, dragging
the scrollbar with the pointer over the grid. Seven cached PNGs fill synthetic
cards and component slots. Artwork and built-in icons are warmed before timing;
assertions check that subsequent viewports request no unwarmed images. This is not
a captured player inventory. Each case has 160 frames, with 20 discarded as warmup.

All policies retain source snapshots and damage history. Independent sources use
the most recently completed different buffer; each acquired target still repairs
its own age. Copies and remaining repairs share one Blend2D context. Scratch modes
first copy the acquired target into persistent scratch storage, complete that
context, then queue relocation and repair. Offscreen mode includes dirty-region
copying into a separate three-buffer presentation model. No compositor, acquisition
wait or live game FPS is measured.

## Results

Means of per-process warm medians, milliseconds; four render threads:

| Policy | 100% | 125% | 200% | Pixel Storage |
| --- | ---: | ---: | ---: | ---: |
| Ordinary repair, three targets | 3.881 | 3.326 | 2.851 | 42.19 MiB |
| In-place row relocation | 3.898 | 3.576 | 2.855 | 42.19 MiB |
| Previous independent target, three slots | 3.639 | 3.340 | 2.769 | 42.19 MiB |
| Previous independent target, two slots | 3.570 | 3.244 | 2.775 | 28.13 MiB |
| Full snapshot then native relocation | 5.266 | 4.770 | 4.429 | 56.25 MiB |
| Required-source snapshot then relocation | 4.680 | 4.044 | 2.951 | 56.25 MiB |
| Offscreen pair plus three presentation slots | 4.929 | 4.454 | 3.687 | 70.31 MiB |

Storage counts only allocated pixel buffers, not total RSS, assets, native scratch
or metadata. The two-slot case does not justify shrinking a compositor-facing pool:
the model always has a writable target, unlike a live compositor.

At 100% scale, using three slots for both policies:

| Threads | Repair | Independent Source | Change | Repair CPU | Source CPU |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 6.261 ms | 4.228 ms | -32.5% | 6.233 ms | 4.212 ms |
| 2 | 4.478 ms | 3.681 ms | -17.8% | 6.236 ms | 4.815 ms |
| 4 | 3.881 ms | 3.639 ms | -6.2% | 7.246 ms | 6.597 ms |

CPU values are process CPU milliseconds per frame, not utilization or live-game
cost. The independent path consumes more CPU than serial in-place relocation at
four threads, although it uses less than ordinary replay in this workload.

Paired bootstrap intervals use eight per-process medians, not individual frames.
At four threads, the three-slot difference is -0.241 ms at 100% (95% interval
-0.331 to -0.144), +0.014 ms at 125% (-0.077 to +0.130), and -0.081 ms at 200%
(-0.201 to +0.042). The higher-scale changes are inconclusive. These are diagnostic
intervals, not simultaneous confidence guarantees or accepted regression gates.

At 100%, mean repaired area fell from 2.79M to 0.34M pixels, while about 2.45M
pixels were copied. Input, layout and recording still cost roughly 2 ms. Moving
pixels is real work, and four-thread replay is already efficient: pixel reduction
does not translate directly into the same latency reduction.

The offscreen pair's final presentation copy alone had a mean process-median cost
of 1.069 ms at 100%. Full and required-source snapshots cost 1.329 and 0.830 ms
before their relocation/repair context. Persistent allocation removes allocation
churn, not copy bandwidth or the intermediate completion dependency. Phase medians
are not additive.

## Decision

- Keep the exact independent-source copy mechanism and the test-only comparison.
  Do not replace safe in-place relocation with an overlapping native self-blit.
- Prefer an already-owned immutable source lease when a backend can provide it.
  Neither host exposed that lease at measurement time; do not hide an extra
  image copy behind the interface to simulate it.
- Do not adopt a permanent screen-sized scratch image or offscreen pair by default.
  Source-region snapshots help versus full duplication, but lose to direct repair
  here. A cropped/packed scratchpad remains a separate, deferred candidate.
- Selection still needs effective execution concurrency, transfer topology, command
  coverage and available raster alternatives. The measured benefit is conditional,
  not grounds for an always-relocate policy or a universal thread-count change.
- Preserve backend buffer-release rules. Current-frame presentation damage and
  acquired-buffer repair remain separate; dirty submission does not make unknown
  pixels outside the damage region valid.

## Evidence

Ignored evidence root: `research/retinue/benchmarks/independent-source-20261103/`.
`gui-artwork-snapshot/` preserves the binary and source/build manifest;
`artwork-matrix/` has 24 raw reports, its run manifest, summary and paired intervals.
`measure.py` and `compare.py` reproduce collection and analysis. Earlier bare-card
and warmup runs in the directory are not included in these tables.
Measured binary SHA-256:
`a48991106f442ac61849d520f504cec13a1217d1c074e45138dfe924a05a063f`.
The additional untracked-source archive contains only files matching the snapshot
hashes. Later shared-test and documentation edits are excluded, not silently
substituted for the measured sources.

The shared correctness suite checks exact alpha-preserving copies, bounds, failed
targets and irregular buffer reuse. GUI tests compare independent-source and
source-region-snapshot rendering against fresh rendering for Foundry, Inventory,
Mastery and Relics at 100/125%, with three targets. Native antialias boundaries
allow at most one channel unit; the rectangular shared fixtures require exact bytes.
The larger scratch/offscreen policies remain experiments, not live acceptance.
