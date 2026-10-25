# Overlay Rendering

Shared rendering belongs in `retinue-ui`; game clients own layout, appearance
and interaction. The plan below extends the existing Blend2D/Wayland stack, not a
new widget framework or GPU renderer.

## Flexibility

Expose mechanisms, not mandatory widget types. A client may draw custom content
through a clipped painter, use a cached layer, or invalidate its whole view.
Custom rendering still obeys buffer ownership, resource budgets and scheduling.
Extend these internal APIs with their callers in lockstep; do not freeze an ABI
or add compatibility adapters between applications shipped together.

## Current State

- Both hosts reuse released SHM buffers, keeping at most three current-size buffers.
  PoE2 gates painted submissions on frame callbacks; Warframe uses callbacks for
  loading animation and submits static changes on demand.
- Warframe uses shared background preparation and resource accounting for scene
  images. Its static-frame cache remains game-owned; loading repaints use the
  shared buffer-repair tracker.
- PoE2 uses shared byte-budgeted RAM caches for chrome derivatives and label
  masks. Small chrome misses prepare synchronously before raster painting.
  Hover, caret and loading updates use shared partial repainting. Layout and
  command submission still traverse the view; raster work is clipped.
- Both companions use cached native label masks. Uncached fonts remain supported;
  the text editor
  retains its separate glyph and raster caches.
- Both hosts wait for events or explicit deadlines. The existing 100 ms focus
  probes remain control work; neither host uses a fixed render-loop timer.
  Partial Wayland damage alone does not eliminate layout or command submission.

These are architectural gaps, not measured claims about game frame-time impact.

## Blend2D Integration

Use Blend2D's native mechanisms before adding equivalent infrastructure. Native
builds are optimized even in development; the shared JIT pipeline cache stays
enabled. Geometry uses native primitives. Equal-size scaled blits already take
Blend2D's unscaled fast path.

`RasterImage` and label masks own immutable native images. Pixel allocations
transfer without copying and remain alive through queued rendering, independently
of cache eviction. Native wrappers are created once per prepared resource, not
per draw. Construction failures propagate; failed preparation is not cached.
Text shaping and the editor retain their existing behavior.
Keep `CachedFont`/`LabelFont` through drawing helpers; borrowing its underlying
`fontdue::Font` is appropriate for measurement, but bypasses caching when drawing.

Rendering always uses Blend2D's queue. `Painter::new` queues on the caller without
extra workers; `Painter::with_threads` accepts a nonzero total thread count.
The native bridge rejects zero rather than silently switching to immediate
drawing. Counts include the caller: two uses one additional worker.
Use the shared native pool, not isolated per-frame pools or a second application
render queue.

Clients choose a preferred budget through `render_thread_budget`, which caps it
at four and at the process's available CPU count. This runs when creating the
renderer, not on every paint. Warframe prefers two for static scene rendering;
PoE2 prefers four for its full-panel paint. Offscreen checks and Warframe's
transient surface pass use one. A complete cached Warframe frame needs no extra
context when there are no transient additions. These are measured starting
points, not CPU-model heuristics or automatic load tuning. Explicit benchmark
counts can exceed the normal budget.

`finish()` waits for native completion and checks accumulated worker errors before
releasing the target. Drop also completes outstanding work. Only submit a Wayland
buffer after successful finish. Never reuse or publish a target while native
workers may still write it.

The pinned renderer distributes ordered commands across image bands; it does not
provide general scene occlusion elimination or a retained replayable scene graph.
Our damage tracking and cross-frame resource caches address different work.
Prefer direct geometry or reusable native objects over raster snapshots unless
the actual workload benefits. Measure submission **and completion**, all-thread
CPU, memory and small dirty regions, not just full-buffer wall time.

Blend2D v0.21.2-0.21.3 miss queued mask cleanup markers. The native build compiles a
corrected copy of `rastercontext.cpp`; it does not modify the vendor checkout.
`native/blend2d-mask-retention.cmake` contains the two-line correction and rejects
a changed patch site. Review/remove it on dependency updates. Lifetime tests cover
images, masks, early drop, errors and cache eviction.

## Runtime Cache

`retinue_ui::resources::ResourceCache<K, V>` owns an LRU of immutable
`Resource<V>` handles. Keys are fixed-size identities; values report their owned
heap storage through `ResourceSize`. An oversized value is usable without
flushing the working set, but is not retained by the cache. Empty values still
have a nonzero cost. `clear()` and `retain()` release cache ownership, not external
handles.

