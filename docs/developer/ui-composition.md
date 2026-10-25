# Declarative UI Composition

Design for the next `retinue-ui` iteration. Not implemented yet. Validate a small
prototype before migrating the GUI; keep Blend2D, existing hosts and resource
workers. Application code owns visuals and actions, not the composition engine.

## Model

Declarative Rust descriptions reconcile into retained, keyed elements. Elements
hold layout and presentation state, paint chunks and resource dependencies.
Layout produces geometry; painting records ordered primitives; execution lowers
those primitives to the existing Blend2D queue. Reconciliation and animation do
not rebuild unchanged descendants or construct every item in a large collection.

HStack, VStack, ZStack, Grid, Anchor and custom layouts are first-class. They share
a contract for measuring children, choosing size and placing keyed children.
Custom layouts can contain stacks and vice versa. Layout-only nodes emit no paint.
Anchored badges can be excluded from parent sizing. Use Taffy for supported flow
and grid algorithms, not a new general constraint solver. Reject anchor cycles.

Maintain separate layout, content/optical, paint and hit bounds. Keep signed,
fractional logical coordinates until backend resolution. Paint, input,
accessibility, previews and inspection use the same resolved geometry.

Composition functions may define layout, drawing or both. Custom paint normally
emits primitives through a scoped context. Lower-level escape hatches must report
bounds and dependencies or accept conservative invalidation. They must not need
their own focus, scheduling, caching or damage implementation.

The session context owns identities, resources, dirty state and timelines. Scoped
layout/paint contexts expose the relevant services, not unrestricted global state.
No hidden IO or per-element workers. Keep resources alive until queued work ends.
Host teardown and companion reload retain their existing lifecycle boundaries.

## Animation

Keep model targets separate from sampled presentation values. A transition binds
to a stable element/property key, not a temporary description or list index.
State changes form a transaction with one start time so coordinated properties
start together. Retarget from the current presentation value, not the old target.

| Change | Required work |
| --- | --- |
| Width, height, spacing, font size | Recompute affected layout and dependent placement; update paint as needed |
| Translation, scroll offset | Reuse local layout and paint chunks; update transforms, visibility, hit mapping and damage |
| Color, stroke, progress, shape | Re-record the affected paint chunk, normally without layout |
| Group opacity or effects | Reuse child chunks; update composition, with an intermediate image when required |
| Image/font/resource replacement | Invalidate dependent chunks or raster entries; preserve old valid images until replacements arrive |

Animation policies are explicit. Do not animate every state difference or turn
price updates and sorting into unsolicited motion. Provide reusable coordinated
transitions, initially the existing category-strip expansion/collapse. Custom
properties supply interpolation and invalidation classification. Shape morphing
requires compatible geometry or an explicit interpolator, not command guessing.

An exiting element may retain a paint-only representation until its transition
ends. It must no longer receive input or accessibility focus. Reduced motion
applies final values without visual timers. Hidden surfaces skip visual sampling
and redraw; presentation resumes from the timeline, without replaying missed
frames. Keep control deadlines independent of visual deadlines.

Sample active timelines on the existing compositor-paced redraw path. Do not
visit every node each frame to discover animations. Update only active channels
and their dependents; stop requesting frames when they settle.

### Backend Constraints

A translation-only update avoids layout/recording, not necessarily rasterization:
the CPU renderer still replays commands unless a valid raster is reused.
Likewise, group opacity is not equivalent to multiplying each child's alpha when
children overlap. Preserve an explicit isolation/effect boundary; cache eviction
falls back to equivalent uncached rendering, never different blending semantics.

The pinned Blend2D A8 mask path requires pixel-aligned translation. Keep animation
state fractional, but resolve glyph/mask placement using the existing snapping
policy. Do not pass unsupported arbitrary transforms into it. A transformed-image
or alternate drawing path needs separate quality and cost validation; see
[mask limitations](blend2d-issues.md). UI scale still selects physical derivatives.

## Display Representation

Keep ordered paint chunks in local coordinates. Each chunk references its
transform, clip/effect state and resources, with a content revision and accurate
paint extent. These properties can change without rebuilding its command payload.
Start with simple shared property records; do not copy a browser's entire
compositing architecture or serialize a new public protocol.

Dirty reasons distinguish layout, paint, composition, interaction/semantics and
resources. Damage includes old and new painted extents, removal and affected
siblings. Shadows and antialiasing expand those extents. Repair every contributor
to damaged pixels, not only changed elements. Preserve existing buffer-age repair
separately from new compositor damage, including failed/deferred presentation.

Backend execution preserves painter order. Do not create a Blend2D context per
element or parallelize each element independently; its queued renderer already
distributes raster work. Resource preparation remains outside frame painting.

## Optimization Passes

Every pass must state its equivalence conditions and expose counters. Keep an
unoptimized full-paint path as a correctness oracle, not a second UI implementation.

1. Reuse unchanged layout and command chunks by identity and dependencies.
2. Cull chunks outside clips/viewports using paint bounds, not only layout bounds.
3. Eliminate provably redundant state operations and empty work without changing
   clipping, ordering, alpha or blend semantics.
