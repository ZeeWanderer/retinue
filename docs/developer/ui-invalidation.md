# Incremental Rendering

Incremental updates and explicit item-raster reuse extend [composition](ui-composition.md).
Automatic background optimization is deferred. Keep a full replay oracle and
benchmark real pages. Blend2D owns rasterization and its ordered worker queue.

## Next Work

1. Profile current GUI/overlay regressions against matched baselines. Separate
   description/layout work, command recording/submission, native completion and
   buffer copies; fix demonstrated regressions first.
2. Implement stage 3: the ordered draw-tree optimizations below, with equivalence
   tests and measured total costs. Automatic raster promotion is only one part.

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

The bounded multi-rectangle damage set lives inside the cache. The host may
still use conservative bounding damage; cache repair does not require replacing
all host damage APIs. Account for allocation, repaired pixels, copies, cache RAM,
blits and final composition. Foundry uses 32 MiB of idle raster retention and includes
it in Settings memory usage/clearing. Active rasters remain frame-owned even when
their total exceeds retention. Preserve previous frame handles until replacements
are acquired; otherwise a viewport larger than the LRU thrashes on every scroll.
Weak lookup reuses still-live entries without keeping inactive pixels alive.
A zero raster budget disables optional caching; changing scene identity clears
rasters because local keys/revisions alone do not identify another scene's content.
Rasters, command storage and held resources have bounded lifetimes;
shared source images remain accounted by their existing resource cache.
At 2560x1440 one BGRA surface is about 14 MiB; copying
it every frame is not free. Firefox's retained slices/tiles are useful precedent,
not evidence that a CPU cache will beat our renderer.
[Firefox rendering](https://firefox-source-docs.mozilla.org/gfx/RenderingOverview.html)

Live residency and bounded keep-alive retention are also separate in
[Flutter's image cache](https://api.flutter.dev/flutter/painting/ImageCache-class.html).

## Deferred Draw-Tree Optimization

Deferred. No analyzer, automatic promotion, or additional worker is enabled.

The scope is useful ordered draw-tree reductions, not only raster promotion:

1. Remove empty work, identity transforms and redundant state scopes.
2. Cull by proven paint bounds; occlusion requires proven opaque coverage, not
   merely an overlapping rounded or translucent bounding box.
3. Reuse stable command fragments by identity, comparable inputs and resource
   generations. Do not hash or pattern-match the entire scene every frame.
4. Promote/demote raster groups using measured cost and parent/child reuse.
5. Consider batching/fusion only where Blend2D does not already optimize it and
   painter order, clipping, antialiasing and alpha remain equivalent.

Each pass needs a full-replay pixel oracle, work counters, a disable switch and
cost accounting including analysis/preparation, retained bytes and submission.
Hints identify static, conditional and dynamic work but never replace invalidation.
Cheap local passes stay synchronous; expensive optional analysis may run behind
the current correct plan using immutable metadata and a bounded result cache.

Render the current correct plan while optional analysis/preparation is pending.
Publish a replacement only if content, resources, geometry and scale generations
still match. Bound and coalesce the queue; cancel obsolete work. No idle whole-tree
scanner and no frame blocked waiting for an optimization result.

Current paint chunks contain `Rc` and mutable font caches: they are not a worker
payload. Send immutable analysis metadata first. Background rasterization needs
an explicitly frozen, thread-safe resource/text snapshot; do not bypass Rust's
ownership rules to share a live scene.

Keep cheap deterministic reductions at recording/submission time. Examples are
empty work and identical clip/translation operations. Do not merge matching fills
unless antialiasing, overlap, opacity and painter order remain equivalent. Blend2D
already bins ordered commands into bands; a second batching layer must demonstrate
an additional benefit. [Blend2D threading](https://blend2d.com/doc/multithreaded-rendering.html)

Automatic raster promotion is later than explicit cache validation. Consider
measured replay cost, blit cost, reuse count, invalidation frequency and bytes, with
hysteresis. Static/dynamic hints guide priority, never correctness. Cross-instance
reuse needs equivalent local content and compositing, not merely a matching shape.
Promotion also needs demotion: a valid parent raster can make child caches redundant,
but a child used outside that parent still needs its own policy. Publish the parent
before releasing descendants; invalidation must immediately permit direct replay.

## Validation Order

First remove measured tree/submission waste. Then prototype retained placement and
input boundaries. Next compare replay against tiles/items with simultaneous scroll
and sparse updates, insertion/reorder and rotating destination buffers. Only after
that evaluate asynchronous promotion. Each stage keeps a full-paint oracle and
reports wall time, caller/process CPU, memory and invalidation/cache counters.