Statistics distinguish cached bytes, live allocations and bytes retained by
callers. They include resource/key bookkeeping, not allocator overhead, font
geometry, encoded sources, SHM buffers or worker temporaries. A cache budget is
not a cap on all process memory; consumers must also bound retained handles.
Statistics are sampled explicitly, not logged per frame.
Native queues can retain pixels after the last cache handle is gone.
`painter::image_memory()` counts those allocations until native release, including
uncached sources. This overlaps cache accounting; do not add the totals together.
Native metadata, queue storage and font geometry still require process-level
measurements.

PoE2 currently assigns 8 MiB to chrome; each companion assigns 2 MiB to labels. Chrome keys use
build-time source digests, physical dimensions and a versioned transform; the
cache's value type fixes the pixel format. `CachedFont` includes the font digest,
collection/settings, glyph index and physical pixel size. It shares a
`GlyphCache` across the view's fonts without changing kerning, positioning or
tinting. Font hashing happens on construction, not during painting.

Call `prepare_header`/`prepare_controls` before drawing chrome. Draw methods only
use prepared handles. Cold label masks are still produced on demand; warm draws
reuse them. Neither cache performs disk I/O. Do not route large downloaded images
through this synchronous preparation path.

## Background Preparation

`retinue_ui::preparation::Preparation<I, O>` owns one worker, one replaceable
pending request and one result slot. New requests invalidate running work and
discard unread old results. Preparation checks a revision token between bounded
operations; obsolete results cannot publish. A result never waits for the UI to
consume a previous one. Idle workers sleep on a condition variable.
`spawn_streaming` publishes partial results through the same slot. Its consumer
supplies a bounded merge function; only an empty-to-ready transition wakes the
UI. Use it for independent images so one slow asset cannot delay ready ones.

Consumers provide the preparation closure and a short wake callback. Warframe's
callback wakes its Calloop loop, so ready assets do not wait for the next tick.
Keep preparation caches in the worker closure; hand immutable resources to the
renderer. Keep the last valid scene resources until replacements arrive.
Worker panics stop that worker and surface an error; consumers report it rather
than silently installing empty assets. Drop cancels and joins, including during
development reload. Individual decode calls are not preemptible, so their
allocation and input limits still matter.

Warframe retains only current-scene source digests, with a 64 MiB cache, a 64 MiB
decoded scene budget and a 16 MiB per-image limit. Cache keys hash daemon-provided
digests, not paths. The worker reads daemon-owned source files; it does not fetch
or write derivatives. Decode errors remain per-asset diagnostics and are retried
on later requests. Existing handles survive source replacement and eviction.
Displayed, ready and in-progress scenes can overlap in memory; these budgets are
not a whole-process cap.

## Reusable Fragments

No generic fragment renderer is implemented yet. Direct drawing helpers already
submit Blend2D operations to the current target; they do not open child contexts.
Before adding a cache, compare those operations against a prepared native image.

Blend2D has one target per context, not a public replayable command list or a
multi-target render graph. A compound cache miss can render directly into a tight
premultiplied image using Blend2D, finish it, then transfer ownership to the cache.
Later instances submit ordinary image draws to the frame's queue. The preparation
context is per miss, not per instance or frame, and need not acquire extra workers.
Do not extract fragments from an unfinished frame: that needs synchronization and
can bake unrelated background pixels into a supposedly reusable image.

PoE2 headers currently compose cached resized caps with `imageops`; glyph masks
come from Fontdue. Those are preparation operations, not alternate scene renderers.
Blend2D offers image scaling, patterns and font rendering; evaluate native
replacements with cold/warm measurements and pixel/font parity before changing
them. Moving preparation into Blend2D does not automatically put it in the frame
queue.

- Render in local coordinates into tight physical-pixel bounds, including any
  shadow or antialiasing margin. Preserve the content origin separately.
- Key appearance, source/font revisions, recipe and physical scale/size. Position
  is not identity. Reprepare for changed geometry rather than stretch cached text.
- Store native premultiplied pixels in RAM. Prepare before paint; expensive
  compositions can use the shared worker. Account for retained handles normally.
- Draw dynamic labels, counts, selection and animation separately. Cache small
  state variants only when useful. Hit regions remain client-owned.
- Preserve compositing semantics. Backdrop-dependent effects need direct drawing
  or an explicitly captured backdrop, not an independently cached fragment.
- Benchmark direct drawing against cached blits, including cold preparation and
  memory. A bitmap can be slower than a few simple primitives. Keep direct paint
  available, and test transparency, clipping and scale before adopting a fragment.

