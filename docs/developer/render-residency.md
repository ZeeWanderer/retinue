# Render Residency

Part of the [render pipeline redesign](render-pipeline.md). The shared fragment/item/layer
store and consumer accounting are implemented. The broader cost, coverage and
pressure policy below remains the design target.

## Current Implementation

`render/residency.rs` replaces the item asset-LRU and separate layer byte budget.
`Renderer` owns one store for these representations. Each plan consumer holds a
raster identity; multiple consumers and recipes share physical allocation counts.
Parent preparation reserves its output before creating redundant child rasters,
uses existing child images, then releases only the covered consumers on publication.
An obsolete repair donor is discarded if retaining it blocks replacement.
Selected dirty layers are repaired before new speculative promotions; at most one
new layer is promoted per frame. All output reservations use the same peak limit.
Clip metadata changes outside actual paint can reuse the existing allocation
without reserving an identical output.

`ResidencyPolicy` separates maximum surface size, pending/live peak admission and
warm retention. A zero warm allowance keeps selected rasterization enabled.
Hidden, previously drawn content can enter warm storage for a short return window;
changed/removed content and redundant children are discarded. Warm entries retain
identity metadata and pixels, not source programs or their image/font dependencies.
Structural scene updates remove warm owners no longer present in the declared
scene; merely hiding a still-declared row can preserve its short return window.
When warm space is full, measured preparation time per retained byte ranks optional
replacements. This is an initial heuristic, not the full marginal-benefit model.

`ResidencyStats` separates selected, warm, retired-in-flight and pending pixels.
Weak native storage handles keep retired allocations accounted until queued
Blend2D readers finish. Reservations count outstanding output pixels before work
starts. These counters do not include native scratch, source assets or all metadata;
they are not process-RSS limits. Warm admission also charges its record metadata.
Statistics report unique pixel bytes, not the sum of every recipe reference.

Frame journals update affected instance plan entries and ancestor recipes.
Structural changes rebuild the interval index. Candidate readiness/publication
still scan group metadata; there is no claim of a fully sparse optimizer yet.
Allocation-blocked items retry after reclamation or released reservations, not on
every clean frame.

Content-space rows, tiles and named source fragments share this store. There is no
row-specific or widget-owned glow cache. Fragment preparation records actual
reads by parent tasks; publication releases redundant child consumers while
preserving outside uses. A repeatedly used fragment may enter bounded warm storage
after a parent takes over, rather than being permanently pinned by a style helper.

Paint compilation also discovers expensive primitives and dense structural command
blocks without cache annotations. Equivalent candidates share weakly interned
programs across independent view IDs. Original commands remain available; discovery
does not search arbitrary overlapping subsequences or rasterize during compilation.
Cheap fills, images and sparse blocks stay direct. The work/covered-area estimate
is deliberately coarse, not a native timing model. Text stays in its existing glyph
mask path: caching another colored alpha raster changes rounding and is not yet
eligible for automatic discovery. Explicitly authored raster policies are unchanged.

Automatic candidates need repeated visible paint consumers or several actual
repaints of an unchanged program. Idle preparation and relocation-only updates do
not advance that observation. A paint split into repair regions counts once per
preparation. Ready or scheduled child rasters reduce the parent's estimated replay
cost before admission. Discovery uses the existing reservations, preparation graph,
resource versions and warm retention, not another cache pool.

`CachePolicy::Never` disables whole-instance materialization, not reusable drawing
inside that instance. An authored fragment with `CacheHint::Dynamic` excludes its
body from automatic discovery. `set_materializations(false)` disables raster
preparation globally; `set_optimizations(false)` selects original source replay.
The component glow is ordinary drawing and no longer names or hints its cache.

Tiles retain independent selected consumers even when only part of a group fits.
Unselected coverage replays; it does not evict an active tile to rotate through
the same working set. A parent can consume several disjoint child regions and
replay the gaps. Publication releases a child only when none of its visible
coverage remains independently selected. Hidden declared tiles can use the same
short warm-return window; output revisions still have to match before reuse.

`RenderHost` adds an aggregate ceiling across sibling stores. Reservations and
unique allocations are counted once at host scope; selected/warm references can
belong to different stores. Dropping a store releases its plan references, but
retired pixels remain charged while queued native readers retain them. Reclaim by
one store allows another store's allocation-blocked items to retry. Work deferral
uses separate task/pixel/time admission, not allocation rejection or LRU eviction.

The host owns inactive image retention and indexes its deadlines. Each event loop
schedules the earliest deadline as maintenance, including while an overlay is
hidden. Expiry drops only warm references; it needs no scene traversal, paint or
recurring timer. Selected consumers and queued native readers remain valid.
Reacquisition cancels the old deadline; another consumer keeping an allocation
alive does not revive an expired entry. Stores discard bounded stale lookup
metadata on their next maintenance or teardown. `trim_inactive` can still release
warm retention immediately.

