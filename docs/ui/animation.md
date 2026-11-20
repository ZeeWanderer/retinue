# Animation And Clocks

Model state changes immediately; presentation values interpolate toward it.
Choose the smallest tool that describes the motion:

| Tool | Use It For |
| --- | --- |
| `ViewContext::animated_with` | Retarget a named value with a specific curve |
| `with_animation` / `with_transaction` | Animation intent on mutations outside views |
| `ViewContext::animated` | A custom interpolated presentation value |
| `Canvas` with a sampled value | Custom drawing that follows an interpolated input |
| `Playback` / `MotionProgram` / `Keyframes` | Explicit multi-stage or controlled motion |
| `ViewContext::time(schedule)` | A ticking timer or deadline-driven update |

## Keep Geometry And State Distinct

Interruption starts from the current presentation, not the old target. A
translation should change placement rather than rerecord its drawing. Input,
clipping and damage must follow the presented geometry. The
[tab example](../../crates/retinue-ui/examples/views/model/tab.rs) animates
width and color from the same selection change.

A direct progress value from zero to one is a valid custom-drawing tool, not a
performance bug by itself. Resolve immutable draw commands for that sample and
let the renderer track changed paint. Use phases/keyframes when that makes staging
clearer than a large progress-dependent drawing function.

## Schedule Work Only While Needed

Read time through the view context or the host timeline. The owner registers its
next deadline; unmounting cancels it. Paused/finished motion must stop waking the
host. Use wall-clock schedules only for real clock boundaries; animations use
monotonic time.

Honor `ReduceMotion` and disabled-animation transactions. Do not run a permanent
polling loop to animate a view. The host coalesces wakeups and supplies redraw
deadlines; views do not choose a rendering thread count.

See [motion and frame scheduling](../developer/ui-motion.md) for host responsibilities.