This complements partial repainting: fragments avoid repeated raster work;
damage tracking avoids touching unchanged destination pixels.

## Resource Ownership

The daemon owns encoded sources, fetching and source revisions. Clients own
renderer-native derivatives. Keep Qt's native cache separate; share identity and
policy, not pixel objects across processes.

A render-session resource service should provide:

- Immutable image and glyph handles, a byte-weighted memory budget, and deduplicated
  preparation jobs. Account separately for retained handles and work in flight:
  evicting a cache entry does not free an image still held by a scene.
- Keys containing source digest, physical target size, crop/fit, tint, pixel format
  and recipe version. Embedded sources use a build-time digest. Position is not
  part of an image key; scale changes must select the actual new pixel size.
- Runtime-only storage by default: glyph masks, control states, composed headers
  and transient frames. Do not write these on hover, caret changes or animation.
- Optional disk derivatives for costly reusable source transformations. Persist
  only when requested by policy, off-thread with atomic writes. Source replacement
  invalidates its derivatives; a late job cannot restore an obsolete revision.
- Bounded visible-first work, coalesced scale/item changes and cancellation of
  obsolete preparation. A miss keeps the current valid image or a stable
  placeholder; no decode, file read, SVG parse or high-quality resize in paint.

Do not add a second persistent source cache. Introduce derivative persistence only
with a consumer that benefits; small embedded controls need only RAM. Disk
retention follows source/recipe identity, not an arbitrary size cap.
Prune old objects only after their last valid source reference disappears; memory
eviction is not disk eviction. Recipe versions change for rendering changes, not
every application release.

