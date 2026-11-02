# Incremental Rendering

Incremental updates, command reductions and reuse-driven raster layers extend
[composition](ui-composition.md). Keep a full replay oracle and benchmark real
pages. Blend2D owns rasterization and its ordered worker queue.

## Next Work

The [render pipeline redesign](render-pipeline.md) now separates resolved paint
frames from layout. Its incremental compiler, materialization planner and
granularity experiments remain in progress. The mechanisms below describe the
current implementation, not the completed design.

Profile new page/overlay changes against matched baselines, including preparation,
native completion and retained resources. Do not enable a new optimization merely
because it reduces command counts.

## Existing Behavior

The scene reconciles stable keys. Style, layout algorithm and child-order changes
invalidate Taffy caches; paint revisions and presentation offsets do not. Taffy's
cached constraints avoid recomputing clean descendants. Paint signatures include
content revision, size and physical scale. Equal patches now skip projection too.

`update_subtree` replaces one owned description without rebuilding its siblings.
Taffy still propagates changed constraints. Compact preorder placement records
resolve dirty ranges and aggregate their ancestors; clean geometry and paint chunks
remain intact. Topology changes use a full projection. Full reconciliation remains
available as the correctness oracle.

Foundry uses scoped content replacement when its virtual window changes, and offset
patches within that window. Explicit bounded card rasters reuse pixels during
scrolling; there is no full-viewport copy or extra viewport worker pool.

## Input Diffing

