# Render Pipeline

WFGUI and both in-game overlays use the same renderer. Applications describe
content and interactions in Rust. The UI SDK lays them out and records paint;
the renderer prepares resources, repairs the target, and executes Blend2D
commands. Only the host presents the finished buffer.

## From A View To Pixels

1. An observed change invalidates the affected view. The retained instance graph
   preserves keyed identity and applies placement, paint, and interaction updates.
2. A paint program records commands in local coordinates. Its instances bind
   placement and clips. Translating an instance need not rebuild its commands.
3. Preparation resolves images, fonts and optional raster materializations.
   Source commands remain available if an optional cache cannot be prepared.
4. The acquired target's history determines what must be repaired. Intersecting
   commands replay in painter order; current-frame damage is submitted to the
   compositor separately. Another buffer's valid pixels cannot be assumed.

## Optimization Context

An element can identify a viewport, a repeated item or a reuse opportunity
without promising that caching will be faster. The renderer still checks
coverage, resource identity, clipping and paint order. A hint changes which
plans are considered; it must not change the image.

## Raster Planning

Moving already-valid pixels and caching an expensive paint are separate
decisions. A scroll may relocate most of a viewport and repaint an exposed
strip using cached fragments. A small hover change inside the moved region
still needs repair. Transparent overlays and intervening paint can rule out
relocation. See [relocation and repair](render-relocate.md),
[residency](render-residency.md), and [invalidation](ui-invalidation.md).

## Execution

Layout and preparation can run off the platform thread. One submitted native
batch uses Blend2D's own participants, including the caller. The platform
owner continues handling input and publishes completed work. Do not add worker
time, owner occupancy and completion waits as if they were serial frame phases.

Use the [benchmark workflows](../../tools/benchmarks/README.md) to compare
representative frames, and [profiling](profiling.md) to attribute a measured cost.
The [performance history](https://zeewanderer.github.io/retinue/research/benchmarks/)
keeps workload and machine configurations separate.
