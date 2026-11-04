# Native Relocation Integration

Correction: the [copy-executor study](2026-11-03-copy-executors.md) found an
uncontrolled right-rail loading animation in this GUI workload. Its deterministic
GUI matrix supersedes the small GUI percentage claims below. The original evidence
is retained; PoE and Warframe did not use that animation.

The independent-source path is integrated into the Wayland desktop and both overlay
hosts. This report supersedes the integration gap in the
[earlier experiment](2026-11-03-independent-source.md), not its historical results.
It is a measured implementation checkpoint, not a live-game FPS acceptance or a
new regression baseline.

## Integration

Shared presentation storage lends a released writable target and the previous
submitted read-only source. Each slot has its own SHM pool, so the leases are
disjoint without copying pixels or bypassing buffer-release rules. Blend2D queues
source copies and remaining repairs in one context and finishes before submission.
The compositor may still read the source; neither side writes it during the lease.

Views do not request relocation. The shared renderer selects valid moves from
prepared surface content. Only native hosts see acquisition/source leases and the
existing relocation opt-out. X11 and unavailable-source cases retain the safe
in-place fallback. No full-screen scratch image or final presentation copy was added.

## Method

i7-10700, benchmark processes pinned to CPUs 4-7, eight fresh processes per requested
thread count. Policy and thread order alternate between repeats. No builds/tests
ran during timing; normal desktop activity was uncontrolled. Values below are means
of process-level warm medians, including native completion. One thread means
asynchronous caller-only Blend2D, not its zero-thread synchronous mode.

- GUI: real shell/Foundry drag-over-grid code, seven warmed artwork assets in a
  synthetic inventory, 2560x1440 at 100/125/200%, 160 frames with 20 warmup frames.
- PoE: Evaluate drag fixture, 1080p/1440p at 100/150%, 160 timed frames after six warmups.
- Warframe: repeated full cached-frame restoration, 1080p/1440p/2160p, 160 timed frames
  after six warmups. This deliberately exercises full restore, not idle/loading duty cycle.

All GUI/PoE policies maintain the same content/damage evidence. The native candidate
calls the production queued execution path; the older in-place control remains
available for comparison. Headless targets model three presentation slots. These
timings exclude buffer acquisition waits, compositor presentation and game FPS.
Separate isolated software-KWin tests exercise actual native submission.

## GUI

Milliseconds, four requested render threads:

| Scale | Ordinary Repair | Previous In-Place | Native Source | Native vs In-Place |
| --- | ---: | ---: | ---: | ---: |
| 100% | 3.964 | 3.831 | 3.655 | -4.6%, inconclusive |
| 125% | 3.266 | 3.384 | 3.087 | -8.8% |
| 200% | 2.688 | 2.811 | 2.708 | -3.7% |

Against ordinary repair, the four-thread improvement is 7.8% at 100%, 5.5% at 125%,
and effectively neutral at 200% (+0.7%, interval includes zero). The smaller gain
against in-place relocation is the relevant incremental improvement.

Thread sweep, in-place / native milliseconds:

| Threads | 100% | 125% | 200% |
| ---: | ---: | ---: | ---: |
| 1 | 4.364 / 4.298 | 4.071 / 3.751 | 4.017 / 3.573 |
| 2 | 3.932 / 3.652 | 3.710 / 3.181 | 3.207 / 2.951 |
| 4 | 3.831 / 3.655 | 3.384 / 3.087 | 2.811 / 2.708 |

At four threads, the native-minus-in-place paired 95% intervals are -0.413 to
+0.167 ms at 100%, -0.472 to -0.139 at 125%, and -0.205 to -0.005 at 200%.
Intervals resample eight process medians, not individual frames; they are diagnostic,
not simultaneous confidence guarantees. One-thread 100% is also inconclusive.

At 100%, ordinary repair redraws 2.797M pixels/frame. Native relocation repairs
0.344M and copies 2.447M. Compared with in-place relocation, repairs fall from
0.587M, while copying rises from 2.213M. Planning falls from 0.176 to 0.044 ms,
largely because the independent source is the latest rather than an aged target.
Input/layout/recording still cost about 2 ms. Phase medians are not additive.

## PoE Evaluate

Milliseconds at two threads, the normal overlay budget:

| Resolution / Scale | Ordinary Repair | Previous In-Place | Native Source | Native vs In-Place |
| --- | ---: | ---: | ---: | ---: |
| 1920x1080 / 100% | 0.363 | 0.231 | 0.208 | -9.8% |
| 1920x1080 / 150% | 0.636 | 0.447 | 0.364 | -18.6% |
| 2560x1440 / 100% | 0.365 | 0.238 | 0.216 | -9.3% |
| 2560x1440 / 150% | 0.597 | 0.369 | 0.322 | -12.7% |

All four paired intervals favor native relocation. At 1440p, they are -0.029 to
-0.016 ms at 100%, and -0.061 to -0.032 at 150%. Against ordinary repair, gains
are 41-46%. At 1440p/100%, repaired pixels fall from 447,720 to 80,625; 368,160
pixels are copied.

Thread sweep at 1440p, in-place / native milliseconds:

