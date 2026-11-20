# UI Framework

Build desktop and in-game interfaces from comparable Rust views. State observation
updates the affected tree; layout and drawing feed the same retained renderer.
The SDK supplies composition, text, images, drawing and input, not a styled widget
catalog. Applications own their visual language and game policy.

## Get Started

[Build a view](../ui/first-view.md) introduces `View::body`, stacks, modifiers and
observable input using a complete example. Run `make view-demo` to explore the
[standalone example](ui-composition-prototype.md) without a daemon or game.

## Author Interfaces

| Topic | Use It For |
| --- | --- |
| [Views and identity](../ui/views.md) | Composition, conditionals, keyed collections and retained state |
| [State and bindings](../ui/state.md) | Observable models, edits and use outside a view tree |
| [Inherited environment](../ui/environment.md) | Shared values and models with local overrides |
| [Layout](declarative-layout.md) | Stacks, anchoring, custom arrangement and scrolling |
| [Text, images and drawing](ui-drawing.md) | Intrinsic text, asynchronous images and custom paint |
| [Animation and clocks](../ui/animation.md) | State transitions, playback and scheduled updates |
| [Input and focus](../ui/input.md) | Hit testing, capture, keyboard navigation and popups |
| [Window and overlay hosts](../ui/hosts.md) | Platform events, rendering and presentation ownership |

Each topic links to its generated API. API pages link back to these topics and
retain compiler-produced signatures, trait implementations and source links.

## Understand The Implementation

- [SDK contracts](ui-contracts.md): detailed observation, ownership and invalidation behavior.
- [Render pipeline](render-pipeline.md): draw representation, optimization and execution.
- [GUI organization](gui-organization.md) and [overlay organization](companion-presentation.md): find actual application views and models.
- [Testing](testing.md) and [profiling](profiling.md): verify correctness separately from performance.

These are internal APIs. Improve their boundaries when real use warrants it;
there is no external compatibility promise or requirement to hide useful low-level
tools. Design research is separate from implemented authoring guidance.