Background snapshots/results have separate bounded metadata reservations, not pixel
reservations. Published bounds indices belong to their immutable program and do
not keep source images alive independently. Failed/cancelled work releases its
reservation; it does not invalidate selected rasters or require a redraw.

Remaining work: exact native scratch accounting and calibrated benefit/pressure
demotion. Current peak admission
rejects optional new work and preserves valid direct replay; it does not evict a
selected working set to meet an asset-cache quota. Timing calibration follows the
renderer and declarative-layout implementation.

## Ownership Before Eviction

A resource lease answers whether an allocation can be freed safely. It does not
answer whether keeping that representation improves the next frame. Reference
counts include snapshots, pending repairs and other consumers, not just visible
uses. The planner must record those uses explicitly.

| Resource | Owner And Retention |
| --- | --- |
| Source paint, properties, current plan | Live scene/plan dependency graph; release when no live owner needs it |
| Cheap analysis and compiled programs | Immutable program/version lifetime; weak interning does not keep dead scenes alive |
| Expensive optional analysis | Versioned dependencies and reuse scope; cancel obsolete work and bound queued work/metadata |
| Images, sampled derivatives, glyphs | Active consumer leases plus size-bounded inactive asset storage; existing derivative disk policy |
| Selected UI rasters | Execution-plan consumers, repair inputs and explicit short-lived reuse scopes; not an asset LRU |
| Unselected reusable UI rasters | Optional bounded warm storage with expiry and a reason to expect reuse |
| Required intermediate targets | Semantic operation/task lifetime; cannot silently replace with incorrect direct drawing |
| Presentation targets and validity history | Host-owned buffers plus bounded generation metadata; not optional raster-cache entries |
| Scratch surfaces | Last native consumer completion; bounded pool of reusable empty allocations |
| External GPU surfaces | Producer/host presentation leases and synchronization; separate accounting, no implicit RAM copy |

The currently selected raster set is a working representation, not a cache that
must compete with itself for an LRU quota. A zero warm-retention allowance must
not disable active rasterization or recreate the visible working set each frame.
Likewise, an asset becoming a source of a parent raster does not imply the source
must remain decoded forever. Track its remaining replay/repair consumers.

Use explicit live scene roots and navigation retention scopes. Keeping a page's
application model must not implicitly keep its last render plan and every decoded
asset alive. Inactive declarative branches describe alternatives, not residency.

## One Materialization Store

Fragments, items, rows, tiles and subtrees use the same records and ownership rules. Their
granularity changes recipes and coverage, not the lifetime API. Keep a versioned
recipe and validity coverage separate from its surface. Original commands remain
available for fallback, repair and choosing another representation.

The plan records consumer roles: final composition, preparation input, repair
input, animation/scroll reuse, and externally retained snapshots. Each physical
allocation is accounted once even when several roles or instances reference it.
Do not infer those roles from an `Rc` count. A completion lease is non-evictable;
a planner preference is revocable at the next valid plan publication.

Materializations move through preparation, selected, warm and retired states.
Roles may overlap: an old version can be retired from the current plan but still
in flight. Retiring drops planner ownership; actual release waits for the final
consumer. Stale dependency versions cannot become selected after asynchronous work
finishes. Partially invalid coverage may remain a repair source, never valid paint.

Parent promotion is a plan transaction:

1. Evaluate the parent's marginal benefit against the existing child plan.
2. Reserve peak storage, including old and new surfaces and native scratch.
3. Prepare from reusable child surfaces and replay missing content.
4. Publish only after completion and dependency validation.
5. Release children with no remaining uses. Keep children used elsewhere; consider
   warm retention only when an independent near-term use justifies it.

Flattened parent pixels cannot generally recover an isolated translucent child.
Do not preserve every child merely because a parent might someday be demoted.
Conversely, do not discard children that are still needed by a different consumer.

## Selection And Demotion

Choose representations by expected work saved over a finite, explainable reuse
horizon. Compare against the best available alternative, not always full replay:

```text
benefit = avoided replay/repair work
        - preparation and future repair work
        - added copying/composition work
```

Use actual damaged coverage and expected presentation cadence. An idle visible
card is not repainted at monitor refresh frequency. A moving overlay may repeatedly
damage unchanged content beneath it; include that repair cost. Translation-only
motion and changing card geometry have different validity and reuse patterns.
For [relocate](render-relocate.md), admission must use the repair
work left after validated pixel moves. A cache is not useful merely because its
content moved; an existing cache may still beat copying and repairing target pixels.
Conversely, skipping its draw this frame does not remove a retained replay/repair
consumer. Validity, retention value and current execution demand are separate.

