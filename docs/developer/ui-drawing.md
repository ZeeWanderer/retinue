# Text, Images And Drawing

[api:Text]: fn@retinue_ui::authoring::Text
[api:Image]: fn@retinue_ui::authoring::Image

Choose a view boundary for independent layout, observation, input or reuse. A card
can be one Canvas, several views or a mixture; a view does not imply a separate
raster buffer. All these descriptions feed the same retained renderer.

## Text And Images

Use `Text` when text participates in layout. It measures and wraps under its
proposal, supports line limits and baseline alignment, and inherits `Typography`
unless its font, size or color is overridden. Custom drawing can use the same
`drawing::text::Paragraph` service instead of inventing a second text-measurement path.
It retains Parley shaping and records Blend2D glyph runs from the same layout.
Keep the paragraph for repeated width proposals; changing color does not reshape it.
`layout(width, alignment)` returns a checked, borrowed placement. Release that borrow
before requesting another placement; recorded commands retain their own glyph data.
Static TrueType/CFF outlines are supported without hinting. Unsupported fonts or
invalid sizes return an error; `Text` propagates it through layout without replacing
the last successfully published frame.

Text and editable fields use the requested face first. On Linux, a missing
grapheme can use an installed outline face chosen through Fontconfig. Parley
uses HarfRust for shaping; Blend2D paints the same faces and positions used for caret
and selection geometry. If no compatible face covers a character, it remains
a missing glyph. See the [color font limitation](blend2d-issues.md#color-glyph-runs-capability-gate).

Interactive hosts prepare the Linux font catalog as background work when the
render executor has spare capacity. Startup does not preload faces or wait for
the catalog. A missing glyph can still incur a first lookup.

In desktop text fields, `LineEdit::preedit` changes what is drawn, not the
committed value returned by `text()`. Commit through the same validated insertion
as keyboard input. Cancellation restores the original selection. Password
fields do not offer composition or candidate geometry to the input method.

`Image` observes an `ImageResource`, not a URL or a synchronous decoder. It supports
intrinsic sizing, `resizable`, fit/fill and a placeholder size while metadata is
unavailable. Use background/overlay views for loading or failure decoration.

The resource owner starts requests and publishes immutable renditions on the UI
thread. `begin` returns an identity used by `set_metadata`, `publish`, `complete`
and `fail`; obsolete responses are rejected. Freshness, loading status and usable
pixels are separate. Refresh failure need not discard the image already shown.
Views do not fetch data in their bodies or decode files while painting.

## Record Custom Drawing

Implement [CanvasContent](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/drawing/trait.CanvasContent/)
with comparable drawing inputs:

```rust
use retinue_ui::{prelude::*, Bounds};

#[derive(PartialEq)]
struct Meter { fraction: f32 }

impl CanvasContent for Meter {
    fn draw(&self, context: &mut DrawingContext) {
        let bounds = context.bounds;
        context.fill(bounds, 4.0, Color::rgb(0x252b30));
        context.fill(
            Bounds::new(bounds.x, bounds.y,
                bounds.width * self.fraction.clamp(0.0, 1.0), bounds.height),
            4.0,
            Color::rgb(0x2ca379),
        );
    }
}

let meter = Canvas(Meter { fraction: 0.6 }).frame(Frame::new().width(180.0).height(12.0));
```

The context uses local logical coordinates. It records commands rather than
executing Blend2D calls. Do not multiply by display scale, convert to window
coordinates, manage a rendering thread or create a native context here.

Compare every input that changes the drawing. Recording must be deterministic
for those inputs; execution can be replayed for several repairs or skipped.
Do not mutate models, advance time or acquire external data in `draw`.

## Available Operations

[DrawingContext](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/drawing/struct.DrawingContext/)
supplies fills, outlines, lines, rings, images, scoped rectangular
clips and translations. `emit` also accepts typed renderer commands such as
text, polygons, inset shadows and retained fragments. See the linked API for exact
signatures. `PixelCanvas` and `PixelLayers` are lower-level tools for existing
pixel-oriented layouts, not another renderer.

The recorded API does **not** yet expose every Blend2D path, brush, transform or
compositing operation. Bounded external drawing is the escape hatch; it must
declare its affected region and dependencies. It must not flush or terminate the
host context. Broader capability work belongs in the typed representation and
analysis together; see the separate [drawing design notes](ui-drawing-design.md).

## Let The Renderer Repair

Typed commands provide paint bounds. Frame comparison derives damage, and buffer
history determines the repair regions. Local Canvas coordinates and its layout
rectangle are not a substitute for the true footprint of strokes or effects.
Movement and removal must repair old output as well as new output.

Record the complete drawing, not only the current dirty rectangle. The renderer
needs retained content to uncover it later. Native escape hatches must obey their
bounds; ordinary views should not hand-maintain dirty rectangles or force caches.
See [the render pipeline](render-pipeline.md), [repair](render-relocate.md) and
[animation](../ui/animation.md) for the downstream behavior.
