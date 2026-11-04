# Relocate And Raster Caching

Status: shared Wayland surface integration is implemented and verified with pixel,
protocol and isolated-compositor tests. See the
[integrated measurements](../../tools/benchmarks/history/2026-11-03-native-relocate.md)
for gains and regressions by workload/thread count. Live game FPS is not verified.
Use the [copy-executor follow-up](../../tools/benchmarks/history/2026-11-03-copy-executors.md)
for corrected GUI comparisons and the decision to keep queued native copying.
The [reference-architecture study](../../tools/benchmarks/history/2026-11-03-reference-scroll.md)
compares exact no-relocation controls and actual Qt/Blend2D CPU paths.
The [effective-cost follow-up](../../tools/benchmarks/history/2026-11-03-effective-cost.md)
measures cache-aware admission and its small remaining metadata cost.
The earlier prototype measurements are not acceptance results for this path.
See the [prototype measurements](../../tools/benchmarks/history/2026-11-03-target-reuse.md)
and [production investigation](../../tools/benchmarks/history/2026-11-03-relocate-production.md).
The [independent-source comparison](../../tools/benchmarks/history/2026-11-03-independent-source.md)
covers queued blits, scratch snapshots and final presentation-copy costs.
This extends the [render pipeline](render-pipeline.md), not the UI authoring API.

## Decision

Treat valid pixels in an acquired presentation buffer as an input to repair planning.
Do not create another viewport image merely to translate unchanged content. Retain
raster materializations when they reduce the remaining drawing work: repeated
fragments, expensive exposed content, independent composition and required effects.
Neither target reuse nor raster caching is an unconditional first choice.

Relocate is the name of the completed-target pixel-movement optimization. Historical
benchmark selectors/reports retain their original `target_reuse` names.

Both hosts draw directly into presentation memory. Wayland desktop and overlays
share SHM storage that can lease a released writable target and an independent,
read-only previously submitted source. Copies and repair use one Blend2D context.
X11 retains softbuffer and overlap-safe in-place movement before painting. No
snapshot image, offscreen presentation copy or Blend2D-private queue access is used.

| Alternative | Decision |
| --- | --- |
| Move acquired target pixels, then repair | Fallback when no independent source is available |
| Raster-cache each moving viewport/window | Optional when its independent pixels and cheaper replay justify the allocation |
| Read another completed presentation buffer | Shared Wayland backend capability; queue copies before repair |
| Queue a self-blit in Blend2D | Do not assume source retention snapshots pixels or makes overlapping writes safe |
| Move a dedicated native overlay surface | Separate platform optimization; does not solve scroll reuse and changes popup/input/surface handling |
| Split every move around dirty holes | Not a default; extra copies and source dependencies can outweigh saved bytes |

## Plan At Surface Scope

The buffer contains the final composition, not isolated leaf images. A page-local
renderer cannot prove reuse while ignoring a tooltip, fixed header or sibling
renderer that also painted those pixels.

```text
record current ordered surface contributions
  -> acquire target and recover its content generation
  -> validate candidate moves against the available source generation
  -> choose moves and remaining repair regions
  -> queue independent-source copies, or move pixels in-place before the context
  -> prepare materializations needed by those repairs
  -> restore/replay repairs in painter order
  -> finish, present current-frame damage, commit target evidence
```

`retinue-render` owns validation, copy scheduling, repair selection and cache demand.
`retinue-ui` owns target acquisition, buffer age/release and native submission.
`surface::Buffered` and `surface::Aged` share rendering and completion bookkeeping;
hosts supply a prepared `surface::Snapshot` and acknowledge its `Submission` only
after successful native presentation. Abandoned or failed work invalidates evidence.
`RenderHost` coordinates preparation budgets; it does not own native handles.
Games supply view state and prepared content, not buffer-copy algorithms.

Recording and resource publication finish before planning. Desktop and overlay
hosts ask for prepared content before rendering. Nested GUI page renderers expose
their recorded frames in surface order. Opaque editor callbacks remain bounded
repair barriers. No mid-frame native flush is needed.

### Host Boundary