Separate identity from inputs and from resource generations. An element's key
answers which instance survives; equal inputs answer what can be reused. Do not
hash closures or recursively compare entire domain objects each frame. Apple
describes updates in terms of identity and dependency changes, not a public
general-purpose tree-diff algorithm. [SwiftUI performance](https://developer.apple.com/videos/play/wwdc2023/10160/)

| Changed input | Invalidation |
| --- | --- |
| Layout style, measured text/font, intrinsic image size, child order | Layout and dependent placement; paint when local dimensions change |
| Color, selection, timer text in a fixed box, image pixels of unchanged size | Paint/resource revision only |
| Scroll or presentation translation | Placement, clipping, visibility, hit regions and display damage |
| Hit/semantic state | Interaction/accessibility; paint only if a visual changes |
| Physical scale | Geometry resolution, glyphs, image derivatives and eligible rasters |

Custom measurement must expose every changing input or a revision covering those
inputs. Function identity alone cannot represent captured or external state.
Custom paint has the same obligation through `PaintKey`. Unknown dependencies use
conservative invalidation; an optimization hint never makes unknown state safe.

Layout is not simply "mark every descendant dirty." A changed child's size can
resize ancestors or move siblings. Constraints flow down and measured sizes flow
up. Clean children receiving unchanged constraints can reuse layout. Independent,
tightly constrained regions can stop propagation, but only when the parent does
not depend on their changing measurements. Flutter documents these boundaries;
Taffy's cache already handles constraint reuse in our tree.
[Flutter layout](https://docs.flutter.dev/resources/inside-flutter),
[Taffy custom trees](https://docs.rs/taffy/0.12.2/taffy/#low-level-api)

Update discipline:

1. Use scoped rebuild boundaries with comparable inputs or immutable description
   identity. Unchanged descriptions skip construction/reconciliation, not necessary
   layout when a parent supplies different constraints.
2. Track dirty roots and reasons. Coalesce overlapping roots; reuse clean siblings.
   Adjacent ranges can have different parents and must retain their own ancestry.
   Keep custom layout conservative until measurement dependencies are explicit.
3. Retain local geometry and separate placement/clip properties. Scroll updates
   update those properties rather than re-reading every Taffy node. Resolve the
   same properties for paint, input, captures and accessibility.
4. Test against a fresh full tree: changed intrinsic sizes, wrapping, parent resize,
   reparenting, reorder, resource replacement, hidden nodes and interrupted motion.

Do not add a global reactive state system or duplicate Taffy's layout algorithms.
The dependency contract matters more than a sophisticated general tree diff.

## Moving Cached Pixels

Keep three concepts separate: changed content, required raster repair, and changed
screen pixels. Moving unchanged content changes the last without necessarily
changing the first two. This distinction is also explicit in Chromium's compositor.
[Chromium compositor](https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md)

The implementation uses isolated item rasters. `CachePolicy::Prefer` opts a paint
chunk into RAM caching. Position is not part of its content key. Changed
commands produce disjoint local repair rectangles; the cache copies the immutable
old image, clears each repair and replays all intersecting contributors, including
those behind translucent content. More than 16 regions falls back to a full item
repair. A completely unchanged item only needs a blit, including after a reorder.
Nested live paint is ineligible. Cache allocation failures fall back to replay.

Insertion/reorder should move stable item rasters, not blindly slice the current
window framebuffer. Reuse needs valid content/resource revisions, physical size,
sampling phase and composition state. Out-of-bounds shadows, overlapping children
and backdrop-dependent effects can forbid independent item reuse. A row cache
simplifies vertical insertion but makes unrelated updates within that row larger.

Use owned cache surfaces. A recycled presentation buffer may contain an older
frame. Never mutate an immutable image retained by queued Blend2D work. A future
writable tile must finish writes before publishing its image and retain the old
generation until consumers finish. Fractional scroll, scale changes or unsupported
effects fall back to replay unless separately proven equivalent.

Host invalidation and buffer history also retain disjoint regions. Account for
allocation, repaired pixels, copies, cache RAM,
blits and final composition. Foundry allows 32 MiB of warm raster retention and
includes raster residency in Settings memory usage/clearing. Selected rasters remain
plan-owned even when their total exceeds warm retention. Rebind surviving consumers
before releasing previous plan ownership. The materialization store accounts those
consumers directly, separately from queued native readers. A zero warm allowance
keeps selected rasterization enabled; changing scene identity clears
rasters because local keys/revisions alone do not identify another scene's content.
Rasters, command storage and held resources have bounded lifetimes;
shared source images remain accounted by their existing resource cache.
At 2560x1440 one BGRA surface is about 14 MiB; copying
it every frame is not free. Firefox's retained slices/tiles are useful precedent,
not evidence that a CPU cache will beat our renderer.
[Firefox rendering](https://firefox-source-docs.mozilla.org/gfx/RenderingOverview.html)

Live residency and bounded keep-alive retention are also separate in
[Flutter's image cache](https://api.flutter.dev/flutter/painting/ImageCache-class.html).

## Target Repair

Keep this sequence explicit: frame changes, pending damage, acquired-buffer
history, repair regions, rendering, presentation. `Repaint::damage` describes
the current displayed change; `Repaint::repair` also includes changes missed by
the acquired buffer. Never report the latter as compositor damage just because
an older buffer required more repair.

`Damage::union` preserves gaps. `Region` normalizes overlapping rectangles into
disjoint coverage, so translucent draws cannot be applied twice. Excessively
fragmented invalidations use bounded conservative tiles; this is a storage/work
bound, not a tuned repaint-cost heuristic. Intersections used for clipping never
expand coverage. Use `bounds()` only when a bounding rectangle is actually needed.

Set the target repair with `Painter::repair`. Prepare layout, programs and caches
once, then submit through one target context and finish once. Playback skips
nonintersecting instances and commands; the backend clips each surviving draw to
the active disjoint regions. External paint callbacks still execute once per
source occurrence, not once per damaged rectangle. Scoped clips and translations
must preserve target repair.

Blend2D chooses horizontal bands from target dimensions and worker count, not
from our damage rectangles. Its workers process interleaved bands and skip
irrelevant commands. Geometry jobs use a separate shared job index. These are
not reasons to repaint entire horizontal bands or merge separated dirty areas.
Verify the pinned `rastercontext.cpp` and `workerproc.cpp` when upgrading; see
[Blend2D multithreading](https://blend2d.com/doc/multithreaded-rendering.html).

Chrome still recorded during paint must invalidate both old and new layout
envelopes when resized. A previous recorded program cannot predict the bounds of
new commands. Prefer declaring size-dependent paint before damage planning.

`make bench-repair REPAIR_REPORT=research/repair.jsonl` compares exact command-first
clipping, region-first replay, coarser regions, bounding repair and full repaint.
It measures preparation, context creation, submission, completion and CPU time
separately, with replayed/cached content and 100/125/200% scales. Run without other
builds or benchmarks. Queued thread count 1 is not the synchronous backend;
native synchronous controls must explicitly use Blend2D thread count 0.
These measurements exclude presentation and game effects. Cost-based adaptive
coalescing remains a policy extension; do not invent thresholds from dirty-area
percentage alone.

## Draw-Tree Optimization

Enabled local reductions retain the same ordered Blend2D commands:

- `Surface` reconciles section layout only when size, keys, order or bounds change.
  Paint revisions still update independently. No subtree is assumed static.
- `Paint::commands` shares immutable fragments with the scene, without cloning
  their strings and command arrays into another retained representation.
- Partially damaged chunks skip transparent fills/text/inset shadows and rectangular
  draws outside the repair clip. Full-chunk replay avoids these extra checks.
  Empty clips skip their children. Line, ring, polygon
  and stroke edge bounds are not used for this reduction; preserve antialiasing.
- Translation remains level-by-level at device-pixel rounding. Combining floating
  offsets before rounding can move pixels at fractional scales.

`PaintStats` reports submitted and culled command roots. `Renderer::set_optimizations`
disables all optional rasterization and replay reductions for pixel oracles.
`set_materializations(false)` keeps command compilation enabled but disables
optional rasters, including nested fragments, for a compiled-only comparison.
Benchmark builds additionally accept
`RETINUE_BENCH_FULL_LAYOUT=1` and `RETINUE_BENCH_FULL_REPLAY=1` for matched comparisons.
Native completion must be included in wall time; layout savings alone are not an
end-to-end rendering speedup.

Recorded paint also caches a compiled command stream. It removes transparent or
empty work, zero translations and fully covered fills/text/images. Opaque coverage
must come from a solid rectangular fill; rounded/translucent bounds prove nothing.
External callbacks are observation barriers. Fractional nested translations retain
their separate rounding. Unchanged command vectors remain shared, with no copy.

Named `Command::Fragment` sources share compilation and optional raster preparation
below layout/item boundaries. `OptimizationContext` on fragments/groups holds
extensible typed hints; unknown annotations do not affect pixels. Group hint edits
update the planning journal without inspecting every paint instance. Nested hint
changes participate in source reconciliation, but recipe repair compares actual
paint separately so unchanged pixels need no redraw. See the
[context contract](render-pipeline.md#optimization-context).

`TileHint` lets the renderer propose spatial coverage without replacing the
collection's paint order. Ready tiles mask only their own output regions from
source/item replay. Missing or failed producers keep the gaps on the source path;
parent preparation uses the same rule. `PaintStats::preparation_blits` counts
successful region substitutions during preparation. Row/tile extent remains a
policy input to measure, not a mandated scroll-container bitmap.

`Element::cache_hint` or `Recording::cached_section` marks a candidate subtree.
`Stable` and `Conditional` are reuse hints; `Dynamic` requests no layer. They never
replace invalidation. The renderer counts actual drawn frames, not idle prepares,
and admits complex, repeatedly reused candidates. Tiny partial repairs do not count
as full-subtree reuse. Already-rasterized children count
as blits, not their former glyph/shadow work. Large sparse groups of cached cards
failed end-to-end benchmarks and remain separate.

`Renderer::with_layers` and `Surface::with_layers` take `ResidencyPolicy` and
`LayerPolicy`. Unconfigured renderers keep only explicit leaf caching. Initial
guardrails for the shared item/layer store:

| Host | Peak Pixels | Single Surface | Stable/Conditional Repaints |
| --- | ---: | ---: | ---: |
| GUI | 128 MiB | 12 MiB | 3 / 8 |
| Overlay | 8 MiB | 512 KiB | 12 / 24 |

Peak admission includes selected, warm, retired and pending output pixels in one
renderer, not all process memory. Host-wide aggregation and native scratch remain
follow-up work. The warm allowance is separate; zero does not disable selected
rasters. These are safety guardrails, not benchmark-tuned limits or asset eviction
quotas. Validate hints at the supported scales: larger text-layer blits can lose
even when command submission gets cheaper. Selected dirty layers are repaired
first, subject to peak admission; at most one new speculative layer is promoted
per frame. Signatures include content
revision, resources, scale, relative placement, painter order and effective clips.
Moving a whole unclipped subtree can reuse its raster; changing visibility cannot
reuse missing pixels. External paint is ineligible. Failed preparation falls back
to direct painting. Frequent invalidation adds a cooldown.

`PaintFrame` also accepts advisory `CollectionContext`: a content origin, optional
row partitions and motion state. Row rasters retain full paint bounds and repair
clip changes locally. Reordering discourages whole rows/ancestors while allowing
independent cell reuse. This lower-level API is tested independently; current
layout callers do not yet emit it. See [raster planning](render-pipeline.md#raster-planning).

A ready parent releases redundant child layers and explicit rasters, except leaves
also used outside that parent. Hidden, previously drawn candidates can retain warm
pixels briefly; removed or invalid content is discarded. Warm entries do not retain
source programs. Hosts call `clear_cache` when a page/panel closes; `trim_inactive`
releases warm storage without disturbing selected work. No disk I/O or idle scanner.
`residency_stats` separates selected/warm/retired/pending pixels. Paint stats report
changed plan entries, inspected groups, promotions, preparation time and simplified
commands. `set_optimizations(false)` selects raw replay and disables promotion;
benchmark builds also accept `RETINUE_BENCH_NO_LAYERS=1`.

Analysis is local and cached; layer preparation currently completes synchronously
through Blend2D's queued renderer, with the host's bounded thread policy. There is
no background optimizer. Larger or more expensive work needs a measured case and
a frozen, thread-safe resource/text snapshot first: live `Rc` paint/font caches
cannot be sent to a worker. Such a worker must keep direct painting available,
coalesce requests, bound in-flight bytes and reject stale results.

Do not reorder/fuse commands merely to reduce counts. Painter order, antialiasing,
clipping and alpha must remain equivalent, and Blend2D already bins ordered work
into bands. [Blend2D threading](https://blend2d.com/doc/multithreaded-rendering.html)

Motion can make an unchanged subtree worth caching; it does not make changing
paint static. Build an isolated layer from that subtree, not a crop of the final
framebuffer containing unrelated overlays or aged pixels. Include preparation,
invalidation, short-animation break-even and retained bytes in comparisons. The
`subtree_motion_benchmark` compares an explicit layer with existing card rasters;
`adaptive_motion_benchmark` measures admission, invalidation, preparation and reuse
against full replay. Fewer commands or visible cards need not mean faster painting.

## Validation Order

First remove measured tree/submission waste. Then prototype retained placement and
input boundaries. Next compare replay against tiles/items with simultaneous scroll
and sparse updates, insertion/reorder and rotating destination buffers. Only after
that evaluate asynchronous promotion. Each stage keeps a full-paint oracle and
reports wall time, caller/process CPU, memory and invalidation/cache counters.
