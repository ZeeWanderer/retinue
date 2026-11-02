# UI Drawing

Proposed contract for the new UI SDK. The current Canvas/command implementation
exposes only part of this surface; this document is not an implemented API list.
See [UI SDK](ui-sdk.md#drawing-and-damage) for damage ownership and
[UI motion](ui-motion.md) for presentation sampling.

## Composition And Drawing

A card may be one Canvas, a hierarchy of Views, or a mixture. A material slot may
draw its image/glow/count together or compose Image, Shape and Text primitives.
Choose boundaries for reuse, independent state, layout, interaction and semantics,
not because each View needs a raster surface. Ordinary composition lowers to the
same draw IR without an offscreen allocation for each View or stack.

Composite Views keep a pure body entry point. CanvasContent supplies custom drawing
through a Canvas primitive, not a second mandatory paint method on every View.
Primitive descriptors need no fabricated body. Custom Layouts measure/place
declared children; arbitrary child Views must not be instantiated during painting.
A hybrid uses declared child slots, background/overlay drawing and shared anchors.

Canvas can expose stable drawing fragments without manufacturing child Views.
These preserve useful invalidation/reuse boundaries. They are not mandatory raster
caches. Conversely, interactive subregions and accessibility semantics must be
declared explicitly when direct drawing replaces semantic child Views.

## Drawing Context

Supply local logical bounds, inherited drawing defaults, read-only sampled
presentation values and access to shared resources. Normal drawing needs no host
coordinate conversion, BGRA packing, scale multiplication, manual PaintKey, damage
rectangle, cache lookup or Blend2D context creation.

Use typed geometry, brushes, stroke styles, paths, text layouts and image handles.
Provide scoped transforms/clips/styles with automatic restoration, plus explicit
per-operation overrides. Explicit values override inherited defaults. Drawing
order remains painter order unless a downstream pass proves reordering valid.

Illustrative operations, not settled Rust syntax:

```text
ctx.fill(card.rounded(8), background)
ctx.draw_image(artwork, art_rect, contain)
title = ctx.layout_text(name, title_style, title_width)
ctx.draw_text(title, title_origin)
```

Measurement resolves text/resources without emitting drawing. Recording appends
typed commands; it does not execute Blend2D calls, synchronously fetch/decode
assets, advance animations or modify model state. Immutable paths, brushes,
shaped text and image snapshots can be shared between recordings and remain alive
until all queued rendering that consumes them finishes.

## Blend2D Coverage

Do not constrain the new API to today's card-specific helpers. The normal recorded
surface should represent general 2D geometry and paths; fills/strokes with caps,
joins and dashes; solid/gradient/pattern brushes; affine transforms; image source
rectangles and sampling; text/glyph runs; and supported compositing operations.
Shapes and declarative Text/Image must lower to these same operations.

This is visual capability coverage, not a one-to-one copy of the entire BLContext
API. Context lifetime, flushes, thread counts, render targets and submission remain
renderer-owned. A typed operation may retain a native immutable Blend2D resource;
it need not duplicate that resource's internal representation in Rust.

Do not present every desired effect as a native Blend2D operation. General masks,
nonrectangular clipping, isolated group opacity and filters need explicit lowering
and capability checks; some require temporary surfaces or additional passes.
Applying opacity to each overlapping child is not equivalent to group opacity.
Unsupported operations must not silently disappear or acquire hidden cheapness
claims. Common missing capabilities should extend typed IR and analysis together.

A bounded native escape hatch remains for exceptional operations. Its bounds,
resource dependencies and replay behavior are explicit; optimization cannot inspect
its implementation. It must not freely flush/end the host context or mutate state
that leaks into later commands. External GPU surfaces remain separately composited
surfaces, not a reason to read GPU output back into CPU drawing.

## Text And Images

Text is a layout primitive with intrinsic measurement, wrapping, baselines,
truncation and inherited typography. Layout can measure text without drawing it.
Canvas uses the same text-layout service and can retain the resulting layout for
drawing, avoiding separately implemented metrics and line-breaking behavior.
Text layout/shaping is a distinct service; raw Blend2D text calls do not replace
all paragraph, fallback or script-layout responsibilities.

Image is also a layout primitive, backed by an image resource rather than only an
already decoded raster. It supports intrinsic metadata, explicit aspect/size,
fit/fill, cropping, alignment and application-defined placeholder/error content.
Canvas draws the same resource without taking over its loading lifecycle.

An image handle has stable asset identity. Its observable snapshot separates
metadata, the current drawable rendition, loading/error status, quality and
freshness. These are not one loading-or-ready boolean: cached pixels may remain
usable during refresh, a preview may precede the final rendition, and a refresh
failure need not remove an already displayed image.

Provider rules:

- Acquisition is shared and deduplicated, outside body/layout/paint. Game asset
  fetching remains daemon-owned; the app's provider bridges daemon updates and
  existing client preparation/cache workers. Bundled/local images use the same
  presentation contract without requiring a remote fetch.
- Subscribe retained consumers to handles; acquire no new independent job per
  body evaluation. Visibility controls priority and residency. Unmount removes
  that consumer, not work still needed by another consumer.
- Metadata can establish intrinsic size before pixels are ready. Use supplied
  size/aspect or an explicit placeholder-sizing policy until then. Pixel-only
  refinement invalidates paint; intrinsic metadata changes invalidate measurement
  only for consumers whose layout depends on it.
- Publish immutable renderable snapshots, or an equivalent synchronized ownership
  transfer. Never let decoding mutate pixels still read by queued Blend2D work.
  Retire superseded generations after consumers/in-flight work release them.
- Track request/content revision separately from rendition quality. Late work for
  an obsolete request must not replace newer content; lower-quality results for
  the same revision must not downgrade a better rendition. Compare resources by
  identity plus generation, not a mutable handle's address alone.
- Coalesce intermediate notifications to frame demand. Do not repaint on every
  downloaded chunk. Partial source-image damage may be supplied and mapped through
  crop/scale/filter footprint; otherwise invalidate the image's displayed bounds.
- Keep usable pixels while refreshing. Placeholder/error content and optional
  visual transitions belong to the View, not to remote-fetch policy.

The requirement is stable layout without pixels and efficient streamed resource
updates, independent of the image's encoding. No particular codec implementation
is a prerequisite for the Image view. This admits progressive decoding without
requiring it from every provider. PNG
interlacing can support progressive display, but an interlaced file and a decoder
that produces only a complete image do not by themselves provide staged updates.
The resource contract accepts preview/refinement/final snapshots; actual decoder
support must be implemented and verified separately. Animated image frames also
need a timeline/lifecycle rather than treating download chunks as animation frames.

## Current Baseline And Checks

`render/paint.rs::Command` currently has rounded fills/outlines, rings, lines,
polygons, text, inset shadows, ready images, clips, translations, fragments and
external commands. It lacks general paths, gradient/pattern brushes, general
affine transforms and a general compositing API. Canvas exposes helpers over that
subset. Do not describe it as a complete Blend2D wrapper.

`ImageSource` currently holds ready raster resources. wfgui's preparation worker
publishes cached images first and subsequent complete thumbnails incrementally;
this is not progressive decoding of an individual image. Existing text code uses
fontdue metrics/kerning and Unicode line breaking, not a full paragraph shaper.

Prototype both a composite card and a direct-drawn equivalent. Check equivalent
geometry/pixels, resource sharing, mixed child layout and drawing, nested state
restoration, scaling and semantic hit regions. Exercise missing/stale/failed images,
metadata arrival, progressive generations, rapid source replacement, shared use,
unmount and in-flight rendering. Bench recording/layout/repair/raster work separately;
neither smaller view trees nor more draw fragments are universally faster.

## References

- [Blend2D context](https://blend2d.com/doc/classBLContext.html), checked against
  the vendored `core/context.h` and Retinue bridge.
- [SwiftUI resolved images](https://developer.apple.com/documentation/swiftui/graphicscontext/resolvedimage)
  illustrate sharing image primitives with context drawing and environment resolution.
- [PNG interlacing](https://www.w3.org/TR/png/#12Interlacing) supports progressive
  display; delivery and decoder behavior still determine whether a provider emits it.
