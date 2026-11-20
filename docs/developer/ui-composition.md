# Composing an interface

Use a [view](../ui/first-view.md) to describe content, a [layout](declarative-layout.md)
to arrange it, and [drawing primitives](ui-drawing.md) to paint. These layers
are shared by WFGUI and the two rendering companions.

## From description to instance

The authoring macros produce a structural template with input bindings. An
`InstanceGraph` mounts that template, retains keyed instances and publishes
changes to layout, input geometry and paint. The compiler does not turn an
entire page into a single native rendering function.

A conditional mounts its active branch. Scroll containers can mount just the
visible content; keyed identity belongs to the item rather than its position.
Changing a color need not remeasure its parent. Moving a composed element
need not invalidate an unchanged drawing recorded in its local coordinates.

## Custom content

Use a custom layout for geometry that cannot be expressed with the structural
containers. Use a canvas when drawing the content is the custom part.
An overlay with already positioned pixel layers can publish them directly;
it does not need to construct a second layout tree to present them.

- [Ownership and invalidation](ui-contracts.md)
- [Animation and clocks](../ui/animation.md)
- [Window and overlay hosts](../ui/hosts.md)
- [Render pipeline](render-pipeline.md)
