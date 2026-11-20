# Input And Focus

Attach interactions to retained content. The shared router uses the resolved
view geometry, clipping and painter order; do not construct a second page-wide
hit-test tree from manually rounded rectangles.

## Route A Gesture

`InputOptions` describes input behavior; `EventHandler` handles events with an
`EventContext`. Its local position is in the receiving target's coordinates.
Handling an event and stopping propagation are separate actions.

Capture a pointer for a drag, then release it on completion or cancellation.
Capture belongs to a stable target identity, not whichever item happens to be
under the next event. Hiding/removing the target or changing pages must cancel
ownership. Opening a blocking modal cannot silently strand a background press.

## Keyboard And Accessibility

Keyboard navigation establishes visible focus. A mouse press must not leave a
keyboard-focus outline on every clicked control. Use shared focus policy and
semantic roles so the host can supply traversal and accessibility consistently.
Rendered, hit-test and accessibility bounds must describe the same presentation.

Text editing uses the shared line editor and host clipboard/IME integration.
A custom text field is more than drawing a string: it needs caret, selection,
keyboard commands and focus ownership.

## Popups And Overlay Policy

Put popup content above the owning page, outside a scrolled content subtree.
Handle outside presses, Escape and dismissal through one owner. Native game focus,
global shortcuts and click-through policy remain in the companion host, not in
generic view events. See [hosts](hosts.md).

Test captured drags beyond bounds, removal while pressed, page switches, modal
opening, scale changes and release after focus loss. Geometry tests alone do not
prove a live compositor or game interaction works.