Views, layout, state and paint programs remain platform-independent. The SDK's
feature-gated `desktop` and `surface` modules integrate native windows and Wayland
overlays. A custom host, such as Warframe's game-aware lifecycle, uses the same
acquired-buffer renderer. It does not duplicate optimization selection, target
history or failure handling. Platform protocols stay out of `retinue-render`.

Relocation requires no view modifier or app opt-in. The shared renderer discovers
compatible movement from prepared programs and placements. A custom procedural
surface can omit a snapshot and retain ordinary repair; incomplete evidence must
never be presented as the whole surface. Direct optimizer access is a lower-level
tool, not the normal app integration.

Hosts can set `surface::Relocation::Disabled` to retain ordinary repair without
discarding damage or target history. The desktop and overlay `Application` traits
expose the same policy; no view needs to know which transfer backend is available.

Reuse existing immutable paint programs, instance/property identities, frame
journals, group context and effect analysis. A surface contribution names its frame,
placement, clip and ordered position. Recorded nested frames can participate;
unrecorded callbacks remain conservative repair barriers in their affected regions.
There is no need to flatten every source command into a second copied tree.

## Validity And Damage

- Target evidence identifies the actual content generation, dimensions, scale,
  format, backdrop and ordered contributing paint. Recover it by slot identity or
  successful submission history and buffer age, never an assumed rotation.
- Compare the acquired generation directly with the current frame. An age-three
  target must not apply only the latest frame's movement or dirty rectangles.
- Initially accept integer device-pixel translations with compatible raster phase.
  This is an optimization restriction, not a new rounding rule for layout.
- Hints identify candidates; program/resource versions, clips, order and backdrop
  establish validity. Unknown content is repaired, not interpreted as unchanged.
- Account for changed/added/deleted paint, both old and new hover locations,
  shifted old occluders and current occluders. Expand invalid regions for effects
  whose declared reads or writes extend outside primitive geometry.
- Translucency is not automatically disqualifying. Moving a translucent panel over
  a constant transparent surface backdrop can preserve its final pixels. Moving
  pixels already blended with a different GUI background cannot, unless the result
  is independently proven equal or the affected area is repaired.
- Repaired pixels are restored and fully replayed, not painted over stale output
  with another source-over blend. Coalescing may enlarge repair; it must never
  enlarge the region claimed valid for copying.
- Keep current-frame compositor damage separate from target-age repair. Moving
  pixels saves application work; it does not make the changed display area disappear.
- New buffers, lost history, resize/remap and uncertain completion invalidate the
  relevant evidence. Failure after any writes must not leave a reusable old state.
  Publish successful evidence only through the host's completion/submission path.

The portable copy uses overlap-safe row movement before creating the target
`Painter`. Finish all copies before any clearing or drawing that could overwrite
another copy's source. Multiple moves need source/write dependency ordering;
conflicting or cyclic candidates can fall back to repaint without a scratch image.
Do not accept arbitrary cell permutations just because their rectangles are known.

Wayland forbids modifying buffer storage while the compositor uses it. Shared
read access needs a genuine immutable lease and valid mappings through native
completion. `retinue-ui::presentation_buffers` owns one SCTK pool per slot, splits
the owners for disjoint source/target borrows, and gates target access on release.
Only its read-only source borrow uses SCTK's raw mapping access. A busy source can
be read concurrently with the compositor, never overwritten or resized.
[Wayland buffer lifetime](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_buffer-event-release)

Pools grow on demand to at most three slots. Prefer a released target different
from the last submitted source; fall back to a released same slot without source
aliasing. Resize clears pool/history together; geometry mismatch is an error.
Desktop uses Winit's existing Wayland display/surface, not a second window or a
softbuffer fork. X11 keeps softbuffer's acquired-buffer API. Hosts may disable
relocation without forcing a second source allocation. Views never manage pools.

### Independent Sources

The shared queued execution path uses a borrowed `painter::PixelSource`
and exact `SRC_COPY` blits. Source and target must have separate backing storage;
two image handles sharing pixels do not suffice. The source stays immutable until
native completion. Queue relocation copies first, then remaining repairs in the
same target context. Reconcile the acquired target's age even when the source is
the latest completed frame. Only indices/leases swap; no full-frame duplication
is required.

