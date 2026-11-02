# UI Motion

Motion contract for the UI SDK. Transactions, scoped presentation interpolation,
custom animated drawing, bounded outgoing transitions, keyframes and controlled
playback are implemented in the View host. Springs and automatic program retargeting
remain planned work. All use one presentation runtime and scheduler.

## State And Presentation

Model state takes its final value immediately. The UI reconciles the new target
with retained presentation values and interpolates supported visual properties.
A boolean such as `is_folded` triggers the change; width, placement and opacity
are interpolated, not the boolean or the entire application model.

Track identity is the retained element plus a typed property/channel, not the
address of a newly allocated view description or binding. Each track stores its
target and current presentation. Retarget from the current presentation; preserve
velocity for spring motion. Do not restart an unchanged target every time body
is evaluated.

Stable identity allows continuity. Inserting/removing a node uses an explicit
transition, retaining only outgoing presentation until it finishes. Unrelated
inactive branches need not be instantiated. Discrete content such as text or an
image changes immediately unless an explicit transition, crossfade or custom
animation describes how to present that replacement.

## Low-Effort Authoring

Provide three complementary entry points:

- A view/container modifier chooses an animation when a comparable trigger value
  changes. Initial mounting does not implicitly animate from arbitrary defaults.
- A narrowly scoped modifier block applies animation only to the properties in
  that block. This lets size, hover colour and opacity have independent timing.
- `with_animation` supplies a transaction default for observable writes inside
  its synchronous closure, including writes from non-view code. Writable bindings
  can attach the same transaction metadata.

A trigger is a scheduling condition, not proof that every property changed in a
subtree was caused by that value. Container-wide animation can include other
changes in the same update. Use explicit narrow scopes when that is undesirable;
do not pretend arbitrary Rust expressions expose precise property-level causality.

Animation policies need distinct inherit, animate and snap behavior, plus a hard
disable for reduced-motion or explicit no-animation transactions. Nearest explicit
property policy overrides an inherited default; hard disable wins. Scope/modifier
order is intentional and tested rather than dependent on traversal order.

Curves should cover linear/ease-in/ease-out/ease-in-out, cubic Bezier and springs.
Do not require duration-based easing to masquerade as spring physics. Custom
animatable values extend the same interpolation contract; they do not install
their own timer or renderer.

## Transactions

Animation context must travel with mutation notifications. Consulting only a
thread-local variable during rendering loses the context after the originating
closure returns. The observation runtime needs pending change metadata, not just
a dirty bit.

Nested transactions inherit defaults with scoped overrides. Changes to the same
source before an update may collapse to the latest target and its transaction;
they are not an event log requiring every intermediate target to be displayed.
Preserve ordering/provenance for coalesced causes and choose a deterministic
default when different transactions invalidate the same consumer. The initial
rule is latest contributing mutation for that consumer, with explicit property
scopes taking precedence. Do not impose the last transaction on unrelated
consumers across the whole frame or attempt expensive whole-model snapshots.

An animation transaction is not database atomicity or rollback. Its scope is
synchronous and unwind-safe. Work after an `await` or on another thread requires
explicitly reapplying transaction intent when publishing the result. Completion
tracks the animations actually started, not an estimated sleep duration.

## Complex Motion

Implemented authoring:

```rust
let reveal = Keyframes::new(0.0_f32)
    .to(1.0, Animation::new(Duration::from_millis(160)))
    .hold(Duration::from_millis(300))
    .to(0.0, Animation::new(Duration::from_millis(120)));
let player = Playback::new(reveal);
let completion = player.play(now);
// In View::body or Primitive::resolve:
let progress = context.playback(&player);
```

Parallel tracks use `a.parallel(b)` and produce a tuple. Holds can provide delays
and stagger without per-frame work. `jump` changes a value at a boundary. Every
track samples the same monotonic playhead; intervals are explicit wall durations,
not fractions of a globally eased progress scalar. A custom `MotionProgram` can
provide different sampling and deadlines without owning a timer or drawing.

`pause(now)` freezes presentation, `seek(position, now)` changes the playhead,
`reverse(now)` changes direction without changing paused/playing state, and
`restart(now)` supersedes the old run. Seek does not emit historical business
events. `Run::result()` is observed normally; scheduling callbacks must defer
application work, as with all observation. No completion callback runs from paint.
`PlaybackOwner` cancels when its last owner drops; controls from `playback()` do
not extend ownership. Store the owner in view properties to tie it to unmount.
Plain `Playback` instead belongs to its explicit host owner. Rebuilding a program
description does not replace or restart an existing player.