4. Reuse explicitly identified immutable resources and raster groups.
5. Evaluate automatic raster promotion and scroll-pixel reuse after measurement.

General command-pattern mining, arbitrary reordering and speculative occlusion
analysis are not initial requirements. A known opaque rectangle may eventually
prove coverage; a rounded or translucent element cannot prove full coverage from
its bounding box alone. Custom opaque callbacks form optimization barriers.

### Raster Promotion

Use the existing bounded RAM cache and immutable resource handles. Provide a
policy such as Never, Auto or Prefer; Prefer is a hint, not permission to violate
compositing or memory constraints. Required intermediate effect surfaces are
semantically distinct from optional persistent caches.

Start with explicit expensive compositions such as frames/glows. Automatic
promotion can use stable content, sampled replay cost, reuse frequency and image
size. Include preparation and blit cost in the decision. Add hysteresis/backoff
so animated or frequently invalidated content does not oscillate between modes.
No global scan/hash of all drawing commands on every frame.

Raster keys include content and resource generations, dimensions, physical scale,
sampling and relevant paint/effect state. Position-only changes should not break
a position-independent cache. Fractional phase or backdrop dependence may prevent
that equivalence. A parent opacity applied after an isolated raster need not
invalidate its contents. Resources referenced by chunks also invalidate their
dependent raster entries. Keep the last valid image while preparing replacements.

Track cached and still-retained memory, evictions, invalidations and hits. RAM-only
is the default for composed UI. Existing source-derived image disk caches retain
their own policy; do not persist transient UI snapshots automatically.

## Scroll Containers

A scroll container owns viewport geometry, clipping, offset, extent and visible
range. Virtualized variants build/measure a bounded visible range plus overscan;
custom layouts supply their own range lookup. Stable item keys preserve the visible
anchor through sorting or data updates. Fixed headers/scrollbars remain outside
the scrolled content representation.

Always support command replay with unchanged local layout. Optional tiles or a
viewport cache may reuse pixels when beneficial. The container chooses a supported
strategy; callers should not implement copy regions or buffer-history repair.

Pixel reuse requires unchanged scale/clip/effect semantics and a valid source
generation. Integer translation permits copying overlap and redrawing exposed
strips. Also invalidate changed content already inside that overlap: timers,
hover, image arrivals and status updates. Store damage in content coordinates;
map it into the current viewport. A stale destination buffer must never be treated
as the previous presented frame. Fractional movement or unsupported effects use
correct replay unless a separately validated strategy exists.

Do not require scroll caches to be full-sized copies of all content. A bounded
tile cache can avoid that cost; large pixel copies can still lose to rerasterizing.
Decide from measurements, not container type alone.

## Prototype

Keep the first prototype small and outside production routing. Use ordinary Rust
APIs, with one retained layout tree and the existing painter/resource mechanisms.
Evaluate `xilem_core` only for reconciliation if it removes code without bringing
in another renderer; no dependency decision is implied by this design.

Exercise a shared animated category strip and a virtualized grid of component
artwork with anchored amount and void badges. Nest standard stacks in one custom
layout and vice versa. Include one updating counter, delayed image replacement,
an interrupted transition and overlapping children under group opacity.

Compare command replay, explicit raster reuse and eligible scroll reuse separately.
Automatic promotion initially records decisions/costs; enable it only after an
explicit-cache baseline demonstrates savings. Migration does not require a
speculative optimizer to succeed.

Test partial versus full painting through resize, fractional UI scale, removal,
reordering, cache eviction, overlapping layers and rotating buffers. Inspect
pointer/focus/accessibility geometry during motion. Test unchanged frames and
offscreen changes: no redraw or full-tree rebuild without a dependency reason.

Benchmark release builds at100/125/200% using the same data, fonts and renderer
thread policy as the baseline. Report p50/p95/p99 layout/record/paint/presentation,
missed deadlines, CPU by thread and process, allocations, live/cache memory,
nodes revisited, commands submitted and copied/damaged area. Measure timing hooks'
overhead separately. Reuse existing desktop and overlay benchmark hosts.

On success, migrate the GUI page by page, preserving workflows and approved
visual differences. Validate one overlay composition before spreading the API.
Remove superseded per-page mechanisms as their replacements are accepted; no
permanent parallel engines. UI snapshots should expose hierarchy, anchors,
resolved geometry, dirty reasons, resource generations and cache decisions.

## References

- [Xilem architecture](https://raw.githubusercontent.com/linebender/xilem/main/xilem/ARCHITECTURE.md): declarative descriptions and retained elements.
- [Flutter repaint boundaries](https://api.flutter.dev/flutter/rendering/RenderRepaintBoundary-class.html): display-list reuse and redundant-boundary costs.
- [Flutter opacity](https://api.flutter.dev/flutter/rendering/RenderOpacity-class.html): grouped opacity and intermediate images.
- [Flutter raster-cache hints](https://api.flutter.dev/flutter/widgets/CustomPaint/isComplex.html): explicit hints plus heuristics.
- [Chromium paint representation](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/graphics/paint/README.md): ordered chunks and transform/clip/effect/scroll properties.
- [Blend2D rendering](https://blend2d.com/about.html): queued, band-based raster execution.
