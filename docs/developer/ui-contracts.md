# UI ownership and invalidation

## Retained views

A compiled view declares structure; its mounted instance owns identity. The
instance graph retains children across updates when their structural location
and key still match. Use a stable item ID for reorderable collections, not the
current row number. A replacement branch must not inherit another branch's
focus, timers or subscriptions. See [views and identity](../ui/views.md).

## Observation contract

Reads during body evaluation establish dependencies. A mutation invalidates
readers, not every sibling of the writer. An observable handle identifies its
model; it is not a snapshot of every field. For editable values use a binding
to the authoritative model instead of maintaining a second mutable copy.
See [state and bindings](../ui/state.md).

## Inherited environment

Install shared services at a scope that owns their lifetime. Descendants inherit
the nearest provider for the key; overriding a value in one subtree must not
affect its siblings. Reading a model through the environment still tracks
its observed fields. See [inherited environment](../ui/environment.md).

## Drawing and damage

Keep ordinary composition in layout. Use a canvas for content that needs custom
paint. Record in local coordinates and include the bounds of effects that paint
outside nominal geometry. On change the renderer must repair both the old and
new coverage; the window host accounts for the history of the buffer it acquires.
See [drawing](ui-drawing.md) and [repair](render-relocate.md).

## Input ownership

Dispatch platform events through the mounted graph. Hit testing uses published
geometry and clip order. Focus, pointer capture, and the current physical hit
are distinct: a drag can retain capture while the pointer moves over another
item. A resize or branch replacement must not send a release to a stale owner.
See [input and focus](../ui/input.md).
