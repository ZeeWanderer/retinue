# Shared Surface Relocation

The production hosts now share acquired-buffer history, relocation and repair.
Correctness checks pass, but large GUI scrolling has not passed performance
acceptance. No live game-FPS conclusion follows from these headless measurements.
The follow-up [independent-source comparison](2026-11-03-independent-source.md)
includes populated artwork, thread counts and scratch/presentation-copy costs.

## Application Results

Eight fresh processes, four rendering threads, CPUs 4-7 on an i7-10700. Entries
are means of per-process warm medians in milliseconds. Source was uncommitted;
no baseline was replaced. Desktop load was uncontrolled.

| Workload | Scale | Ordinary Repair | Relocate |
| --- | ---: | ---: | ---: |
| Foundry scrollbar drag | 100% | 2.980 | 3.213 |
| Foundry drag, pointer over grid | 100% | 3.145 | 3.490 |
| Foundry scrollbar drag | 125% | 2.414 | 2.733 |
| Foundry scrollbar drag | 200% | 1.571 | 1.776 |
| PoE panel drag, 1440p | 100% | 0.298 | 0.237 |
| PoE panel drag, 1440p | 150% | 0.427 | 0.373 |

The GUI results include the expanded-leaf cost correction described below. They
use fixture cards without remote item artwork, not a fully populated live grid.
The PoE results follow a recording correction: backdrop and item frame are local
sections separate from the changing title. These are structural boundaries, not
optimization hints. At 1440p/100%, mean process CPU fell from 0.513 to 0.252 ms
per frame. The first recording layout had regressed with relocation enabled.

Foundry repaired roughly 0.2M instead of 2.7M pixels per frame at 100%, but moving
the preserved pixels cost enough to erase the saving. A separate phase diagnostic
measured approximately 0.72 ms copying, 0.04 ms planning and 0.04 ms snapshot
recording. Native completion fell from about 0.9 to 0.2 ms. These diagnostic
numbers locate the cost; they are not another eight-run comparison.

Sampling identified glibc's `__memmove_avx_unaligned_erms`: the portable move is
SIMD accelerated, but serial. It runs before the next Painter opens and adds no
native flush. New test-only counters separate planning from copying; production
does not retain those clocks or counters.

### Thread Sensitivity

The same GUI binary and 100% scrollbar trace were measured at one and two threads
in four fresh processes each. Four-thread entries are from the eight-process run
above. One thread is Blend2D's asynchronous caller-only mode, not its synchronous
zero-thread path. These separate runs are diagnostic comparisons, not a calibrated
confidence interval or a recommendation to change application thread counts.

| Threads | Ordinary Repair | Relocate | Ordinary CPU | Relocate CPU |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 4.675 ms | 3.378 ms | 4.401 ms | 3.239 ms |
| 2 | 3.435 ms | 3.454 ms | 4.522 ms | 3.631 ms |
| 4 | 2.980 ms | 3.213 ms | 4.997 ms | 3.474 ms |

CPU entries are mean process CPU time per frame, not utilization percentages.
Copying stayed around 0.74-0.84 ms while ordinary native completion decreased
from about 2.61 to 0.90 ms. This supports treating serial transfer and parallel
replay separately in selection. A lower CPU total alone is not desktop latency
acceptance. Tail latency also remained worse with relocation at two/four threads.

## Native Transfer Investigation

Pinned Blend2D: `58ca9460b4138af6e793184682e9324e0a8053b2`.
An isolated test used one patterned `BLImage` as both source and destination,
`SRC_COPY`, integer `blit_image`, then `end()` before comparison. Six overlapping
directions were tested at 0, 1, 2 and 4 threads. An untouched independent source
provided both the expected output and the control case.

Overlapping self-blits corrupted pixels. A one-pixel downward move corrupted
239,040 pixels even synchronously; multithreaded upward moves also failed.
Every independent-source control passed. The image fetch retains source storage,
not a snapshot; command order does not protect cross-band read/write hazards.
Do not replace the overlap-safe copy with an ordinary queued self-blit.

The safe transfer experiment used three existing targets, 200 warm frames per
case, eight processes and alternating policy order. Four-thread results below use
512 columns of side space outside the copied rectangle. Only the benchmark
processes were pinned; this is not a runtime scheduling recommendation.

| Copied Rectangle | Row Memmove | Previous-Image Blit | Snapshot Then Blit | Snapshot Storage |
| --- | ---: | ---: | ---: | ---: |
| 1280x960 | 0.295 ms | 0.189 ms | 0.410 ms | 4.69 MiB |
| 2048x1280 | 0.781 ms | 0.839 ms | 1.506 ms | 10.00 MiB |
| 3200x1800 | 1.887 ms | 2.069 ms | 4.002 ms | 21.97 MiB |

The previous-image path assumes a safe immutable source lease, which both current
host APIs do not provide. It has no extra image allocation. The snapshot path uses
two native contexts and includes the intermediate completion. Parallel native
copies substantially increase total CPU consumption; they do not universally
reduce wall time. Copying complete contiguous rows was also slower, even before
the additional side-area repaint it would require.

These are transfer-only experiments, not complete repair/presentation workloads.
They do not justify adding a scratch image or forking a host library. A future
independent-source capability remains useful to evaluate where actually available.

## Remaining Work

Selection must consider transfer topology, native execution, replay and available
raster alternatives rather than dirty-area reduction alone. A discovered estimator
bug assigned each expanded leaf the cost of its entire parent program; its fix and
regression test pass. The final GUI measurements above confirm that this correction
does not resolve the latency regression. Avoid tuning an arbitrary area cutoff
merely to pass these fixtures.

The next selection work needs the effective native thread allocation, command
coverage rather than whole-program area multiplied by operation count, and valid
existing raster alternatives. Keep those cost facts separate from pixel-validity
proofs. Include populated-artwork workloads before accepting a general policy;
do not infer their performance from these fixture cards.

Correctness coverage includes nested GUI pages and rails, actual acquired-buffer
generations, hover and tooltip changes, opaque callbacks, target loss and failed
submission. Fresh-render comparisons allow at most one channel unit at native
antialias clip boundaries; rectangular fixtures remain exact.

Evidence: `research/retinue/benchmarks/relocate-demand-20261103/`. The maintained
`gui-drag` and `poe2-drag` suites exercise the shared production API. The native
probes are `self-blit.cpp`, `copy-bench.cpp` and their local Makefile; final native
measurements are `copy-wide-run-*.jsonl`. Earlier `copy-run-*` used narrower side
space and must not be mixed with the table above. Snapshot manifests preserve
application source, build and binary identities; these dirty results are not
accepted baselines.

Final GUI evidence is `gui-leaf-snapshot`, `gui-leaf-measurements` and
`gui-leaf-threads{1,2}`. Binary SHA-256:
`889fb59555cbdacda4915f615a89f3d87ee1bdd90f91cc31cae27e06c9c25de6`.
Shared renderer/SDK tests, 227 GUI tests, 74 PoE presentation tests, 42 Warframe
presentation tests and Common Test passed after the leaf correction. No runtime
build was staged, reloaded or committed as part of these measurements.
