# UI SDK

`retinue-ui` provides the authoring frontend for the desktop GUI and both in-game
overlays: observable models, bindings, inherited environment and retained
`View::body` evaluation. Named stacks, ordered modifiers, Text, Image and Canvas
feed the same layout and render mechanisms. The old builder frontend is removed.
Some application content deliberately uses lower-level pixel drawing and native
input integration. The [drawing checklist](ui-drawing.md#current-baseline-and-checks)
distinguishes implemented primitives from planned coverage.

## Organization

Keep the frontend's responsibilities distinct:

- `state`: observable fields, dependency tracking and subscription lifetimes.
- `environment`: named values and type-keyed model providers with scoped lookup.
- Authoring: `View`, primitive descriptions, modifiers and the `Element` boundary.
- Layout: named structural containers and custom `Layout` implementations.
- Runtime: keyed state, body evaluation, reconciliation and lifecycle.
- `retinue-ui-macros`: compile-time model authoring, using the same state runtime.
- `retinue-render`: layout-independent draw IR, repair planning, resources,
  cache policy and Blend2D execution, including the native bridge and dependencies.

The frontend modules are `view`, `runtime`, `state` and `environment`; `prelude`
exports the authoring vocabulary. Reusable layout contracts live in `layout`,
roles in `semantics`, and layout-to-frame adaptation
in `scene`. `drawing::PixelCanvas` and `PixelLayers` support custom pixel layout;
`runtime::ViewSurface` retains those drawings through the same ViewTree. They are
lower-level SDK tools, not a second authoring runtime or renderer.

These are internal APIs, not an external compatibility promise. Expose useful
lower-level mechanisms and revise boundaries when real usage warrants it. Keep
boundaries that preserve ownership and correctness: frame content and placement
patches, for example, share damage calculation and a renderer change journal.

[UI motion](ui-motion.md) defines the planned shared animation model: scoped
state transitions, transaction propagation, phases/keyframes and playback control.
It depends on stable view/property identity and observation change metadata.

In wfgui, put each View and custom Layout in its own named file, grouped by page
or reusable feature. Keep its model and actions alongside the views, and
task/client integration outside presentation. See the [source map](gui-organization.md).

## Retained Views

`View::body` returns a description. Composites implement `View`; structural and
graphical primitives implement `IntoElement` without inventing another body.
`HStack`, `VStack` and `ZStack` accept tuples, arrays or vectors. Ordinary Rust
`if`/`match` works; use `into_element()` only when branches have different types.
There is no required `declare()` call.

Optional `ui!` child blocks use the same constructors and runtime:

```rust
use retinue_ui::prelude::*;

let expanded = true;
let names = ["Karak", "Skana"];
let view = ui! {
    VStack(8.0) {
        Text("Equipment").font_size(20.0)
        if expanded {
            for (id, name) in names.into_iter().enumerate() {
                Text(name).id(id as u64)
            }
        }
    }.padding(12.0)
        .background { Rectangle(Color::rgb(0x252b30)).corner_radius(8.0) }
};
```

Capitalized calls in view positions abbreviate `Type::new`; regular constructors
and struct literals also work. Modifier arguments are ordinary Rust expressions;
`background { ... }` and `overlay { ... }` supply a single child. Child blocks
support `let`, `if`/`else`, `match` with braced arms, and `for`. Conditional/loop
groups preserve the identities of following siblings but add no layout box.
Use stable item keys for reorderable collections. An ordinary Rust block is an
expression escape hatch, not code whose internals are rewritten by the macro.

```rust
use retinue_ui::prelude::*;

#[derive(PartialEq)]
struct Meter { level: State<u32> }

impl View for Meter {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        HStack::new(8.0, (
            Rectangle::new(Color::rgb(0x52be8c))
                .frame(self.level.get() as f32 * 10.0, 20.0),
            Rectangle::new(Color::rgb(0xd3b560)).frame(20.0, 20.0),
        ))
    }
}
```

`runtime::ViewTree` retains identities and the scene. Its wake callback schedules
work; `update(size, scale)` evaluates dirty bodies and publishes one frame/damage
update. Equal inputs skip body evaluation, while observed changes invalidate the
consumer independently. Resize alone does not reevaluate bodies. Changed source
descriptions and changed primitive output are distinct: a child's new color does
not require projecting its enclosing layout again.

Identity is type plus structural slot, or an explicit sibling `id`. Keyed
reordering preserves owned fields; removal or a type/key change releases them.
Keys do not move state between different parents. Only active branches mount.
Composite and environment nodes do not add layout boxes. Disjoint scene subtree
patches commit together so the renderer receives every change in one journal.
Errors retain pending work for an explicit retry or corrected inputs; they do not
schedule an immediate error loop. Body panics unwind observation scopes and retain
the previous published frame.

`context.state("field", initializer)` retains local state;
`context.owned("model", initializer)` retains a cloneable model handle. Initializers
run once per identity and do not subscribe the body to their reads.
`context.properties::<Fields>()` resolves a typed declaration without string slots:

```rust
#[derive(ViewProperties)]
struct EditorFields {
    #[owned(EditorModel::new())]
    model: EditorModel,
    selected: State<Option<u64>>, // Default initializer.
    #[environment]
    session: SessionModel,
    #[environment(Accent)]
    accent: Color,
    #[environment]
    optional_service: Option<ServiceModel>,
}
```

In `body`, `let fields = context.properties::<EditorFields>();` provides ordinary
members: `fields.model.title.get()`, `fields.model.title.binding()`, `fields.accent`.
Owned fields retain cheap handles per view identity and property type. Initializers
run once; removing the view releases ownership. Environment members resolve the
nearest current provider; a required missing model fails explicitly. Named entries
use their defaults. Resolution subscribes to provider/key changes; model field
dependencies still come only from fields actually read. Declare only the inherited
values that this view needs. No payload copying or hidden field-access rewriting.

Use the explicit `owned` initializer when it needs the view's inputs, or implement
`ViewProperties` yourself. Passed-in models stay normal view members. Property
declarations are optional and don't change the `View::body` contract.
`context.environment()` exposes inherited named entries
and model handles; `.environment` and `.environment_model` override descendants.
`LayoutView::new(layout, children)` uses the existing custom `Layout` contract.
Changing its algorithm preserves keyed child identity and releases the old
algorithm's cache. `with_algorithm` accepts an erased `LayoutAlgorithm` when a
runtime choice needs different implementation types.
Pass comparable layout parameters from the body; reads inside `Layout::measure`
or `place` do not yet establish their own observation phase.

Keep fractional measurements and placements. Do not ceil text blocks or quantize
layout to reproduce an older screenshot. A future explicit pixel-grid alignment
policy belongs at the rendering boundary and must account for display scale.
Native pixel rectangles round shared edges, not origins and sizes separately.
Input uses fractional logical bounds and a separate ancestor clip; never convert
rounded pixel bounds back into a logical input clip.
Damage follows actual paint bounds, including newly published images, not hit areas.

`Layout::render_context` supplies advisory, typed rendering metadata. It refreshes
with layout parameters or declared children, not on every scroll offset. A
`layout::rendering::Collection` partitions child indices into stable keyed rows;
the scene resolves these to paint ranges and the current content origin. Invalid
partitions are ignored, never used to change drawing semantics. `UniformGrid`
provides row metadata; `.reuse(CacheHint::Conditional)` lets the renderer evaluate
row caching without forcing it. Other annotations pass through unchanged.

For known child rectangles, `Positioned` with `Placement` layout values places
children directly. It avoids separate size/offset wrappers for each child without
changing the ordered semantics of ordinary modifiers. Unpositioned children fill
the container; explicit frames are local to it. Use negotiated stacks/anchors when
the layout needs their sizing behavior.

The scene owns resolved layout, clipping and hit geometry. It submits placement,
content and optimization-context changes in one renderer transaction; it does not
rebuild a duplicate renderer property tree from already-resolved coordinates.
Drawing commands stay local to their content. Viewport clipping belongs to placement,
not the recorded source, so scrolling does not invalidate static card drawings.
The frame retains a bounded journal across multiple publications before render
preparation. A renderer falling behind that journal safely rebuilds its indexes.

`ImageResource` publishes immutable renditions through observed pixel state;
metadata, loading and freshness have separate dependencies. Host acquisition stays
outside drawing. Wfgui's `media::views` connects its existing thumbnail worker to
these resources: active views own shared demand leases, unmount releases demand,
and stale pixels survive a descriptor refresh until its replacement arrives.
Image arrival updates the primitive, not the whole page's draw bindings. Ordinary
`Image` and custom `Primitive` drawing can use the same publication mechanism.
Raster reuse is advisory: measure total preparation plus replay, not only fewer
draw calls. Current listing grids replay directly because row materialization was
slower in the scrolling benchmark; shared expensive glow fragments still reuse.

Run `make view-sdk-demo` for offscreen captures at 100/125/200% and an invalidation
report. Its source is [the retained example](../../crates/retinue-ui/examples/retained/).
Backend-neutral input routing, timeline updates and the first
presentation-animation layer are implemented.

`padding`, `background`, `overlay`, `frame`, `offset` and `clipped` preserve modifier
order. Decorations do not contribute to the primary child's measured size.
Offset changes presentation, not sibling layout. `layout_value` exports to the
immediate arranging parent without adding a box; `role` and `window_action`
annotate any layout rather than requiring a special window container.

Frames propose their resolved size to their child, then align the returned size.
`frame_aligned` selects a noncentral anchor. `ZStack` proposes the same available
space to each child; flexible drawing fills that space while fixed-size children
keep their dimensions. This is explicit layout behavior, not CSS grid track sizing.

`input(InputOptions)` installs a target for pointer capture, focus and routed
events. `.interactive()` also publishes a target, with primary-button release
activation. Use `ViewTree::dispatch` and `input_key` to route app-owned actions;
`Activation::Press` supports press-triggered controls. Optional `Interaction`
fields expose observed hover, capture and visible keyboard focus. Semantic
annotations add no layout boxes. `ViewTree::unique` resolves an authored key when
it is unambiguous; `geometry`, `authored_key` and `inspect` provide diagnostics
without treating sibling-scoped IDs as globally unique names.
Use `ViewTree::hit_test` for pointer lookup and keyed geometry for known targets.
Do not materialize `named_geometry()` or `inspect()` on every paint or
pointer event; those enumerate the tree for diagnostics and bulk inspection.

Custom drawing may publish `ControlRegion<K>` through `Controls<K>`, using the
same router. Publish one complete snapshot after layout, back to front. Region
identity is not action identity: distinct regions may invoke the same action.
Do not publish an empty snapshot during a rebuild. Moving a retained target
preserves capture; removing or disabling it cancels capture. Release activation
requires the original pressed region to remain under the pointer. Disabled
opaque controls block underlying targets without activating them.
Geometry-only changes retain identity indexes; paint-only updates retain the
whole input snapshot. `UpdateStats::input_updates` exposes actual publications.

Native coordinate conversion, modal scopes, editing and game/window focus policy
remain in hosts. Convert coordinates once and route motion, press, release and
cancellation consistently. A pointer leave need not end capture; focus loss must.
Recompute hover after layout changes even without pointer motion. Prepare input
geometry independently of rasterization; caching or partial repair must not omit
targets. PoE's surface host prepares layout before painting, once a presentation
buffer is available, so buffer backpressure cannot advance actionable geometry.

Useful precedents: [Flutter's layout-based hit testing](https://api.flutter.dev/flutter/rendering/RenderBox-class.html),
[stationary hover refresh](https://api.flutter.dev/flutter/rendering/MouseTracker/updateAllDevices.html),
[Slint's retained capture path](https://github.com/slint-ui/slint/blob/d0e1c7a009027bfe199514750958348c8c2487ab/internal/core/input.rs),
and [Iced's overlay-first dispatch](https://github.com/iced-rs/iced/blob/8ff48c42b0d5188223501ea1ccc506fd5beaf36a/runtime/src/user_interface.rs).

`update_at` accepts one `FrameTime` for layout, recording and presentation.
`context.time(schedule)` registers a deadline only for its consuming body or
primitive. `next_deadline` includes timelines and motion, so idle trees need no
animation polling. Wall-clock corrections invalidate wall-clock consumers only.
Keep paint-only clocks in the drawing primitive, not the containing layout body.
Compare resolved drawing values rather than elapsed time so an unchanged displayed
colour or shape does not invalidate paint.

`.animation(curve, trigger)` scopes interpolatable changes; mutation transactions
work outside the tree. Known geometry and rectangle paint properties animate
without reevaluating parent bodies. `context.animated` and `AnimatedCanvas` support
custom interpolated values. Retargeting starts from the displayed value. Reduced
motion and disabled transactions snap; incompatible properties are discrete.
`Switch::new(key, content)` mounts the selected content. An optional
`TranslationTransition` retains at most one outgoing page, disables its input and
releases it when the transition ends. Reversal preserves current presentation;
rapid navigation drops the oldest outgoing page. `Switch::vacant()` allows exit
without a replacement. Initial selection is immediate. No inactive page bodies or
alternative trees are built speculatively.

`Keyframes` sequences interpolated targets, holds and jumps; `parallel` combines
tracks on one playhead. `Playback` offers play/pause/seek/reverse/restart/cancel.
Sample with `context.playback(&player)` in a View or Primitive; it registers only
that consumer's next deadline. Holds skip idle frames. A `Run` reports completion,
cancellation or supersession through ordinary observation, not reentrant callbacks.
Keep `PlaybackOwner` in owned view properties when unmount should cancel even if
external controls survive. Host-owned `Playback` has an explicit separate lifetime.
Springs and automatic velocity-preserving program retargeting remain pending.

## Drawing And Damage

See [UI drawing](ui-drawing.md) for the proposed context API, compositional/direct
drawing choices, Blend2D coverage and text/asynchronous-image resource contracts.

The planned drawing API has layered control, with automatic conservative damage
as the default. Separate content invalidation (what must be reevaluated/recorded),
paint footprint (where an operation can affect pixels), frame damage (what changed)
and buffer repair (what the acquired buffer needs). These are not interchangeable.

- Shapes, text and images derive paint footprints from typed commands. Include
  stroke caps/joins, glyph ink, antialiasing, transforms and effect expansion;
  layout rectangles or text advances alone are not sufficient paint bounds.
- Custom Canvas content records the same typed IR in local coordinates. Changed
  inputs invalidate recording; retained fragment identity, revisions and resource
  generations allow unchanged work to be skipped. Compare changed fragments where
  useful, with the whole old/new fragment footprint as the safe fallback. Do not
  require a key or hand-authored dirty rectangle for every primitive, or a full
  deep command comparison for every frame.
- Opaque native drawing declares its write footprint and any destination-reading
  dependencies, or uses an enforced bounded surface/clip. On invalidation, damage
  its complete affected footprint by default. An optional producer-supplied local
  damage region is a correctness contract, not a best-effort performance hint.
  Unknown/unbounded effects require widening to the affected render target.

Custom effect primitives need output bounds and input-dependency mapping: a blur
or backdrop effect can spread a local input change beyond that input's rectangle.
Normal callers may add invalidation, but cannot accidentally suppress automatic
damage with a too-small hint. Trusted partial-update contracts remain explicit
and can be checked against full redraw in debug/test tooling.

Movement, removal, clipping, reordering and resource changes must account for
both previous and current affected output. Transform local regions to the target,
expand for actual sampling/effects, round outward and apply effective clips.
Do not immediately collapse separated regions into one enclosing rectangle.

The downstream path is frame damage -> pending damage -> acquired-buffer history
-> repair planning -> execution. Repair planning can merge regions or choose full
repaint based on cost. Preparation happens once; replay intersecting content in
painter order, including unchanged background/foreground needed to reconstruct
damaged pixels. Presentation reports current-frame damage separately from repairs
needed only because an acquired buffer is older.

Canvas records content independently of a transient repair clip, so later
uncover/scroll repair still has its commands. A low-level execution callback may
receive a repair clip for culling, but must not advance animation or mutate model
state: execution can happen multiple times or be skipped. Bounds describe possible
writes, not proof that pixels changed or that the area is fully opaque.

Canvas records typed commands and frame diffing derives damage at paint-instance
granularity. Fine-grained changes within one custom drawing remain a separate
optimization. Verify incremental
output against independently constructed full renders, including movement,
removal, resource updates, effects, scaling, overlap and aged buffers.

Relevant precedents: Flutter distinguishes a custom painter's input changes from
whether painting is actually needed ([shouldRepaint](https://api.flutter.dev/flutter/rendering/CustomPainter/shouldRepaint.html));
Qt exposes explicit bounds guarantees for custom native render nodes so its
software renderer can preserve partial updates ([QSGRenderNode](https://doc.qt.io/qt-6/qsgrendernode.html#RenderingFlag-enum)).

## Models And Actions

Models do not inherit a controller class or require whole-object comparisons.
Declare fields normally; `#[observable]` supplies shared property storage:

```rust
use retinue_ui::observable;

#[observable]
struct Equipment {
    rank: u32,
    bonus: u32,
}

impl Equipment {
    fn total(&self) -> u32 {
        self.rank.get() + self.bonus.get()
    }
}

#[observable]
struct Selection {
    item: Equipment,
}
```

A tracked read of `selection.item.read().total()` subscribes to the selected
reference and that item's two fields. Changing an unrelated item does nothing.
After selection changes and the consumer reevaluates, the previous item's fields
are no longer dependencies. Computed getters compose without annotations or a
second cached-data model.

Use separate observable fields when consumers need separate invalidation.
`State<LargeSnapshot>` observes the entire field, not individual members hidden
inside it. Collection order/membership and fields of individual keyed items
should likewise be separate. Borrow large data with `read()`; `get()` clones its
value and is intended for small values or handles.

Views describe UI and bind events. Actions or model methods mutate state outside
body evaluation. Host-owned tasks perform asynchronous client work and publish
results on the UI thread; canonical data and domain computation remain in the
daemon. View-local state belongs to retained identity through `ViewContext`,
not a new `State` allocation on each call to `body`.

## Observable Authoring

Fields are observable by default. Exclude bookkeeping that should not invalidate
UI explicitly. Place `#[observable]` before other derives:

```rust
use retinue_ui::observable;

#[observable]
#[derive(Debug, Default)]
struct Equipment {
    name: String,
    rank: u32,
    #[observation(ignore)]
    request_generation: u64,
}

let item = Equipment::new("Karak".into(), 0, 0);
let same_item = item.clone();
item.rank.set(1);
assert_eq!(same_item.rank.get(), 1);
let name = item.name.read(); // Borrow; no String clone.
let rank_editor = item.rank.binding();
```

The generated model is a shared handle with immutable access to property handles.
Fields retain their declared visibility; values change through `set`, `replace`
or `update`, not by replacing a property's storage. Construct with `new` in field
declaration order. Generic payloads need not implement Clone or equality; reads
can borrow and writes can use `replace`/`update`. Stored data must be `'static`.

Clone and equality compare model identity, not snapshots. Optional Debug and
Default derive from the fields; Hash uses identity and remains stable during
edits. Copy and ordering derives are rejected: order field values or snapshots
instead. The macro resolves renamed crate dependencies; `#[observable(crate =
path)]` overrides resolution when needed.

`field.read()` borrows, `field.get()` explicitly clones, and `field.binding()`
edits the same source. Looking up a property or constructing its binding does not
subscribe; reading does. Computed methods track their actual field reads. Ignored
fields use `Unobserved<T>` with the same value-access methods but no binding or
registrar. Their replacement does not notify; reads of nested observable objects
still track normally.

Rust cannot intercept ordinary payload field access like Swift properties.
These explicit access methods avoid hidden clones, unsafe dereferencing and
rewriting arbitrary Rust expressions. Manual `State<T>` remains available for
local state and lower-level integrations; both paths use the same runtime.

## Ownership And Bindings

Use the same observable model in each ownership context:

- View-owned: a declared member is initialized once per retained view identity.
  Body reevaluation must not recreate it; identity removal releases ownership.
- Passed-in: a view member receives a shared model handle. No additional wrapper
  is necessary just to observe it. Replacing the handle changes its dependencies.
- Environment: a declared member resolves a named entry or type-keyed model from
  its inherited scope. A root injection makes it app-wide without a singleton.
- Outside views: ordinary actions and host services may own/read/write the same
  handles. Observation does not depend on construction inside a view body.

View ownership is the normal feature-local authoring path, not an advanced host
setup requirement. A view should be able to declare and initialize its model
member; the runtime supplies stable storage. Passing that model to a child does
not transfer ownership or require environment injection. App/session-wide models
can instead be owned at the root. This corresponds to SwiftUI's distinction
between an owned `@State` model and an ordinary model property received from a
parent, described in [Managing model data](https://developer.apple.com/documentation/swiftui/managing-model-data-in-your-app).

Read-only field access and a writable binding refer to the same source. A binding
does not allocate a second state cell or synchronize copies in both directions.
Generated properties provide bindings without handwritten getter/setter closures;
custom projections remain an escape hatch. This writable
binding edits the source property; it does not own a second copy of its value.

Nested bindings must describe a live path, not capture the current leaf by
accident. Replacing a selected item redirects reads and writes to the replacement.
Optional paths and deleted keyed items need explicit unavailable-state handling;
never redirect an edit to a different item now occupying the same list index.
Binding creation alone must not subscribe an unrelated parent view. Consumers
subscribe when reading, while actions write through the binding outside body.

### Writable Binding Foundation

`ui::state::Binding<T>` edits existing state without another value cell:

```rust
use retinue_ui::{observable, state::State};

#[observable]
struct Item {
    rank: u32,
}

let selected = State::new(Some(Item::new(1)));
let rank = selected.binding().try_follow(|item| {
    item.as_ref().map(|item| item.rank.clone())
});
assert_eq!(rank.set(2), Ok(true));
selected.replace(None);
assert!(!rank.is_available());
assert!(rank.set(3).is_err());
```

`get()` explicitly clones; `with()` borrows through a callback without requiring
the payload to implement Clone. `set()` suppresses equal writes, `replace()`
returns the old value without comparison, and `update()` mutates in place with
the same conservative invalidation-on-unwind behavior as State. Access returns
`Unavailable` when a dynamic target disappears; writes do not silently redirect.

`follow()` and `try_follow()` resolve an observable child field on each access,
so replacing a parent redirects future reads and edits. `map()` projects into
one stored value and therefore retains that source field's observation granularity;
its read/write projections must address the same field. `resolve()` is the lower
level escape hatch for environment or other dynamic paths. Resolvers/projections
must be side-effect free. Binding construction does not evaluate these paths.
Read resolution is tracked; writes are untracked.
Use stable item identity when resolving a collection member, not its old index.

## Observation Contract

`Observation::track` is the low-level runtime boundary. The View host calls
it automatically; application views do not manage observers.

- Reads register with the current synchronous tracking scope. Ordinary getters
  inherit it; explicitly nested consumers own their own reads.
- Successful reevaluation replaces dependencies and reuses unchanged
  subscriptions. No subscriptions to inactive conditional branches accumulate.
- Writes mark consumers dirty and call their scheduling callback once until the
  next evaluation. The callback must enqueue work, not evaluate UI synchronously.
- Every contributing write carries a mutation sequence and `Transaction` intent.
  Coalescing a wake does not discard a later write's intent. `with_animation` and
  `with_transaction` capture this at mutation time; `ViewContext::mutation` exposes
  the latest contributing cause, not the ambient transaction at rendering time.
  This is propagation only: presentation tracks are not connected to the new host.
- `set` suppresses equal values. `replace` and `update` invalidate without cloning
  or comparing the old value. An in-place update invalidates even on a no-op or
  panic, because a partial mutation may already have happened.
- A mutation after a read but before subscription installation remains detectable
  through the source revision. Body mutation is still not an application pattern.
- Subscription handles do not retain model payloads. Dropping the observer
  unsubscribes it; a failed evaluation preserves prior dependencies and remains
  dirty. Tracking scopes restore correctly during unwinding.
- State handles compare by identity, not by data. This supports comparable view
  inputs without copying or deeply comparing the model.
- `untracked` explicitly excludes incidental reads. Generated properties and
  manual State fields use this same mechanism.

State and observations are UI-thread objects. Worker results arrive through the
host event path. Tracking does not extend across `await`, add a polling timer,
or need a global lock. Cached derived values and atomic state transactions are
separate future features. Animation transactions provide neither rollback nor
database atomicity. A body write queues another host turn, not recursive evaluation.

## Outside A View Tree

Observable data must also work in ordinary code with no environment, window or
View runtime. Reading without a tracking scope simply reads data. Mutations
still notify any subscribed consumers, regardless of who owns the model.

For a standalone reactive computation, retain an `Observation` and evaluate the
computation through `track`. Its callback schedules reevaluation using the
caller-owned event/task mechanism. Each evaluation replaces its dependencies;
dropping the observation cancels subscriptions. The View host uses this same
mechanism automatically around body evaluation, not a separate observer system.

Swift's basic
[`withObservationTracking(_:onChange:)`](https://developer.apple.com/documentation/observation/withobservationtracking(_:onchange:))
provides standalone access tracking. That overload invokes a one-shot callback
before mutation and cancels its tracking; continued observation is reestablished
by evaluating again. Retinue deliberately uses post-mutation notification and a
retained observer with coalesced scheduling until the next evaluation. Neither
is an every-write event log. Do not synchronously run UI or depend on intermediate
values in a scheduling callback, and do not imply tracking continues across
`await` or arbitrary worker threads.

## Inherited Environment

`ui::environment::Environment` provides typed entries for both values and shared
observable model handles. A descendant uses the nearest definition of a key;
overrides affect only that subtree. `ViewTree` retains provider scopes; ordinary
code can also use them independently. `ViewProperties` declares inherited members;
explicit `context.environment()` lookup remains available.

Create a root with `Environment::default()` and a descendant scope with `child()`.
Named keys implement `EnvironmentKey`; `get::<K>()`, `set::<K>()`, `replace::<K>()`
and `remove::<K>()` access local providers and inherited values. `get` reads the
nearest provider; writes install/remove only the receiver's own override.

For a single model per type, use `provide(model)`, `model::<T>()` for optional
lookup and `require::<T>()` for a typed missing-provider error. `provide` suppresses
equal values; observable handles should compare identity. `replace_model` supports
types without equality and always invalidates. `remove_model` exposes any inherited
provider again. Lookups clone the provided value, so shared models should be cheap
handles, not entire datasets.

The key trait is shared with the existing authoring API. The new scopes use the
same State/Observation registrar as direct model access, not the old read-only
binding evaluator. Only visited provider slots are dependencies, including absent
nearer overrides. Defaults are initialized once per root/key and shared with its
descendants; separate roots remain independent.

Support named entries with declared defaults and type-keyed observable-model
injection. Named entries distinguish multiple roles using the same model type;
type-keyed lookup avoids needless key declarations for the common single-role
case. Required missing models produce a useful diagnostic; optional lookup is
explicit. Do not silently construct an empty model when a required one is absent.

A statically declared entry defines its key/type/default policy, not process-wide
mutable storage. Root injection supplies an app-wide instance, while subtrees,
windows and tests may override it. A shared default model, if explicitly allowed,
must have defined scope ownership and not be newly constructed on every lookup.
Environment reads do not mutate the provider slot. A model obtained through it
can still expose writable bindings; replacing the slot belongs to its provider.

Resolution and model observation are separate dependencies. Replacing the
resolved model invalidates its consumers so their field subscriptions can move
to the new instance. Editing a field invalidates only consumers that read it.
An ancestor change hidden by a nearer override must not invalidate that branch.
Consumers of missing/default entries must notice when resolution changes too.

The runtime implements scope inheritance and model observation. Both declared
properties and explicit lookups use it. Tests cover nested overrides, sibling
isolation, replacement/removal, defaults and subscription cleanup.

The two access forms and their inheritance rules follow
[EnvironmentValues](https://developer.apple.com/documentation/swiftui/environmentvalues)
and [Environment](https://developer.apple.com/documentation/swiftui/environment).
These are behavioral references, not a commitment to reproduce Swift syntax or
its process/system integration.

The model follows [Swift Observation's access/mutation tracking](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0395-observability.md)
and scoped dependencies. Rust model syntax generation uses the same explicit
observable fields as the lower-level API.