The remaining richer motion design follows; not every item below is implemented.

Offer progressively more control without a second animation engine:

- Phases: named target states, per-phase curves, holds, sequence/parallel
  composition and optional stagger. Useful for staged expansion and reveals.
- Keyframes: typed multi-property tracks with explicit timings, easing/springs,
  holds and discrete changes. Sample all tracks against the same frame time.
- Playback control: a retained handle can play, pause, seek, reverse, retarget or
  cancel a program. A gesture can drive progress directly and then settle with a
  curve or spring. No caller-managed per-frame loop is necessary.

Programs are descriptions; handles own playback state and belong to a retained
view or an explicit host owner. Rebuilding the description must not restart the
program. Triggers explicitly start/restart it. Repetition is opt-in, never the
default for a transient response.

Only one driver owns a property channel at a time. Default replacement samples
the current presentation and supersedes the old driver. Queueing or additive
composition must be explicit and type-appropriate; rapid hover changes must not
build an unbounded queue of animations.

Completion reports finished, cancelled or superseded; unmount releases owned
tracks and settles outstanding handles. Callbacks run through the host after
updating, not reentrantly inside layout/paint. Seeking does not replay unrelated
business actions. A higher-level action may await completion, but the renderer
must never block on it.

## Layout, Canvas And Rendering

Known animated properties update retained layout/presentation values without
reevaluating an entire view body every frame. A custom progress-dependent body is
an explicit escape hatch with a correspondingly scoped invalidation cost.

Distinguish layout animation from presentation-only transforms. Changing the
width of wrapped text or a constraint affecting siblings may require local layout
on each sample. A pure translation can reuse layout and drawing. Do not blindly
interpolate every rectangle when that violates layout relationships. Custom
Layouts participate through their declared measurements/placements and values.

Canvas content can declare animatable parameters and emit typed drawing commands
from their presentation values. It participates in the same scheduler, damage and
optimization machinery. Do not force either whole-Canvas rasterization or a
native opaque callback to animate custom drawing.

### Direct Progress Drawing

The lowest-level authored case is a progress scalar driven from 0 to 1 over a
duration. A drawing uses that scalar to construct a reveal, sweep, path or another
custom visual. It must not need an observable model write or a caller-owned timer
for every frame.

Distinguish normalized elapsed time `u` from eased presentation `t`. For an
ordinary duration curve, `u = clamp(elapsed / duration, 0, 1)` and `t = curve(u)`.
Zero-duration motion resolves immediately. Many easing curves keep `t` in [0, 1],
but springs and elastic curves may overshoot; do not silently clamp all animation
output. A drawing that requires a bounded parameter can request clamped progress
or apply a domain-specific clamp. Retargeting starts from the current presented
value rather than resetting the visual to zero. Replay is a distinct command.

Progress is a small retained presentation track. Its owner, clock, cancellation,
pause/seek and reduced-motion behavior are the same as other animation tracks.
Drawing receives an immutable sample; it neither advances the track nor mutates
domain state. A paint-only reader invalidates drawing, not the whole layout.
If measurement also depends on the parameter, that dependency explicitly incurs
layout work. Changes to drawing bounds must update previous/new paint extents.

### Progress Versus Keyframes

A scalar progress value mapped through piecewise stages is a valid animation
representation, not inherently a performance problem. Its cost depends mainly on
the work each sample triggers: layout, text/path construction, command recording
and repainting. A single progress-dependent Canvas may rerecord a large group;
smaller stable fragments or retained parameter channels can narrow that work.
Typed commands emitted by custom drawing remain available to the optimizer.

Prefer explicit tracks when properties have independent timing or need robust
interruption, velocity continuity and playback control. Tracks also expose holds,
next change times and transform-only segments without inspecting arbitrary
functions of progress. They are not automatically faster: SwiftUI's keyframe
content closure also runs every animation frame. Our tracks and direct progress
must share the same sampling, presentation and rendering machinery.

For exact stage durations, select stages using linear elapsed time, then apply
each stage's easing locally. Selecting stages using globally eased progress
changes their wall-clock timing. Position or velocity discontinuities at stage
boundaries can look choppy even without dropped frames.

Compare equivalent visuals with deterministic samples and identical cache/thread
settings. Measure sampling, layout, recording, repair and rendering separately;
do not attribute a whole-Canvas redraw cost to scalar interpolation itself.

### Trait Responsibilities

Keep these responsibilities distinct; names/signatures need the authoring prototype:

