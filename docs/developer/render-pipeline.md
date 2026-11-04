# Render Pipeline Design

Status: redesign approved; foundation implemented 2026-10-28. The full compiler
and materialization planner below remain in progress.
[Composition](ui-composition.md) and [invalidation](ui-invalidation.md) describe
the running implementation. This design applies to wfgui and both companions.

## Implementation Status

The renderer now consumes `PaintFrame`, not the layout `Scene`. Immutable shared
`PaintProgram`s hold source and compiled commands; `PaintInstance`s supply resolved
placement or bindings to shared properties, plus cache preference. `PaintGroup`s
name ordered instance ranges.
`Scene::frame()` is the existing layout adapter. A different layout API can produce
frames directly through `PaintFrame::replace`, without Taffy nodes or algorithms.
`replace_with_properties` installs translation/clip forests; `patch_properties`
validates a batch atomically and updates dependent properties and paint consumers.
Logical translations compose before pixel snapping. Property lifetime identities
and versions distinguish current values from stale snapshots or reused keys.
The current Scene adapter supplies absolute logical origins to preserve its
existing summation/snapping; the renderer also accepts hierarchical translations.
Layout still owns measurement, hit geometry and the current snapping rules.
The planned [declarative frontend](declarative-layout.md) adds inspectable
alternatives, collection templates, anchors and motion above this boundary. Keep
those declarations separate from realized paint and pass scoped structural hints
without coupling the renderer to layout algorithms.

`FrameSnapshot` retains programs/resources for deterministic in-process replay.
It does not freeze live callback state; `has_external_paint` makes that dependency
explicit. The GUI replay benchmark captures the Relic viewport and refuses live
paint in its captured region. Capturing is opt-in, not a per-frame production copy.
The current fixture exercises text, shapes and embedded icons but has no remote
item artwork; expand trace coverage before choosing a cache granularity.

`retinue-render` owns weak program interning, immutable resource identities,
effect/resource analysis, the sparse consumer index, materialization recipes and
ordered preparation tasks. `retinue-ui::scene` is the layout/recording adapter, not a
renderer namespace. Preparation tasks drive actual native work; their completed
outputs feed a retained replay/item/layer draw plan. Failed optional producers
fall back to source commands. Item and parent rasters share physical-effect-based
repair; mature parent caches can repair small changes while moving, without
recreating their former child rasters. Parent preparation can consume child images
before redundant child ownership is released.

Items and layers now share one residency store. Selected plan consumers are
independent of the warm-retention ceiling; pending outputs reserve peak pixels,
and retired allocations remain accounted through native completion. Warm entries
drop source recipes. Changed instance plans and ancestor recipes update from the
frame journal; structural changes rebuild group indexing.

`CollectionContext` adds advisory content origins, stable row partitions and
motion state independently of layout. Row candidates use the same recipes and
residency as items and subtrees. Their full paint bounds remain stable as the
viewport clips them; edge changes and content mutations repair local regions.
Reordering suppresses aggregate candidates, including containing ancestors, while
preserving independent item reuse. Cost, effect and allocation checks can decline
any candidate. The current layout frontend does not yet emit these hints.

`OptimizationContext` carries extensible typed annotations on groups and named
source fragments. `Command::Fragment` retains reusable source below layout/item
boundaries; its optional raster shares the same preparation graph and residency
store as item/row/parent outputs. Foundry and other component indicators now emit
green glow source commands, not widget-owned raster caches.

`TileHint` proposes bounded, content-aligned spatial coverage through that same
context. Selected tiles and source/item replay can cover disjoint parts of one
ordered range. Parent preparation consumes available child regions and replays
gaps, including after a failed producer. No separate tile cache is involved.

