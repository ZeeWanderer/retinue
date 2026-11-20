# Window And Overlay Hosts

The view/runtime/layout layers are host-agnostic. Native adapters provide the
window or Wayland surface, events, clocks and presentation. Applications retain
visibility, focus, game lifecycle and workload policy.

| Layer | Owns |
| --- | --- |
| `InstanceGraph` | Mounted views, observation, layout and actionable geometry |
| `ViewSurface` | Publish already positioned paint layers and their damage without another layout |
| `retinue-render` | Immutable drawing, cache decisions, repair and Blend2D execution |
| `presentation_buffers::Buffers` | Wayland target/source leases, submission and buffer age |
| Desktop or overlay adapter | Platform events, frame callbacks, flush and lifecycle |

## Select A Host

The `desktop` feature provides the winit window host. The `overlay` feature
provides the layer-shell application host used by PoE2. Warframe has a custom
loop for its compact/full-output surfaces and interaction policy; it still uses
the shared buffer and renderer mechanisms.

Window title/content/toolbars are ordinary layouts. Roles and window actions carry
native meaning independently of their layout. A custom host need not replace the
view model, input router or renderer.

## Publish A Frame

Update observed inputs and sample due timelines, acquire a valid target, prepare
drawing and choose repairs from its buffer history. Native completion precedes
presentation. Publish a successful submission only after native presentation
succeeds; failed submission invalidates the relevant history.

Current-frame compositor damage is not the same as the acquired buffer's repair
region. Keep them separate. Relocation may reuse an immutable source lease; it
does not authorize copying from an in-flight target or ignoring changed content.
See [relocation and repair](../developer/render-relocate.md).

Apps own their scheduling budgets. Desktop focus and in-game overlays have
different resource constraints; views should not encode those policies.

For worker layout, create `LayoutExecution` from the existing `RenderHost` and
the adapter's completion waker. Call `InstanceGraph::advance_at`; on [`Poll::Pending`](https://doc.rust-lang.org/std/task/enum.Poll.html#variant.Pending), retain
the previous controls and wait for the wake before polling again. Both native
adapters support this handoff through `desktop::Application::poll_prepare` and
`surface::Application::poll_prepare`. Do not synchronously
update a tree whose layout is pending, or bind new controls to unfinished geometry.
The synchronous API remains useful for small layouts and offscreen consumers.
Resume a pending frame with its original animation sample. Advancing the sample
before publication can invalidate every completed layout and starve the animation.
Input and data changes may still supersede pending geometry.

Start from the [standalone host example](../../crates/retinue-ui/examples/views/main.rs),
[GUI source map](../developer/gui-organization.md), or
[overlay source map](../developer/companion-presentation.md).