- `View`: declare structure from model/environment input. No mandatory drawing
  or animation methods on every view.
- `Layout`: measure/place declared children, with cached layout computation.
- `CanvasContent`: emit local-coordinate typed drawing from assigned geometry
  and read-only presentation input. Static drawings need no animation boilerplate.
- `Animatable`: optionally project a small typed set of visual parameters from a
  target description. It may expose only a scalar or a multi-field value.
- Interpolation/value contract: define how those parameters interpolate. A
  vector/tangent capability can additionally support spring velocity and blending;
  do not require nonsensical vector arithmetic on every drawable object.
- Timing/driver contract: sample easing, spring or custom motion with retained
  playback state and completion/velocity information. This does not draw.

Immutable configuration and resources stay separate from presentation data. Do
not clone a complete model, image or command list each frame just to substitute
one scalar. A generated Animatable projection may remove field plumbing, but a
manual projection must remain available for normalization or derived values.
Applying a presentation sample must never call Observable model setters.

SwiftUI's `Animatable` exposes old/new `animatableData`; the framework interpolates
that data and applies each presentation sample. Its `VectorArithmetic` supports
addition/scaling/magnitude. This is a useful capability split, not a requirement
to copy its exact mutable-setter interface. Our adapter can pass sampled data
alongside immutable drawing configuration instead.

Hit testing follows presented geometry. Exiting views can retain visuals without
retaining interaction. Previous and current paint extents feed region damage;
the renderer chooses repair and raster reuse. Animation may provide motion and
stability hints, but does not force a raster cache policy.

Use one frame sample time per host, monotonic time for motion, deadline scheduling
and no idle ticking. Paused or unmounted work has no frame deadline. Hidden-page
policy distinguishes finite motion that can settle offscreen from decorative
loops that should pause. Reduced motion supplies an explicit snap or subdued
alternative. Clock/timer content remains distinct from continuous animation.

## Existing Mechanisms

`transition::Tween` retargets from the current value. `runtime::ViewTree` owns
presentation tracks, indexed deadlines and stable view identities
and observes mutation transactions through `state::Observation`. Deferred writes
retain their animation/disable intent, including inherited provider updates;
the latest contributing mutation wins when several writes affect one body.
Unrelated consumers keep their own cause. Scopes restore on unwind; body writes
defer until the next host turn.

Scoped overrides animate retained geometry and rectangle properties without
rebuilding parent bodies. `AnimatedCanvas` resolves custom parameters independently.
`Switch` retains at most one outgoing page and cancels its input. Keyframes and
playback use the same frame schedule, including holds and owner cancellation.
Springs and velocity-preserving program retargeting remain to connect.

## Implementation Checks

1. Establish stable property channels, target/presentation separation and mutation
   transaction propagation alongside View reconciliation.
2. Add scoped/value-triggered animation, transaction defaults/disable, retargeting
   and insertion/removal transitions. Prove normal authoring before sequences.
3. Add phases, parallel keyframes and playback control on the same tracks.
4. Exercise full-window rail collapse, filter-row insertion, mod-card hover/handoff
   and a staged overlay reveal, including reduced motion and scaling.

Use a deterministic clock to test different curves for independent properties,
simultaneous causes, nested scopes, rapid reversals, spring velocity, cancellation,
unmount, completion and zero idle deadlines. Compare retained animation frames
against fresh scenes sampled at the same presentation state; two paths sharing
the same corrupt scene are not an independent correctness check. Benchmark layout,
recording, repair and rendering separately once the runtime is integrated.

## References

- [SwiftUI animations](https://developer.apple.com/documentation/swiftui/animations)
  covers state-driven animation and transitions.
- [Transactions](https://developer.apple.com/documentation/swiftui/transaction)
  carry animation context; [scoped animation](https://developer.apple.com/documentation/swiftui/view/animation(_:body:))
  limits which modifiers receive it.
- [Phases and keyframes](https://developer.apple.com/documentation/swiftui/controlling-the-timing-and-movements-of-your-animations)
  provide declarative staging and coordinated tracks.
- [UIViewPropertyAnimator](https://developer.apple.com/documentation/uikit/uiviewpropertyanimator)
  is a reference for direct pause, seek, reverse and retiming controls.
- [Animatable](https://developer.apple.com/documentation/swiftui/animatable),
  [VectorArithmetic](https://developer.apple.com/documentation/swiftui/vectorarithmetic)
  and [CustomAnimation](https://developer.apple.com/documentation/swiftui/customanimation)
  separate the animated data, value operations and timing implementation.
