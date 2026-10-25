# Composition Prototype

Standalone exercise and benchmark for `retinue-ui/composition`, also used by the
GUI and ducat tooltip. This tests [composition policies](ui-composition.md), not a
new Foundry appearance.

## Run

```sh
make composition-demo
make composition-demo COMPOSITION_ARGS='--scale 1.25'
make composition-demo COMPOSITION_ARGS='--capture'
make bench-composition
make test-composition
```

The window supports tab selection, scrolling, Space to change an amount, R to
replace artwork and C to evict composed rasters. A simulated image arrival occurs
after two seconds. Tab/arrow navigation and accessibility use resolved geometry.
No daemon connection or game required.

Captures at 100%, 125% and 200%, card/tab crops, matching geometry JSON and benchmark reports go
under `research/retinue/ui-composition/`. `COMPOSITION_OUTPUT` selects another
benchmark directory. `RETINUE_BENCH_SAMPLES` controls sample count. Benchmarks use
four total rendering threads for continuity with the original prototype baseline.
The desktop host now prefers six; real-page `make bench-gui` defaults to that policy
and accepts `GUI_BENCH_THREADS=1..8` for controlled comparisons.

## Implemented

- Keyed, retained Taffy tree with cached standard and custom layout algorithms.
- Row, column, grid, stack and anchoring within a resolved frame. Custom containers
  measure/place standard containers and vice versa; no external constraint solver.
- Signed logical geometry, backend pixel snapping, independent paint extents,
  clipping and hit testing. Geometry JSON exposes layout, paint and hit rectangles.
- Immutable local paint chunks with explicit content identities. Translation does
  not re-record them. Named, typed RGB colors avoid the painter's BGRA ambiguity.
- Coordinated keyed selection transitions, retargeting and reduced-motion policy.
- Old/new damage, painter-order repair and existing host buffer-age handling.
- Bounded explicit raster reuse, direct replay fallback and resource accounting.
- A 5,000-item virtual grid that constructs only visible rows plus overscan.

`Paint.key` identifies content, not an element: include every changing resource,
font and paint property in that identity. Equal identities can share rasters across
instances. Logical size and physical scale are added by the engine. This explicit
contract is provisional; there is no automatic resource dependency graph yet.
Prepare the renderer after updating the scene; stale prepared generations are
rejected. Cache preparation is separate from the queued main paint context.

## Verification

Differential tests cover scrolling, interrupted transitions, removal, reordering,
image arrivals, clipped glyphs, hidden elements, scale changes, eviction and three
rotating buffers. Direct partial painting must equal full painting. Optional
source-over rasters allow at most two channel levels of integer blending rounding
for overlapping translucent primitives. Zero-budget caches replay directly.

Benchmarks separate these policies:

| Case | Layout/commands | Painting |
| --- | --- | --- |
| `rebuild` | Recreated each frame | Full |
| `commands-full` | Retained | Full |
| `commands` | Retained | Damaged area |
| `raster` | Retained, explicit composed rasters | Damaged area |

Scroll and counter workloads include periodic image changes. Inputs, pre-resampled
images and thread policy are shared. Reports include p50/p95/p99, process/caller
CPU, memory, cache counters and update/preparation/paint timing samples. They exclude
presentation and game impact. This is not a before/after benchmark of the whole GUI.

## Findings

Retention and damage repair provided the largest benefit. At 125%, counter updates
fell from 0.565ms rebuilding everything to 0.080ms with retained commands and damage.
Explicit rasters helped scrolling (0.431ms to 0.355ms) but did not help counter
updates. Keep raster caching opt-in; viewport-pixel copying is not a prerequisite
for starting GUI migration. These are prototype measurements, not whole-GUI results.

Pixel comparisons alone missed incorrect layout shared by both painting paths.
Geometry inspection exposed zero-sized leaves in nested custom containers. Keep
geometry assertions, real-composition tests and visual captures together.

Production integration added namespaced identities, direct custom paint and
presentation patches that skip description reconstruction. Isolated group opacity
and exit lifetimes remain unimplemented. The example's simple hit testing is not
the GUI's complete input model.

## Integration Discipline

The GUI migration proceeded root-to-leaf; use real application needs to refine the API:

1. Keep the desktop host, scheduling, resource workers, preferences and page models.
   Integrate composition at the shell: scale, title bar, three rails, clipping and
   overlay order. Existing pages initially occupy custom leaf boundaries.
2. Convert chrome, navigation and rail transitions. Give painting, input,
   accessibility and captures one resolved geometry source. Preserve native
   move/resize, focus, modal routing and existing application behavior.
3. Descend through one representative page, Foundry, from its containers into tabs,
   virtual grid, cards and anchored parts. Extract reusable compositions when needed;
   custom layout and painting remain supported. Validate this branch before copying
   its patterns across other pages and the right rail.
4. Migrate remaining branches and remove superseded paths as each is accepted.
   Exercise one overlay composition before treating the shared API as settled.

Use namespaced element/content identities and the explicit invalidation contract.
A custom-page adapter must paint directly
through the existing Painter with declared bounds, clip, revision and resource
lifetime, not rasterize an entire page merely to embed it. It must bridge existing
input and accessibility rather than install a competing event system. Unknown paint
dependencies require conservative invalidation and prohibit optional raster reuse.
Replace unnecessary adapters as their contents migrate; the general custom-node
escape hatch remains supported.

Add group-opacity isolation or exit lifetimes before a migrated feature needs them;
do not substitute per-child alpha for group compositing. Optimize subtree updates
where real integration exposes unnecessary work. Scroll-pixel reuse and automatic
raster promotion remain measured, optional follow-ups, not migration gates.

For each branch, test resizing, fractional scaling, interrupted animations, input,
geometry and partial/full painting equivalence. Compare the real page's timings and
resource use, not only the synthetic scene. Keep the app usable between steps.

The layout adapter follows Taffy's maintained
[custom-tree interface](https://docs.rs/taffy/0.12.2/taffy/#low-level-api).
That interface is needed for nesting custom algorithms, not because duplicate
tree storage has been demonstrated to be a bottleneck.