Hints describe stable/conditional/dynamic content, motion intervals, scroll
locality and likely return. They influence estimates, never correctness. Never
equate a declared inactive layout branch with a reason to rasterize it. Explicit
required surfaces remain distinct from optional cache preferences.

Retain profitable selected rasters while their consumers remain. Re-evaluate on
dependency changes, coverage/visibility changes, observed repair/reuse behavior,
motion completion, navigation and memory pressure. Do not globally rescore the
entire tree each frame. Existing property/resource journals provide affected keys.
Use hysteresis to avoid toggling around an estimate; preparation cost already paid
is not charged again when considering whether to keep an existing raster.

No single score fits both states: admission includes preparation latency and peak
storage; retention compares future reuse against holding cost; demotion compares
the next valid execution plans. A bytes-normalized score can rank candidates under
pressure, but must not automatically replace a frame-critical raster with cheap
space savings. Calibrate estimates in matched replay tests, not invented constants.

## Warm Storage And Pressure

Leaving the viewport or an animation ending removes a selection reason, not
necessarily every reuse reason. Examples: nearby scroll coverage, a likely scroll
reversal, or an explicitly retained navigation surface. Warm admission requires a
valid recipe, bounded horizon and expected recomputation saving. Ordinary unused
entries with no such evidence are released immediately.

Warm storage has a byte ceiling, expiry and no disk backing. Admit only into
available space or by replacing a lower-value warm entry. Do not evict selected
resources to retain an inactive one. Expire on maintenance/opportunistic updates
and coarse idle deadlines, not a polling loop. On pressure, drop warm state and
speculative work first. Keep small dependency-scoped history if useful for avoiding
immediate re-promotion; do not retain every obsolete program/version with it.

Memory is still finite. Use host-specific admission and peak-allocation guardrails,
not unlimited pinning. Stop new speculation before disturbing selected work. Under
sustained pressure, select a cheaper valid plan at a frame boundary: demote the
least useful optional materializations, choose smaller coverage, or replay. Do not
evict in-flight surfaces, thrash the same active set, or silently degrade required
semantics. GUI and overlays share mechanisms but use different work/pressure
policies; overlay memory is more constrained.

Use explicit allocation counters and host pressure signals, not process RSS alone:
allocator arenas and native high-water allocations can remain resident after useful
objects are released. Repeated eviction in response to that RSS would cause churn
without releasing those arenas.

Allocation accounting includes source leases, plan/analysis metadata, selected,
warm, retired-in-flight, pending outputs and scratch. Source and output references
must not double-count a shared allocation. A warm entry retaining a repair recipe
may retain substantial assets: charge those exclusive dependencies or release the
repair recipe when only exact reuse is intended. No decoded-original leak hidden
behind a small thumbnail or raster byte count.

Currently paint commands hold strong image/font handles. Do not claim their decoded
payload was freed while those commands remain alive. Separate immutable source
identity/metadata from a resident payload only for resources with a valid reload or
reconstruction path; preparation must acquire that payload before replay, never
decode or perform I/O inside drawing. In-memory-only sources remain real dependencies.
This is a later resource-store change if measurements justify it, not permission to
drop source handles and make fallback unreliable.

## Integration And Verification

Keep `ResourceCache` for assets and the shared materialization store for plan
rasters. Do not add a second item/layer eviction policy beside it.
Separate raster enablement, selected-plan
admission, pending peak limits and warm byte allowance in the host configuration.
Expose why an allocation is alive and why a candidate was selected, rejected or
retired; instrumentation must be bounded and opt-in where costly.

Tests must cover active sets exceeding/with zero warm allowance, cold/forward/
reverse scrolling, nested promotion with outside consumers, navigation teardown,
motion completion, invalid dependency versions, allocation failure, queued native
readers and pressure-driven plan changes. Verify that memory stops growing across
repeated cycles and no retired plan pins unrelated assets. Preserve direct-replay
pixel oracles, scale/clip correctness and immutable snapshots. Profile timing after
the renderer and declarative layout work, per the implementation sequence.

## References

[Chromium's compositor](https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md)
separates paint inputs, property trees, visibility and raster tasks.
[Firefox's rendering overview](https://firefox-source-docs.mozilla.org/gfx/RenderingOverview.html)
describes picture caching and partial invalidation. Flutter's
[raster cache lifecycle](https://github.com/flutter/engine/blob/main/flow/raster_cache.h)
tracks current-frame uses and preparation limits separately. These inform the
ownership boundaries; the cost/retention policy above is specific to Retinue's CPU
renderer, not a policy borrowed verbatim from a GPU engine.