| Threads | 100% | 150% |
| ---: | ---: | ---: |
| 1 | 0.220 / 0.247 | 0.342 / 0.412 |
| 2 | 0.238 / 0.216 | 0.369 / 0.322 |
| 4 | 0.231 / 0.193 | 0.365 / 0.265 |

Native relocation is slower than in-place here at one thread, despite beating
ordinary repair. Do not infer a universal winner or increase all host thread counts
from this workload. Effective concurrency belongs in later plan-cost calibration.

## Warframe Restore

Integration initially converted whole-frame CPU restoration to a whole-frame native
copy on one thread. That regressed 1440p by 75% and 2160p by 67%. Those failed
measurements are retained. Most of this frame is transparent.

The final implementation clears the exterior of the cached paint bounds and copies
the interior, with no double-writing between them. It also uses the renderer's
existing two-thread host budget instead of forcing the surface pass to one thread.
Both operations remain in the same native context. The original CPU restore is a
test-only control, not an additional production callback.

| Resolution | Previous CPU Restore | Final Native Restore | Change |
| --- | ---: | ---: | ---: |
| 1920x1080 | 0.514 ms | 0.148 ms | -71.2% |
| 2560x1440 | 0.779 ms | 0.650 ms | -16.5%, inconclusive |
| 3840x2160 | 2.450 ms | 2.299 ms | -6.2%, inconclusive |

Paired intervals: -0.426 to -0.307 ms, -0.233 to +0.006, and -0.446 to +0.081.
Do not call the higher-resolution means proven gains. A caller-only surface pass
still loses at higher resolutions; there is no universal copying speedup.

## CPU And Memory

CPU milliseconds per frame at 1440p/100%, not measured utilization:

| Workload | Ordinary Repair | Previous In-Place/CPU | Native |
| --- | ---: | ---: | ---: |
| GUI, 4 threads | 7.493 | 4.548 | 6.476 |
| PoE, 2 threads | 0.512 | 0.250 | 0.267 |
| Warframe full restore, 2 native threads | n/a | 0.841 | 1.342 |

The GUI native path spends more aggregate CPU than in-place movement despite lower
mean latency. At 60 continuously painted frames/s, 4.548 and 6.476 CPU ms/frame
correspond to roughly 27% and 39% of one logical CPU. This is illustrative duty-cycle
arithmetic, not measured system utilization, peak-core load or game interference.
Idle overlays do not perform these workloads continuously.

One RGBA target costs 7.91 MiB at 1080p and 14.06 MiB at 1440p. The shared host
grows to two slots to preserve an independent source and at most three when busy:
15.82/23.73 MiB at 1080p and 28.13/42.19 MiB at 1440p. It may therefore allocate more
presentation memory than an old host that only needed one slot. There is no extra
scratch allocation on top. These are pixel capacities, not application RSS.
Pool allocation now includes SCTK's 64-byte slot padding; omitting it could double
the pool capacity at odd dimensions. The allocation regression test checks that.

Full-snapshot, source-region-snapshot and offscreen-pair alternatives still lose
in the GUI matrix. At four threads/100%, they take 4.923, 4.456 and 4.585 ms versus
3.655 ms for native presentation-source reuse, with 56.25/56.25/70.31 MiB of pixel
storage versus 42.19 MiB. A cropped scratchpad remains a separate deferred experiment.

## Verification And Evidence

- Shared renderer/SDK tests passed, including buffer age, source alpha, clipped
  changes, failed/abandoned frames and disabled relocation.
- The Wayland protocol fixture verifies actual source pointer identity, safe reads
  of busy sources, target-release gating, slot caps, resize/reset and allocation size.
- GUI: 228 tests plus seven native-window tests. PoE presentation: 74 tests.
  Final Warframe presentation/runtime selection: 43 tests. CT and tool tests passed.
- Isolated KWin: native GUI Foundry at 100/125%, Relics, and generic overlay
  scroll/loading submissions passed. No user game, service or staged build changed.
- Live compositor latency and game FPS remain unverified; the earlier FPS-dip
  investigation remains deferred. No claim that relocation fixes those dips.

Evidence root: `research/retinue/benchmarks/independent-source-20261103/`.
`gui-integrated-matrix/`, `poe-integrated-matrix/` and `wf-budgeted-matrix/` contain
raw reports, run manifests, summaries and paired intervals. Intermediate Warframe
`integrated`, `bounded` and `partitioned` matrices preserve rejected approaches.
`measure-integrated.py` and `analyze-integrated.py` reproduce collection/analysis.
`native-smoke/` holds isolated compositor logs and GUI captures.

Saved binary/source snapshots have these SHA-256 identifiers:

| Snapshot | Binary SHA-256 |
| --- | --- |
| gui-integrated-snapshot | `aaee18c6ea502f7dd7a56cce78d2ee7662c94bf098625161dd0bd882f1d2a2e9` |
| poe-integrated-snapshot | `3e3c499b6430ea1a4e8ebec20362b783db81a6f09476fb15c35ea1a3fcc9cdb2` |
| wf-budgeted-snapshot | `9bb7b08ca19c360fb412e9600f0d78d2247218e51c2cf6cf87b5bf4a8be4c19e` |

Each snapshot retains its tracked patch and matching untracked-source archive.
The later pool-padding fix changes host allocation, not the headless GUI/PoE queue
measurements; it was verified separately with final native/protocol tests.