`RenderHost` coordinates sibling stores' pending/live pixels through native
completion and bounds optional preparation per pass. Work that exceeds the pass
budget replays current source and is reconsidered from the newest frame; no old
source graph is queued for later publication. GUI and companion hosts use separate
policies. See [scheduling](#execution-and-scheduling) for the precise limits.
Inactive pixels expire through the host's next maintenance deadline, even when
their scene no longer renders. Maintenance does not request a paint or poll.

Large compiled programs can receive a background-built ordered bounds index.
It skips whole ranges during partial replay without changing source commands or
painter order. Snapshot/result ownership and host scheduling are described below.
Exact native scratch accounting and calibrated pressure demotion remain open.
Group readiness/publication still scan metadata. See the exact implementation
limits in [render residency](render-residency.md). Existing reuse thresholds are
the baseline; no claim that cells have beaten rows or tiles. Timing tuning follows
the layout frontend migration.

## Decision

Completed presentation-buffer pixels are another possible execution input, not a
new raster cache. The [relocate design](render-relocate.md) places move/repair
selection before cache preparation and defines surface-wide validity and host
ownership. Its prototype is measured but not enabled in production.
`Renderer::prepare_for` now separates current repair demand from retained plan
consumers. Shared fragments survive a frame that does not sample them; unrequested
optional preparations do not start work or schedule a repaint.

Separate the persistent description of **what must be drawn** from the derived
plan for **how to draw it**. Retain the existing composition/layout model,
resource preparation, hosts and Blend2D backend. Replace the renderer's coupled
cache-selection/preparation logic with an incremental compiler and resource-aware
execution planner. Raster caching becomes one optimization, not the organizing
principle of the drawing API.

```text
UI state + layout + sampled animation
                 |
       ordered paint instances ------ property state
                 |                       |
       immutable paint programs + versioned resources
                 |
       cached analyses / safe rewrites
                 |
       execution plan + materialization choices
                 |
       prepare / repair / replay / composite tasks
                 |
       existing queued Blend2D backend
                 |
       host damage repair and presentation
```

The plan is disposable in the sense that it is derivable, not rebuilt wholesale
every frame. Unchanged program analyses and plan sections survive. Keep a direct
lowering path for correctness tests and when optional work is unavailable.

This establishes the ownership and correctness boundaries now. Individual
optimization policies still need measurements; designing the boundary does not
require enabling every possible optimization.

## What Exists

- `render/program.rs` retains original `Command` arrays separately
  from locally optimized arrays and raster images. A cache does not erase source.
- Foundry caches individual cards. Other collection pages cache named item scopes.
  These are mostly independent item rasters, not rows or shifted viewport images.
- Changed items have local dirty-region repair; scrolling/reordering reuses their
  content identity. Optional ancestor layers can already use child images and
  release redundant child caches after publication.
- `optimize.rs` has conservative local reductions. `layers.rs` admits hinted
  subtrees using reuse counts, work estimates and byte limits.
- Collection metadata can propose content-space rows without changing source
  paint. Tests compare row repair and cell reuse with direct replay at multiple
  scales; this is not a measured choice of production grid policy.
- Optional tile hints propose spatial partitions instead of ordered row partitions.
  The planner can select only some tiles, preserve nested child coverage outside
  parent tiles, and keep replay for all unselected regions. Production frontends
  do not yet emit these hints; no default tile size has been selected.

The remaining policy work is calibrated materialization choice and pressure
response. Content-space coverage and bounded deferred analysis now have execution
paths. Dependency and
effect contracts already drive local repair and invalidation. A new drawing API
from scratch is unnecessary.

### Evidence Limits

The previous benchmarks justify keeping item reuse as a baseline. They do **not**
establish that cells beat rows or tiles: several changes were measured together.

| Historical Workload | Observed Result | Limitation |
| --- | --- | --- |
| Foundry, 1080p/100%, update + paint | p50 1.231 -> 0.691 ms | Retained projection, invalidation and card reuse changed together |
| Relic drag, 1440p/125% | p50 30.172 -> 2.845 ms | Also removed repeated input/layout work |
| Large transparent parent layers over cached cells | About 80 -> 98 ms per 100 frames at 100%; extra 6-7 MiB | Rejects this composition strategy, not tiled scroll reuse generally |

These are historical runs, not new measurements or current performance promises.
The prior results are retained locally under `research/retinue/ui-incremental/`,
`relic-scroll/` and `render-stage3/`. Independent item updates and reordering make
cells useful; neither is proof that they are the best granularity everywhere.

## Reference Architecture

The recommendation is a Retinue-specific synthesis, not an existing framework
we can enable with a flag.

| Reference | Adopt | Do Not Import |
| --- | --- | --- |
| [Chromium RenderingNG](https://developer.chrome.com/docs/chromium/renderingng-data-structures) | Paint data separate from transform, clip, effect and scroll properties | Browser process model |
| [Chromium compositor](https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md) | Distinct content invalidation, raster invalidation and display damage; visible-work priorities | GPU tile sizes or assumptions about cheap texture composition |
| [Firefox rendering](https://firefox-source-docs.mozilla.org/gfx/RenderingOverview.html) | Persistent scene, per-frame visibility, shared primitives and separately planned picture tasks; locally repairable tiles | A second renderer or mandatory texture for each element |
| [Unreal Slate invalidation](https://dev.epicgames.com/documentation/en-us/unreal-engine/invalidation-in-slate-and-umg-for-unreal-engine) | Geometry/paint retention distinct from retained raster surfaces | Treating every subtree as a texture |
| [Unreal RDG](https://dev.epicgames.com/documentation/en-us/unreal-engine/render-dependency-graph-in-unreal-engine) | Setup, compilation and execution separated; declared resource uses and lifetime validation | GPU barriers or a dependency edge for every pair of primitives |
| [MLIR passes](https://mlir.llvm.org/docs/PassManagement/) | Cached analyses, explicit preservation/invalidation, pass diagnostics | An LLVM dependency or another pixel JIT |

Blend2D remains responsible for rasterization and pixel-pipeline JIT compilation.
Its asynchronous contexts queue ordered work; the pinned implementation processes
that work by bands. Retinue optimizes above this boundary. Use public APIs and
include native completion in timings. Do not capture or modify Blend2D's private
command queue. [Blend2D threading](https://blend2d.com/doc/multithreaded-rendering.html)

## Persistent Representation

Use a small typed representation, not serialized JSON, a generic object graph or
one heap allocation per primitive. Existing `Command` variants are the starting
point. Immutable operation blocks reference reusable fragments, text/path data and
resources. Sharing an operation block does not remove any of its draw instances.

| Record | Identity / Contents | Lifetime |
| --- | --- | --- |
| Paint program | Stable fragment identity, revision, ordered operations, referenced programs/resources | Until no scene, analysis or task needs it |
| Paint instance | Namespaced element/occurrence identity, program, order, property references | Retained scene instance |
| Property state | Transform, clip, isolation/effect, scroll offset; independent revisions | Shared by dependent instances |
| Resource | Typed generational handle, version, immutable image/font/path data | Resource store plus active task/frame leases |
| Compiled program | Source identity + dependencies + backend semantics; optimized operations and analyses | Reusable until those inputs change |
| Materialization | Recipe, input versions, valid content coverage, surface generation, consumers | Pending/ready/retired cache result |
| Execution plan | Tasks, ordered draw ranges, resource uses, output damage | Reusable topology with frame-specific inputs |

UI identity is not paint identity; placement is not content; a resource handle is
not a raw pointer. Allocator address reuse must not produce a cache hit. Keep
native resources alive through execution leases rather than embedding mutable
font/image caches in every operation.

Prefer shared typed arrays and generational tables. Do not require an ID for every
primitive: stable keyed fragments are the diff boundary, with conservative local
diff inside small changed fragments. Frontends should not manage optimizer IDs.
Hash/intern newly recorded content once, verify equality on hash matches, and
reuse identity/revision thereafter. No per-frame recursive hashing of the scene.

Repeated chrome can share one program with multiple instances. Dynamic text or
badges can be separate ordered fragments. Do not rediscover intentional sharing
by scanning arbitrary command subsequences every frame. Automatic pattern finding
may be a later analysis over changed/hot programs, not a foundation requirement.

### Optimization Context

Separate semantic facts from advisory context. Effects, resources, clipping,
ordering and native-paint barriers come from the source commands and analysis.
Hints can influence which valid plan is selected, never make an invalid plan safe.
This distinction follows the intent of [MLIR attributes](https://mlir.llvm.org/docs/LangRef/#attributes)
and [LLVM transformation metadata](https://llvm.org/docs/TransformMetadata.html),
without importing a compiler framework.

`OptimizationContext` is an immutable, copy-on-write set of typed annotations.
Adding a new annotation requires its Rust type and a consuming pass, not changes
to a registry, all callers or an exhaustive metadata enum. Values implement
`Debug + Eq + Send + Sync`; empty contexts allocate nothing. Passes use `get::<T>()`
and ignore unknown types. For example:

```rust
let context = OptimizationContext::default()
    .with(CacheHint::Stable)
    .with(collection);
frame.annotate_group(group_key, context)?;
```

Annotations are scope-local. There is no implicit inheritance or generic merge:
each pass defines how its own hints compose across nested scopes. Metadata edits
invalidate affected planning decisions; they do not themselves change paint.
`CacheHint`, `CollectionContext` and `TileHint` are existing consumers, not a closed list of
permitted annotations. Keep new hints small and immutable, not mutable app state
or closures. Context is in-process metadata, not a serialized protocol or plugin ABI.

### Named Source Fragments

`Command::Fragment` calls a `PaintProgram` with its own advisory context. The
program store interns nested source through the same equality-checked store used
for top-level programs. Placement, clips and each occurrence remain distinct.
Changing source or physical scale changes the program identity; replacing only
hints can reuse unchanged pixels through normal recipe repair.

The planner gathers visible uses, checks effects/cost/allocation limits, and
prepares eligible fragments before their consumers. Parent preparation can sample
these outputs. After publication, independent uses retain the child; redundant
children are released, with bounded warm retention only when reuse justifies it.
Failed or declined preparation replays source commands, including inside another
preparation task. No effect-specific cache or hidden Blend2D command queue is used.

The initial policy uses observed repetition and an optional stability hint. It is
not automatic discovery of arbitrary repeated command subsequences, nor a
calibrated performance policy. Source-equivalence and ownership tests precede
timing tuning after the layout migration.

### Geometry And Effects

Keep paint programs in their local coordinate space; preserve the current
level-by-level device-pixel snapping policy. Layout, paint extent, hit bounds,
clip and proved opaque coverage are different facts. Transform/clip properties
can be updated without copying command payloads. Baking a property into a raster
must register it as an input; an outer viewport clip need not become content.

Derive normal operation metadata in the engine:

- Conservative pixel write extent, including stroke, shadow and antialiasing.
- Destination/backdrop read extent and source-resource reads.
- Known opaque coverage, composition mode and isolation requirements.
- Changing inputs and whether execution has externally observable effects.

Custom drawing that records ordinary operations gets these contracts automatically.
An opaque callback remains an ordered barrier, with conservative damage and no
optional caching/background execution unless it gains a complete contract.
Do not require app authors to manually certify ordinary fills or image blits.

Source-over draws generally read existing destination pixels. Non-overlapping
write rectangles alone are insufficient for arbitrary effects. Backdrop effects,
destination-reading operations and callbacks can create wider dependencies.
Unknown bounds or behavior must reduce optimization, not rendering correctness.

## Incremental Compilation

Keep analyses local to programs or affected instance ranges. Each analysis lists
its inputs and each transformation declares which analyses remain valid. Store
reverse dependencies from resources/properties/programs to their consumers.
The immediate path has bounded work; a clean frame requires no optimization scan.

| Change | Recompute | Usually Reuse |
| --- | --- | --- |
| Scroll / integer translation | Visibility, placement, required coverage, screen damage | Programs, compilation, valid content-space pixels |
| Timer/color/text in fixed bounds | Changed fragment and affected raster coverage | Siblings, layout, unrelated tiles |
| Image arrives, same dimensions | Resource-dependent paint and coverage | Geometry and unrelated programs |
| Item insert/reorder | Instance order/placement, overlap and changed contributor ranges | Item programs and independent rasters |
| Viewport clip changes | Visibility and missing coverage | Pixels already valid in the same content space |
| Font/intrinsic size/column count changes | Dependent layout, geometry and affected programs | Unaffected resources; item content only where geometry still matches |
| Scale/sampling phase changes | Physical derivatives and incompatible raster variants | Logical source descriptions |

Do not invalidate every pending result because a global scene generation changed.
Validate the result's actual dependency versions and coverage. A completed tile
can remain useful after the user scrolls, while a changed font invalidates it.

Useful compiler passes extend beyond pixels:

1. Share unchanged programs, measurements, prepared glyph runs and resources.
2. Remove empty work; simplify state/clips/transforms where rounding is preserved.
3. Cull invisible work and proved opaque overdraw without losing hidden source.
4. Reuse repeated fragments and prepared paths/masks; retain instance multiplicity.
5. Select materializations, propagate local repair and eliminate redundant copies.
6. Cull unused preparation tasks and shorten scratch-resource lifetimes.
7. Lower compatible operations with less backend state churn where measured useful.

Passes carry source provenance for inspection and raw replay. Do not claim that
two fills can always merge: overlap, antialiasing and alpha can change the result.
Avoid maintaining a separate general-purpose algebra/compiler framework.

## Raster Planning

Cache granularity is an execution choice, not the frontend's permanent data model.
Evaluate alternatives for meaningful scopes, not every possible command subset.

Include damage-induced replay cost, not only an element's own changes. A moving
overlay can repeatedly expose expensive, otherwise static content behind it;
both the overlay and those repaired contributors can justify a temporary raster.
Count actual intersecting paint work rather than requiring whole-element repaints
before admitting that reuse. Motion hints may predict a bounded reuse window, but
still compare preparation, copy/blit, repair, memory and expected remaining frames.
Reassess after motion stops; do not pin animation-only surfaces indefinitely.

Distinguish translation from changing geometry. An expanding mod card can reuse
artwork or frame fragments while its clip, description layout or other content
changes; caching the entire card on every expansion frame can cost more. Custom
Canvas-like drawing should emit the normal inspectable commands and stable named
fragments, not become a native callback barrier. Profile the observed mod-card
overlap/expansion choppiness after frontend migration; no cause established yet.

| Candidate | Potential Benefit | Cost / Failure Mode |
| --- | --- | --- |
| Direct replay | No cache preparation or extra surface traffic | Repeated expensive paint |
| Item/fragment | Independent repair/reorder; little empty area | More images, blits and preparation contexts |
| Row/strip | Fewer blits; good vertical locality | One changing item damages a larger unit; reflow changes membership |
| Content-space tiles | Local repairs independent of item/viewport boundaries | Metadata, seams/gutters and contributor tracking |
| Rolling viewport window | Reuse interior, fill exposed strips | Copy/overlap handling, jumps and fractional phase |
| Stable subtree | Cheap composition while moving | Large sparse surfaces or frequently changing children can lose |

Start comparisons with cells, rows and fixed tiles plus direct replay. Keep a
rolling-window strategy expressible; do not immediately build an adaptive quadtree
and several competing allocators. Choose defaults from matched traces, not from
the existence of a browser/GPU precedent.

A scroll container supplies content-space coordinates, viewport, visible range,
optional item/row boundaries and motion hints. It does not mandate a bitmap.
Cache coverage belongs to that content space, with versions for contributing
fragments. Scrolling requests new coverage; a timer invalidates a local portion.
The final screen still needs its changed viewport pixels composed and presented.
Pixel reuse reduces raster work, not that unavoidable output traffic.

The lower-level `PaintFrame::annotate_collection` contract is advisory: unique row
keys and ordered ranges must partition the collection, but crossing other scopes
can make those rows unsuitable for reuse. Empty row hints allow a whole-content
candidate. Resolved instance geometry and clips remain authoritative; annotations
never change what gets painted. `Reordering` discourages aggregate reuse, not
independent child rasters. Treat cost/motion as hints, as in Flutter's
[isComplex](https://api.flutter.dev/flutter/widgets/CustomPaint/isComplex.html) and
[willChange](https://api.flutter.dev/flutter/widgets/CustomPaint/willChange.html),
without borrowing its cost thresholds.

Rows are ordered source partitions; tiles are disjoint spatial regions over an
ordered range. `TileHint { size: (width, height) }` uses physical pixels and requires
a collection content origin. Zero sizes, overflowing coordinates or more than 256
candidate tiles fall back to ordinary candidates. Barrier, reuse, cost and peak
allocation checks still apply. Tile hints propose a representation; they do not
force allocation or establish its profitability over rows/items.

The selected plan indexes coverage by source interval. Replay skips only pixels
actually supplied by a selected raster. Each layer is composited at the end of its
source range; nested layers contribute only outside selected ancestor coverage.
Ready and failed child regions can coexist within one parent preparation task.
No cached or replayed contribution is drawn twice merely because ranges overlap.

Current clip repair clears leaving strips and paints entering strips; this is not
yet a validity atlas retaining all offscreen coverage. Tile recipes retain the
ordered source range and cull by effects during repair, not a fully sparse
contributor index. Existing operations render from their own geometry/resources,
including the analytic inset-shadow bands. A future filter sampling neighbouring
framebuffer pixels needs explicit dependency/support handling before tiled reuse;
tile boundaries must not truncate those inputs.

Repeated paint below instance boundaries uses named source
fragments. The component glow is the first production caller; its source remains
inspectable when a parent item or row is also materialized. Pixel/ownership tests
cover this path; timing comparisons remain pending.

Repair must replay **all** contributors to damaged pixels, not only the changed
operation. Tiles include conservative effect/filter support at their edges and
composite non-overlapping output interiors; otherwise shadows or alpha seams
change. A rectangle-set can coarsen to full repair when tracking it costs more.

Insertion can move intact cached items/rows and repair exposed content. Reorder
changes contributor order, not just positions. Reusing part of a flattened surface
requires proof that its backdrop, clipping and overlapping contributors match.
Never cut arbitrary cells out of a presentation buffer or assume a parent raster
can reconstruct its isolated children.

For example, scrolling a Relic grid while one price changes should:

1. Patch the scroll property and update demanded content coverage.
2. Revise the price fragment and mark its old/new footprint dirty in dependent
   item/row/tile recipes, without invalidating their unaffected coverage.
3. Reuse valid pixels; repair the dirty region with every intersecting contributor;
   render newly exposed content not already present in a cache.
4. Compose those results in order beneath the viewport clip, then draw independent
   chrome/popups. A pending optional repair uses current direct replay for its
   invalid region, excluding that region from the old raster's composition.

This combines movement and dirty regions without assuming a particular cache
granularity. It does not imply copying the entire old viewport each frame.

### Parent And Child Reuse

A parent's recipe can mix valid child rasters, compiled programs and direct
fragments in original painter order. The same child may serve several parents
and a visible instance. Track consumers rather than treating parent promotion as
unconditional child eviction. Drop redundant children only after the parent is
ready and existing consumers have finished; keep source programs for demotion.

A stable background and live label may be separated when their order/composition
allows it. A changing element between two overlapping static ranges cannot simply
be moved above their combined cache. Use multiple ordered slices or repair the
affected region instead.

### Policy And Cost

Keep optional hints separate from correctness:

- Cache preference: avoid, automatic, preferred. A benchmark override can force
  an eligible candidate for experiments, not bypass safety checks.
- Expected behavior: stable, conditional or frequently changing; translation
  animation, interaction and scroll locality. These guide estimates, not validity.
- Required intermediate/exported surface: a semantic output, not a cache hint.
  Its allocation failure cannot fall back to incorrectly blended direct drawing.

Estimate benefit over a reuse horizon: avoided replay minus compilation/preparation,
repair, copies, extra composition and retention cost. Use marginal cost after
existing child caches, not their original draw complexity. Record actual repair
frequency, surface density, reuse, bytes moved and native completion cost. Avoid
flush-per-primitive profiling; use sampled task timings and calibrated trace runs.

Promotion/demotion use hysteresis. Parent promotion evaluates shared-child uses
and temporary overlap in memory. Prefer cheap repair over oscillating between
representations. Stop speculative preparation during rapid invalidation or when
its expected lifetime cannot repay it.

Visible source assets and submitted surfaces retain active leases regardless of
idle-cache limits. Selected UI rasters belong to the execution plan, not the asset
LRU. Admission/demotion follows marginal rendering benefit, validity and explicit
consumer lifetimes; bytes constrain peak allocation and pressure response. Only
inactive rasters with likely near-term reuse enter bounded warm storage. See the
[residency design](render-residency.md) for ownership, states and replacement of
the current split item/layer policies. Do not thrash the active working set.
Account once for shared allocations and separately for live, retained, pending
and scratch bytes. No disk cache for composed UI; existing daemon-identified image
derivatives keep their separate disk policy.

## Execution And Scheduling

The execution graph describes tasks and resources, not a quadratic graph of every
primitive. Example tasks: prepare an image/glyph run, rasterize/repair a surface,
copy a proved-valid region, replay an ordered range, composite, publish.
Each declares versioned inputs, output regions and completion dependencies.

Preserve same-target painter order by default. Independent targets may prepare
concurrently; disjoint draws are only reorderable after read/write footprints and
effects prove independence. A spatial index and interval summaries can establish
local dependencies without comparing every pair. No sorting by color or texture
across overlapping translucent operations.

Published images are immutable while readers exist. Repair uses new tiles, copy
on write, or demonstrably exclusive storage after previous reads complete.
Surface pools may recycle storage only after all queued native work releases it.
The source-to-destination copy is an explicit task, including alias/overlap rules.
One native target has one writer at a time; independent-looking regions do not
authorize concurrent mutation through separate Blend2D contexts.

Submission is not completion. Group work into useful contexts, finish producer
targets before dependent consumers sample them, and avoid a context/wait per tiny
element. Pool compatible scratch buffers with a bounded idle lifetime. Measure
packing tiny cache preparations together before introducing atlas complexity.

Use one aggregate work/thread budget per host, including preparation and Blend2D
workers. The existing per-context thread count is not such a budget. Desktop and
overlay policies differ; do not give every concurrent cache task the host's full
thread allowance. Native tasks are not arbitrarily preemptible, so bound admitted
work rather than promising interruption halfway through a raster call.

The current implementation uses an explicit `RenderHost` shared by sibling
`Renderer`s. Native roots call `begin_frame(preferred_threads)` before creating the
Painter, use its returned thread allowance and retain the guard through native
completion, including on error. Nested `begin_pass` calls share that reservation
instead of counting the same foreground context twice. An already-running analysis
job reserves one worker slot; new analysis waits until all frame/pass guards end.
Native preparation completes serially on the owner thread and receives the remaining
allowance, with the caller shared by both native contexts. Unmanaged paint callers
own their scheduling and do not enqueue analysis. This is not
process-wide control over unrelated image workers or Blend2D scratch pools.

Pass policies bound native task count, repaired/copied pixels and accumulated
native time. They allow one eligible oversized task to avoid starvation, then
yield; the time limit is checked between indivisible tasks, not a hard frame-time
guarantee. Owner-thread planning itself is not time-sliced. The initial desktop and
overlay values are guardrails, not benchmark-selected optima. Standalone renderers
keep immediate preparation; production hosts explicitly opt into shared policies.

Deferred tasks are distinct from failed tasks: no failure cooldown and no stale
raster publication. The next pass rebuilds demand from current source, placement
and clips. Dirty old pixels remain repair donors but never draw as valid output.
The GUI schedules paced follow-up passes; PoE uses its existing animation cadence.
Warframe's one-shot cached frame can keep correct direct replay without waking
merely to optimize an image that is not being redrawn. Memory-pressure rejection
does not request an idle retry loop; reclaim or a scene change permits retry.

### Background Analysis

The first worker pass builds an ordered bounds hierarchy over the top-level
commands of large compiled programs. Leaves retain command ranges, not copies; partial replay
can skip entire disjoint ranges before lowering to Blend2D. Bounds use the existing
per-command culling contract. Unknown operations stay unskippable, clips retain
their existing semantics, and neither translucency nor callback order changes.
Full replay and the raw oracle remain available. This does not change the chosen
raster representation or move native preparation to a worker.

Each host lazily starts one worker. `AnalysisPolicy` limits command counts,
queued/running/completed jobs and reserved snapshot/result bytes before capture.
Snapshots contain only geometry, immutable `ProgramId` and a weak thread-safe
lifetime token. No image, font, `Rc` program, layout object or callback crosses
threads. Small programs allocate no lifetime token. Dead sources cancel work;
host teardown cancels and joins the worker, including for companion reload.

Owner-thread collection validates the original live program identity before
attaching an immutable index. Reusing a UI key cannot apply an old result to new
content. Collection occurs during existing preparation/maintenance, never via an
idle poll or redraw just to install an optimization. Missing, failed or unfinished
analysis uses ordinary replay. Queue pressure retries current programs on a later
render pass; it does not queue an unbounded backlog. Installed indices follow the
program lifetime, separate from image retention. The memory report exposes job
counts and pending metadata reservations, not total analysis heap size.

The current index addresses linear command-bound checks during damage replay.
Submission visits frame instance programs, not every nested fragment. Fragment
replay can use an index prepared for that program by another consumer.
The pass does not perform arbitrary subsequence discovery, global scene optimization,
opaque occlusion across elements or command reordering. Thresholds are guardrails;
profiling and policy calibration follow the declarative-layout migration.

### Native Scratch Accounting

The host ceiling accounts for optional raster outputs, including pending and
retired pixels, not all memory used to produce them. Blend2D's public runtime
statistics expose global JIT and zero-buffer pools. They omit command/job arenas,
worker edge storage, temporary paths and glyph buffers. Report these known pools
separately; neither their sum nor process RSS is a per-task scratch measurement.

The pinned Blend2D implementation does enforce `command_queue_limit`: a full batch
is flushed before growing further. Its header's "no effect" note is outdated.
The limit counts commands, not bytes, and cannot bound a single complex path's
memory. Keep upstream's default until profiling justifies changing it; do not add
per-primitive flushes or guessed scratch-byte reservations. Exact per-context
accounting would need allocator instrumentation or an upstream API, not reads of
private worker structures while they execute. The GUI memory report separates
host raster residency, native image pixels, public native pools and process memory.

### External GPU Surfaces

Future GPU content such as a 3D viewer is a separately produced surface, not a
`Command::External` CPU callback or an image that requires per-frame readback.
Layout supplies a stable surface slot and geometry; the producer owns its graphics
API and render targets. A host presentation plan interleaves CPU paint ranges and
external surfaces in visual order. It must not simply put every GPU surface above
the committed CPU buffer, which would cover menus and tooltips.

The contract needs a versioned surface lease, logical/pixel size, crop and supported
transforms/clips, alpha/color interpretation, damage, readiness/release synchronization
and lifecycle handling for resize, hiding, device loss and reload. Input remains
routed through the shared presented geometry. GPU animation alone need not repaint
unaffected CPU content. Account GPU allocations separately from RAM raster caches.

Wayland sub-surfaces provide compositor-managed stacking and synchronized updates;
they are not automatically clipped to their parent. They are one potential host
backend, not a portable guarantee of arbitrary masks, group effects or zero-copy
scanout. [Wayland sub-surface protocol](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_subsurface)
More demanding composition may require a GPU final compositor that samples CPU
layers and GPU targets. That is different from reading GPU pixels into Blend2D.

External surfaces split CPU materialization scopes; do not flatten a parent across
one without an explicitly supported compositor operation. Capability negotiation
must reject unsupported effects or expose an intentional fallback, never hide a
mandatory readback. Capturing GPU pixels for diagnostics is a separate opt-in path.
The current desktop host uses winit/softbuffer; it does not implement this backend.
Preserve the boundary during this redesign, but defer GPU dependencies and concrete
surface-handle APIs until there is a real producer to validate them against.

### Immediate And Deferred Paths

The immediate path applies cheap cached analyses and builds a correct executable
plan. Optional costly analysis and preparation may run on workers using immutable
snapshots. New UI state does not wait for optimization; replay current commands
when a matching optional raster is unavailable. Do not display stale cache pixels
as a substitute for current state.

Use stable local-coordinate programs as the unit of deferred analysis, not a whole
screen snapshot. Replay indexes, equivalent fragments and safe simplification can
survive scrolling or placement-only animation; current damage and occlusion cannot.
The existing background replay-index analysis is the starting point for further
tiers, not a reason to introduce another optimizer. Admit additional analysis only
when its expected reuse can repay the work, with source identity checked at publication.

Freeze metadata and versioned resource references, not live `Rc` font caches or
UI callbacks. Thread-safe ownership is an implementation requirement before worker
rasterization, not an `unsafe Send` assertion. Coalesce requests, bound in-flight
bytes/jobs, publish only matching dependency versions, and discard obsolete work.
Installing a plan must not change input/hit geometry or restart animation.
No background whole-scene scanner or polling while the host is idle/hidden.

Animation uses the existing sampled presentation properties. Transform-only motion
can reuse paint; geometry animation may invalidate layout; color/text animation
changes paint. Motion informs cache benefit without making content static. Keep
the native compositor's buffer age and repair history outside content-cache state.

## Implementation Plan

`retinue-render` owns programs, analysis, materialization and execution;
`retinue-ui` owns structure, layout, motion and recording. Renderer tests use
layout-independent frames; UI tests exercise their layout adapter separately.
Share mechanisms, not page/game policy.
Evolve existing `Command`, `PaintProgram`, `Renderer`, `layers` and resource types.
Do not create a second renderer, renderer selection layer or a new wire format.
Suggested internal responsibilities are programs/properties, analyses, planning,
materializations and execution; split modules when implementing those boundaries.
Existing recording helpers remain under `retinue-ui::legacy` during migration.
Their cache hints are planner inputs; the new authoring API feeds the same frames.

| Phase | Change | Exit Gate |
| --- | --- | --- |
| 1. Baseline and replay | In-process immutable frame capture and Relic replay implemented; extend workload coverage alongside new policies | Identical workload can compare raw, current item and candidate policies; source provenance retained |
| 2. Representation | Frame/program boundary, immutable resource IDs and effects implemented; property API remains | GUI and both overlays lower through the same path with pixel parity; no layout/input rewrite |
| 3. Incremental compiler | Program interning/analysis, sparse consumer index and executable preparation/draw plans implemented; extend sparse plan updates and decisions | Sparse changes visit affected regions; raw replay oracle; no clean-frame scan |
| 4. Unified planner | Explicit materialization recipes/task lifetimes; item/row/tile candidates and partial coverage repair; parent/child consumer tracking | Matched granularity experiments select policy; forced invalidation and memory-pressure tests pass |
| 5. Deferred work | Frozen worker snapshots, bounded optional preparation, promotion/demotion and aggregate scheduling | Stale-result, cancellation, teardown/reload tests; no latency regression hidden by average timings |
| 6. Consolidation | Remove superseded cache paths; document author contracts and diagnostics | No permanently duplicated rendering APIs; all hosts covered, performance results published |

Establish the representation and dependency contracts before adding more cache
special cases. Each phase keeps the app runnable; it is not another root-to-leaf
UI rewrite. Leave raw lowering as an oracle within the same backend, not a second
production renderer.

## Verification

Extend existing [render benchmarks](../../tools/benchmarks/README.md) and GUI
captures. Add IR/plan inspection to those tools: explain a cache choice, changed
dependency, repair region, demotion and resource lifetime, with stable UI keys.
Keep normal-build instrumentation opt-in and bounded.

Compare direct replay, current cells, rows, tiles and selected mixtures using the
same prepared resources, state/input trace and destination-buffer history. Include
Foundry, Relics, Mastery, Market, a mod-card hover and both overlays at 100/125/200%.

- Steady/reversed/jumping scroll, scrollbar drag, sparse timers/image arrivals.
- Insert/remove/reorder, responsive column changes, scale and viewport changes.
- Nested clips, antialiasing, alpha, shadows, fractional placement and effects.
- Open/close/move popups over scroll content; interruption/retargeting of motion.
- Shared children promoted into parents, child still visible elsewhere, demotion.
- Cold/warm caches, working set larger than idle budget, allocation failure.
- Stale worker results, replaced resources, hidden surfaces, reload/teardown.
- Rotating/aged destination buffers; full redraw after buffer loss.

Use unoptimized semantic replay as the pixel oracle. Require exact equality where
the existing path is exact; any unavoidable intermediate-format rounding tolerance
must be narrow, explained and tested separately, never a blanket screenshot
tolerance. Test graph lifetime/order validation and bounded retained allocations.

Run alternating release comparisons with no overlapping test builds. Report host
load and distinguish CPU render results from compositor/game impact. Measure
p50/p95/p99/max for scene updates, analysis, preparation, submission, **native
completion**, and presentation separately. Include total wall latency, caller and
all-worker CPU at matched cadence, repaired/copied/composited pixels, allocations,
and peak/live/retained/pending bytes. Fewer commands alone is not a win.

Do not freeze tile sizes, promotion thresholds or worker counts without those
measurements. The architecture must permit changing them without changing pages.
