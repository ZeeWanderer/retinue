# Motion and frame scheduling

Separate the model's destination from the value currently being presented.
Retarget from the current presentation value, not from the last destination;
otherwise reversing a rail or tab midway through its transition causes a jump.

Use scoped animation for state-driven transitions, a playback or keyframe track
for a staged sequence, and a timeline deadline for a countdown. A visible
one-second timer needs a wake at its next change, not a continuous animation.
Place the observation at the smallest view that displays the changing value.

The host samples due motion, applies changes to layout or paint, and requests
another frame only when work remains. Moving content and fading content have
different repair requirements. Moving a cached image can reuse its pixels;
a translucent layer still depends on the background it covers.

See [animation and clocks](../ui/animation.md) for authoring and the
[render pipeline](render-pipeline.md) for execution.
