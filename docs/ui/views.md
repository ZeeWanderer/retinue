# Views And Identity

[api:Layout]: retinue_ui::layout::Layout
[api:ForEach]: fn@retinue_ui::authoring::ForEach
[api:ViewProperties]: derive@retinue_ui_macros::ViewProperties

Use `View` for a composite description with a `body`. Use structural layouts to
arrange children and primitives for drawing. These are distinct contracts:

| Contract | Responsibility |
| --- | --- |
| `authoring::View` | Describe content from inputs and observed data |
| [Layout](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/layout/trait.Layout/) | Measure and place declared children |
| [Primitive](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/view/primitive/trait.Primitive/) | Resolve immutable drawing from input data |
| `authoring::Body` | Describe structure and bind its changing inputs to a shared template |

Use `#[view]` on a body or `ui!` for a layout expression. Nest stacks and
conditionals directly. Use `compose(child)` for an ordinary child view and
`ForEach` for a changing collection. The [first view](first-view.md) shows the
syntax in a complete example.

## Preserve The Right State

Static children have structural identity; keyed children have an explicit identity.
Use domain IDs for reorderable rows, not visible cell indices. Reorder within the
same parent preserves owned state; removal, a different type or a different key
releases it. Moving a key to another parent is not a state transfer.

`context.state` retains a local field. `context.owned` retains a model handle.
`ViewProperties` gives those declarations named members alongside environment
inputs. A passed-in model can remain an ordinary view member. Put data that must
outlive navigation in an owner above the departing page.

Only active branches mount. Changing pages does not evaluate all possible
pages or keep their resources alive speculatively.

In the compiled API (`retinue_ui::authoring`), use `ForEach` for ordinary
keyed composition. For a virtual window backed by `Items<T>`, use
`ItemViews::<CardView>(items.clone())` when each row should observe its own
inputs. Implement `ItemView::from_item(key, item)` on the row view; read the
provided `State<T>` in its body. Publish window changes with
`items.set_order_with(order, |index, key| ...)` to construct only entering
rows, and use `items.set(key, value)` for edits to retained rows.

## Compose In Order

Modifiers wrap the preceding description. Padding before background includes the
padding in the decoration; background before padding does not. Background and
overlay use the primary child's measurement rather than enlarging it. Use
explicit layout or a custom `Layout` when decoration should affect sizing.

An `offset` changes placement, not model data. Custom drawing uses local
coordinates; the host resolves placement, clipping and display scale. Do not
flatten logical layout into rounded pixels or rebuild an input tree separately.

See [layout](../developer/declarative-layout.md), [state and bindings](state.md)
and the [detailed retention contract](../developer/ui-contracts.md#retained-views).