Native Foundry uses the shared `thumbnails` implementation for persistent catalog
derivatives. Its disk namespace, clear-generation handling and measurements are
documented in [Native Desktop UI](native-ui.md#foundry-and-images).

## Partial Repainting

`retinue_ui::damage` implements shared invalidation and per-buffer repair.
Both surface hosts use it, including Warframe's cached-frame/loading path.

`Damage` is empty, a physical-pixel rectangle, or full invalidation. Unions use a
single bounding rectangle, keeping storage and the native clipping pass bounded.
`RepaintTracker` records both compositor damage and accumulated repair for each
buffer. New buffers start invalid. The host adds/reset entries with its SHM
buffers, preserves pending damage under backpressure, and clears it only after
successful paint and submission. Failed paints require full repair.

`Painter::clip_to_rect` intersects the native clip, including subsequent clears.
PoE2 submits the same drawing operations under that clip; no extra frame image
or CPU pixel-copy layer is needed. This first slice reduces destination/raster
work, not layout traversal or command-submission cost.

Warframe restores the repair rectangle from its cached static frame, then draws
animation and HUD content under the native clip. A full restore uses one
contiguous copy; a completed frame without transient content needs no painter.
Asset replacement and scene changes invalidate every buffer. Redraw all
intersecting transient layers, even when only the loading animation changed.
Warframe previews use this same surface composition path.

PoE2 invalidates old and new hover bounds, including ornaments and an antialiasing
margin. Caret and loading-pulse ticks invalidate their controls. Popup, move,
scroll and layout changes fall back to full repaint. Animation phase changes
only on its invalidation tick. Listing ages invalidate their rows at the next
displayed time boundary instead of changing implicitly during unrelated paints.

Rules for new consumers:

1. The view identifies invalidated regions or layers. Layout/scale changes may
   invalidate the whole view; hovering a control should not require relayout.
2. Shared invalidation records old and new bounds for movement/removal, includes
   shadows and antialiasing, converts to physical pixels with outward rounding,
   clips and coalesces rectangles. Bound region complexity; fall back to a larger
   region when necessary.
3. Each SHM buffer carries pending repair damage. A change accumulates for every
   buffer, including unavailable ones. A new/resized buffer needs a full paint.
   Never assume a released buffer contains the most recently submitted frame.
4. The painter clears the repair region and repaints every intersecting layer in
   stacking order under a clip, including unchanged layers behind transparency.
   Static layers can blit prepared rasters. Keep compositor damage distinct from
   repairs needed only to bring an older buffer current.
5. Commit only a successfully painted buffer. Keep invalidation pending if no
   buffer is available. Hide/remap, output changes and failures reset validity.

The shared API should offer region/layer invalidation, clipped painting and buffer
repair without requiring a widget hierarchy. Clients must not implement buffer-age
accounting or damage merging independently. Keep full repaint as a correctness
fallback and as the offscreen comparison oracle.

Tests rotate three buffers, hold invalidations, reset validity, remove popups and
compare hover, loading, editing and timed listing updates at fractional scales.
Native antialiasing may round clipped-edge coverage by one channel level; the
partial/full oracle permits only that difference. Pixels outside a native repair
clip must remain untouched. Full-scene before/after previews remain byte-compared.

Prefer small stable layers such as an item body, listing table and transient
popup. Do not cache a full-screen raster for every control or introduce a generic
retained UI tree. Keep input regions independent from damage regions.

## Scheduling And Surfaces

Retain the existing layer-shell role and focus behavior. Dirty regions are not
separate Wayland surfaces; creating a surface for every control adds lifecycle and
input complexity.

`schedule::Deadlines` combines monotonic control and visual deadlines. Calloop
waits for the earliest eligible deadline or an event; it sleeps indefinitely when
there are neither deadlines nor events. This is an event-loop policy, not another
render thread, task queue or custom timer runtime.

PoE2 uses Calloop's coalescing ping for worker replies, observation notifications
and local shutdown. Its X11 shortcut connection also wakes the loop on readability.
Warframe's bounded presentation inbox wakes its existing loop signal, including
messages queued before registration and work left after a bounded drain. Asset
preparation already uses the same signal. Keep publication before notification.

Visual deadlines cover loading, caret blink, tooltip appearance, cooldown labels
and listing ages. Frame callbacks pace drawing; buffer release grants write
access. Neither substitutes for the other. Suppress visual deadlines while hidden,
awaiting configuration/frame permission or lacking a writable buffer. Keep dirty
regions and sample the current phase when drawing resumes; never replay missed
animation frames. Refresh time-based labels for the first remapped frame.

Control work continues without compositor callbacks. Scene expiry and clipboard
timeouts have explicit deadlines. Existing 100 ms XWayland focus probes also bound
atomic shutdown checks; these have not been converted to property notifications.
PoE2 temporarily checks focus return every 16 ms only while an item-copy request
is pending, capped at two seconds, preserving input handoff latency. These probes
do not request paints. Native game observation remains at its own cadence.

New timed behavior must provide its next meaningful deadline, not add a permanent
tick. Work from another thread must wake the host after publishing. A due visual
deadline must advance when consumed, or be withheld while rendering is blocked.
Never use a zero-timeout retry for buffer backpressure, a hidden surface, or an
unconsumed animation frame. Clipboard/focus/stop work must not depend on a frame
callback arriving. Keep tests for wake coalescing, late registration, partial
drains, hidden/backpressured deadlines and delayed animation phases.

Start with coarse panel damage, then finer regions where useful. Consider smaller
surface backing bounds only after move/resize, popup and focus tests; full-output
buffers are simple but cost about 14 MiB each at 2560x1440. Do not add another
full-output retained canvas without accounting for that memory.

### KWin Surface Costs

Source audit against KWin 6.7.5:

- Its ordinary SHM texture path uploads the supplied damage. Full-buffer damage
  defeats smaller application paints. Three or more rectangles currently collapse
  to their bounding box; do not build client policy around that implementation
  detail or assume many tiny damage rectangles guarantee savings.
- Some systems import SHM through u-dmabuf instead. Measure the active backend;
  do not equate SHM with a guaranteed full CPU-to-GPU upload.
- A size change recreates the texture. Keep surface geometry stable during normal
  interaction; reducing backing size needs an explicit resize/remap design.
- Buffer release grants write access; a frame callback only paces work.
  Presentation timing is a separate signal. Never treat these as interchangeable.
- Input and opaque regions use surface coordinates; damage uses buffer pixels.
  Transparent overlays must not declare their full output opaque. Any opaque
  subregion must be proved by the actual painted pixels.

The remaining gaps include full-view command submission and periodic focus probes.
Keep released-buffer reuse, three-buffer backpressure and validated focus behavior.
PoE2 discards its SHM pool on hide; Warframe retains its pool's high-water size.
Measure hide/show churn and retained memory before unifying that policy: SHM pools
grow but cannot shrink in place.

## Implementation Order

1. Shared runtime resource cache and preparation interface; move PoE2 chrome
   derivatives and label masks behind it, then reuse it for Warframe images.
   Native image ownership and queued rendering now support safe batching.
2. Compare queued concurrency and retained resources on actual views. Preserve
   saved pixel/measurement baselines; do not reintroduce a runtime backend switch.
   Decide whether opt-in composite fragments warrant another helper.
3. Shared invalidation, clipping and per-buffer repair; integrate PoE2 first and
   move Warframe's existing static/animation path onto the same primitives.
4. Explicit wake/deadline scheduling; then implement the remaining source-matched
   animations. Adaptive quality and GPU rendering need measurements first.
5. Opt-in derivative persistence when a real expensive asset workload warrants it.

Test full-versus-partial pixel equivalence at multiple scales, translucent overlap,
old-position cleanup, three-buffer rotation, delayed releases, resize and remap.
Test cache byte accounting, source replacement, stale worker completion, queue
bounds and eviction with retained handles. Measure warm paint time, dirty area,
allocations, bytes retained and idle wakeups before choosing tuning thresholds.

Separate deterministic offscreen benchmarks from compositor/game measurements.
Preserve identifiable before/after binaries and run both execution orders under
comparable load. A successful benchmark run alone is not a regression verdict.
Background preparation needs separate cold/warm decode, request-to-ready latency,
worker CPU and retained-memory cases; synchronous paint timings do not cover it.
Use completed-render wall-time distributions together with CPU utilization at a
stated workload and repaint cadence. Process CPU time sums all threads; divided
by elapsed wall time, it gives average logical CPUs occupied. Report both the
one-logical-CPU percentage and its normalization by available logical CPUs.
Do not reject threading merely because CPU time doubled: the absolute utilization
and latency benefit matter. Conversely, a low whole-machine average does not
prove there is no contention with a game's critical thread or shared caches.
Continuous-throughput measurements are not normal companion utilization.
The benchmark supports capped cadence, with sleep excluded from render latency
but included in the utilization interval. Check achieved cadence and budget
misses. Live validation must still cover idle wakeups, interaction and game frame
times; the headless harness does not simulate the compositor or a loaded game.
Record cold preparation and first paint separately from warm redraws. Include
unchanged scenes, hover/caret updates, scrolling, scale changes and asynchronous
content replacement for both games. Report distributions rather than only mean
time, with resolution, UI scale, build/toolchain identity and memory context.
Keep benchmarks opt-in, outside normal runtime, with no timing-based CI failures
until repeatability is established. Headless paint timing does not measure KWin
upload cost, input latency or game frame pacing.
The [benchmark harness](../../tools/render-bench/README.md) provides offscreen
cases for both companions and a mapped Evaluate scheduling comparison. The latter
restores the previous polling policy alongside current event-driven scheduling
with identical rendering code; it is not a complete historical application build.
Presentation feedback uses the compositor's advertised clock. Keep scripted
worker-ready latency distinct from physical input latency and game frame times.
Both harnesses exclude recording code from normal companion builds.

## References

- [Generate version-matched local API docs](../../tools/rendering-docs/README.md)
- [Blend2D batching, resource lifetime and worker guidance](https://blend2d.com/doc/multithreaded-rendering.html)
- [Blend2D context modes](https://blend2d.com/doc/structBLContextCreateInfo.html)
- [Blend2D target ownership and context operations](https://blend2d.com/doc/classBLContext.html)
- [Blend2D image preparation](https://blend2d.com/doc/classBLImage.html)
- [Linux process CPU clocks](https://man7.org/linux/man-pages/man2/clock_gettime.2.html)
- [Linux process CPU utilization and normalization](https://man7.org/linux/man-pages/man1/pidstat.1.html)
- [Pinned band/command execution](https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/workerproc.cpp)
- [Wayland surface damage, frame callbacks and buffer lifetime](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_surface)
- [Calloop event waits and deadline timeouts](https://docs.rs/calloop/0.14.4/calloop/struct.EventLoop.html)
- [Calloop coalescing cross-thread wakeups](https://docs.rs/calloop/0.14.4/calloop/ping/index.html)
- [KWin 6.7.5 SHM texture updates](https://invent.kde.org/plasma/kwin/-/blob/v6.7.5/src/scene/opengl/texture.cpp)
- [KWin 6.7.5 surface damage and texture lifetime](https://invent.kde.org/plasma/kwin/-/blob/v6.7.5/src/scene/surfaceitem.cpp)
- [SCTK SlotPool buffer access](https://docs.rs/smithay-client-toolkit/0.20.0/smithay_client_toolkit/shm/slot/struct.SlotPool.html)
- [LRU eviction primitive](https://docs.rs/lru/0.19.0/lru/struct.LruCache.html)
- [Rust condition-variable wait semantics](https://doc.rust-lang.org/std/sync/struct.Condvar.html)
- [Qt's offscreen layer model and caching tradeoffs](https://doc.qt.io/qt-6/qml-qtquick-item.html#item-layers)
- [Asset ownership and existing Qt cache](assets.md)