`surface::experiment` compares the production queued path with in-place moves,
full or source-region snapshots, and ordinary repair. It is compiled only for
tests/benchmarks and does not model compositor release timing.
A separate offscreen pair also needs presentation copies; measure those rather
than counting only its rendering work.

Dirty submission does not replace content validity: all pixels of the attached
buffer must represent the current image. Age tracking identifies which parts need
repair or copying; the rest can remain untouched. Qt's Wayland backing store uses
the same broad pattern, reconciling stale back-buffer regions from its front buffer.
[Qt backing store source](https://github.com/qt/qtbase/blob/dev/src/plugins/platforms/wayland/qwaylandshmbackingstore.cpp),
[EGL buffer age](https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_buffer_age.txt),
[Wayland damage](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_surface-request-damage_buffer).

A cropped or packed scratchpad is a separate candidate. Its contents need not use
screen coordinates, but source placement, validity and outstanding native reads
still need tracking. Do not allocate one solely because relocation is enabled.

## Selection And Memory

Compare a small, bounded set of plans: ordinary repair, a whole move plus holes,
and limited split moves where meaningful. Include planning, copied bytes/run count,
repair replay, native completion and optional cache preparation. Dirty-area ratio
alone is insufficient; a small hole can contain more drawing work than a large fill.
Calibrate estimates from replay experiments, not always-on production timings.

`Snapshot::record_rendered` adds a read-only projection of the renderer's available
materializations. Whole-item caches and selected fragments cost a blit, not their
original source commands. Valid cached layers/rows/tiles replace their covered
child work; remaining fragments retain their own clipped geometry. Layer evidence
is checked against current content, placement and clips before it discounts work.
Unknown, invalid or changed materializations use replay cost. Plain `record` is
available to hosts without residency evidence.

Work summaries stay inline on paint records; only fragment/layer coverage needs
extra rectangles. They neither change content signatures nor retain images,
touch warm-cache expiry or prepare rasters. Immutable programs summarize solid,
blended and sampled pixels, command setup, edges and glyphs once. Partial repair
uses coverage-weighted summaries, not a command walk for every candidate.

The execution model ranks alternatives; it does not predict milliseconds. The
planner cancels common replay work, compares avoided work with transfer traffic
and setup, and requires a margin before selecting relocation. Worker scaling and
weights come from [warm native experiments](../../tools/benchmarks/history/2026-11-04-cost-calibration.md),
with held-out workloads checked for decision regret. Pixel validity is independent
of this advisory score. A large reduction in repaired area is not a latency guarantee.
Cold preparation and cache admission still have separate policy; this is not a
universal hardware timing model. Benchmark builds can omit resident substitutions
with `RETINUE_BENCH_RAW_REPAIR_COST`; shipping builds contain no such switch.

Related execution coefficients live together in `cost::WARM_NATIVE`, with named
work units and calibration provenance. Local policy, such as the relocation margin,
stays in the selecting planner. There is no global registry of unrelated constants.
This follows the separation of relative costs and plan choice used by
[PostgreSQL](https://www.postgresql.org/docs/current/runtime-config-query.html#RUNTIME-CONFIG-QUERY-CONSTANTS)
and target cost queries in
[LLVM](https://llvm.org/docs/doxygen/TargetTransformInfo_8h_source.html), not their
specific cost values or a full instruction-throughput simulation.

LLVM's [loop-vectorizer profitability checks](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Transforms/Vectorize/LoopVectorize.cpp)
also account for setup and minimum profitable reuse, not only operation costs.
That distinction applies to future cache admission calibration: warm blit savings
must repay materialization. Separating structural features from execution choices,
as in the [Halide autoscheduler](https://halide-lang.org/papers/halide_autoscheduler_2019.pdf),
lets stable program analysis survive movement. These are design references, not a
dependency on either compiler or an instruction simulator in the frame loop.

Cache preparation receives the remaining repair demand through `Renderer::prepare_for`.
The ordinary `prepare` entry point requests full coverage. Count actual
repair consumers and repeated draws, not every translated visible instance as a
cache benefit. Defer a dirty cache that is not needed by this plan rather than
repairing it speculatively or accidentally declaring it clean.

### Three Independent Questions

| Question | Evidence | Consequence |
| --- | --- | --- |
| Is this raster valid? | Source/resource versions, scale, geometry and effect semantics | Invalid pixels cannot be sampled, even if their allocation remains a repair input |
| Is keeping it useful? | Retained replay/repair consumers, repeated uses, preparation cost and expected reuse | Keep or explicitly demote the representation; not inferred from a single frame's reads |
| Is it needed now? | Remaining target repairs and preparation dependencies | Schedule current work and native leases; absence is not an eviction signal |

The retained draw plan describes all current content, including content whose pixels
relocate will preserve. Relocate restricts execution demand; it does not remove
those consumers, source fragments or the direct-replay fallback. An unchanged shared
glow therefore remains available for the next exposed strip or hover repair even
through many frames that never sample it. This is a plan consumer, not a warm-cache
entry kept alive by artificially refreshing an expiry timer.

New optional rasters need actual paint demand before preparation. Waiting for that
demand must not schedule another repaint or poll. A larger repair can activate them
without a new scene revision. Source changes still invalidate obsolete versions;
removing consumers still releases their ownership, including during empty repairs.
Dirty unrequested layers stay invalid and replay their current source when needed.

Source reachability is not permission to pin every possible raster. Existing plan
selection, shared independent consumers, parent substitution, peak admission and warm
retention still apply. A parent that permanently replaces a child is a different
case from relocate skipping this frame's painting. Re-evaluate representation utility
when coverage, source dependencies or motion context change; do not restrict caches
to repeated decorations if a larger isolated representation demonstrably wins.

The Warframe glow is explicitly authored as a named `Command::Fragment` with a
`Stable` hint in `presentation/style.rs`. Program interning shares identical source;
the renderer counts live and currently required uses separately. Cost thresholds
and allocation admission can reject the hint. Arbitrary repeated command-sequence
discovery is not implemented, and cost weights are heuristics rather than live timers.

Retain small shared fragments when they make edge/dirty-hole painting cheaper.
Compare an already-built row/viewport raster against target reuse without charging
its sunk construction cost again. Demote a movement-only raster when the alternate
plan wins over the expected motion interval; use hysteresis to avoid oscillation.
Independent consumers, queued reads and required isolation surfaces still retain
their allocations. Follow the existing [residency rules](render-residency.md).

Target evidence is compact version/coverage metadata, not a retained source graph
or pixel cache. Bound its history to backend needs; do not pin old decoded assets.
Presentation storage remains host-owned and must be accounted separately from
optional rasters. Removing an unused 1800x1360 RGBA raster would save 9.34 MiB;
that arithmetic is not a measured application-wide RSS reduction. Copy bandwidth,
metadata and any retained caches remain costs.

## Integration Order

1. Separate surface recording from execution; collect all ordered contributors
   before acquiring/planning the target. Preserve current output and callback paths.
2. Extract the tested planner from the benchmark-owned `Target` into borrowed-target
   execution and compact generation history. Integrate safe scrolling and isolated
   overlay movement; preserve ordinary repair as a per-region fallback.
3. Feed remaining repair demand into raster preparation/admission, then evaluate
   retaining or demoting movement-only allocations. Keep shared fragment reuse.
   The demand/retention separation and shared surface integration are implemented.
   Integrated benchmarks and native-host correctness checks are complete.
   Live performance acceptance and calibrated demotion remain separate follow-ups.
4. Measure real scroll/hover and overlay traces, including substantial caches,
   rotating/aged buffers, fractional placement fallback and sparse/large mutations.
   Compare whole versus split copies before adding fragmentation complexity.
5. Only after full reuse acceptance, resume the mouse/event/hit-test architecture audit.

Correctness gates compare against direct rendering and cover failure/ownership
paths. Benchmarks report preparation, copy, replay/completion, metadata and unique
pixel storage separately; live presentation/game impact is a separate acceptance.
