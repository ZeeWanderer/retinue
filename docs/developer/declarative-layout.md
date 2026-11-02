# Declarative Layout

The authoring API is documented in [UI SDK](ui-sdk.md). `View::body` builds the
active tree; `ViewTree` retains state and patches layout and paint independently.
The superseded builder/Binding/Realization frontend has been removed.

## Structural Tools

- `HStack` and `VStack`: spacing in the constructor, alignment and distribution
  as modifiers. Tuples, arrays, vectors and `ui!` child blocks are equivalent.
- `ZStack`: shared proposal, ordered overlapping children and explicit alignment.
- `frame`, `padding`, `background`, `overlay`, `offset`, `clipped`: ordered
  wrappers. A decoration does not enlarge its primary child's measured size.
- `LayoutView`: a custom `Layout` arranging declared children.
- `ScrollViewport`: content insets, axes, extent and clipping; the controller owns
  input and position. Virtualization is independent of raster-cache granularity.
- `Switch`: current content plus an optional bounded outgoing transition.

A View is a comparable description, not a native window or controller. State and
environment reads establish dependencies during body evaluation or primitive
resolution. Changes notify only readers; equal child inputs skip their bodies.
Mutable reads inside measurement or replay are not implicitly observed. Pass
comparable layout inputs from the body and immutable drawing snapshots instead.

## Custom Arrangement

Implement `Layout::measure` and `place`, then use `LayoutView::new(layout, children)`.
The local `Cache` stores measurement work. `update_cache` runs when parameters or
children change, not for every proposal. Changing algorithm type releases its
cache while preserving keyed child views. `LayoutView::with_algorithm` accepts an
erased `LayoutAlgorithm` for runtime selection between implementation types.

`Proposal` distinguishes ideal, minimum-content and available-space requests.
`Available::Limit` is an offered size, not a hard clipping constraint. A child
may return its fixed/intrinsic size; the parent decides placement. Keep layout,
paint and hit extents separate. `frame_aligned` makes noncentral placement explicit.

`Subviews` exposes authored sibling keys, measurements, placement, typed child
values and alignment guides. It does not expose mutable node ownership. Place
each child once in container coordinates. Measurement must not place children.
Reject invalid geometry; do not publish a partially invalid frame.

`Anchored` supports attachments to a container or sibling's named anchor without
changing painter order. Invalid/missing targets and cycles are errors. `AlignedStack`
aligns a row or column using an `AlignmentId`; children export a typed guide.
Text supplies first and last baseline guides. `layout_value` attaches parent
metadata without adding a layout box. Use custom Layout implementations for
specialized geometry instead of reproducing the tree or measurement engine.

## Collections And Scrolling

`Items<T>` separates keyed row state from order. Updating one row does not notify
order readers or other rows. Reorder preserves row handles; duplicates are rejected
atomically. Model lifetime is separate from view lifetime.

`UniformGrid` supplies fixed/adaptive columns, content extent, visible ranges and
identity-based scroll anchors. Its `arrange(order)` layout places the declared
window using the full logical order. Mount visible items plus a bounded overscan
range, keyed by item identity rather than the recycled cell index. Moving inside
that window changes the scroll transform, not every child's drawing or layout.

Keep scrollbar chrome, floating controls and popups outside the scrolled subtree.
Content insets express end padding without fake cells. Visible assets and selected
rasters have distinct residency rules; neither should be evicted merely because
an unrelated warm-cache byte budget is full. See [render residency](render-residency.md).

## Navigation And Windows

`Switch::new(key, content)` takes active content only. `vacant()` leaves no selected
page. With a `TranslationTransition`, one outgoing subtree remains until its exit
finishes; its input is disabled. Reversal resumes from current presentation. Rapid
navigation retires the oldest departure, and reduced motion finishes immediately.

Inactive-tree inspection is not required. Consider optional metadata only for a
demonstrated optimization that does not complicate Switch or penalize normal use.
Do not evaluate hidden factories, allocate their resources or subscribe them to
state speculatively. Keep reusable model state outside a page when it must survive
unmounting.

Title/content/toolbars use ordinary structural layouts. `role` and `window_action`
carry host semantics independently of layout type. No rigid Window widget is needed.

## Time And Motion

`ViewContext::time(schedule)` observes a shared frame time and registers the next
deadline for its owner. Wall-clock corrections affect only wall-clock schedules.
Unmounting cancels subscriptions and deadlines. A paused or finished schedule
must not drive a repaint loop.

`.animation(curve, trigger)` scopes state-change animation. `with_animation` and
`with_transaction` attach intent to mutations outside views. Target model state
is immediate; presentation tracks interpolate independently. Translation-only
motion should not rerecord drawing. Custom sampled values and `AnimatedCanvas`
use the same clock. See [UI motion](ui-motion.md) for remaining extended playback
work and explicit cancellation/completion contracts.

## Verification

Run `make test-composition`, `make test-gui-app` and affected companion tests.
`make view-demo` exercises the SDK without a daemon; its source is organized by
View. Capture at 100/125/200%, test pointer/focus geometry and compare partial
repairs against a full render through scroll, resize, reorder, image arrival and
interrupted motion. Preserve the same renderer thread policy when benchmarking.

See [render pipeline](render-pipeline.md) for source IR, optimization and execution;
[UI invalidation](ui-invalidation.md) for frame damage versus acquired-buffer repair.
