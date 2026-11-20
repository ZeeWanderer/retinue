use super::{Glyph, GlyphRun, Hinting};
use crate::{Bounds, Color, Command, TextAlignment, painter::CachedFont};
use std::sync::Arc;

// Replay fixtures need deterministic native glyph geometry, not a second shaper.
pub(crate) fn label(
    font: &CachedFont,
    text: &str,
    size: f32,
    bounds: Bounds,
    alignment: TextAlignment,
    color: Color,
) -> Command {
    let run = run(
        font,
        text,
        size,
        (alignment == TextAlignment::Center).then_some(bounds.height),
    );
    Command::Clip {
        bounds,
        commands: Arc::new(vec![Command::GlyphRun {
            origin: (bounds.x, bounds.y),
            run,
            color,
        }]),
    }
}

pub(crate) fn run(font: &CachedFont, text: &str, size: f32, centered: Option<f32>) -> GlyphRun {
    let native = font.native();
    let scale = size / f32::from(native.metrics.units_per_em());
    let line = native.metrics.metrics().hhea_line.unwrap();
    let ascent = line.ascender.to_f32() * scale;
    let descent = line.descender.to_f32() * scale;
    let height = ascent - descent + line.line_gap.to_f32() * scale;
    let first = ascent
        + centered.map_or(0.0, |h| {
            (h - ascent + descent - text.bytes().filter(|c| *c == b'\n').count() as f32 * height)
                * 0.5
        });
    let map = native.metrics.charmap();
    let metrics = native.metrics.glyph_metrics();
    let mut glyphs = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let mut x = 0.0f32;
        for c in line.chars() {
            let id = map.map_unicode(c).unwrap_or_default();
            glyphs.push(Glyph {
                id: id.to_u32(),
                x: x.into(),
                y: (first + index as f32 * height).into(),
            });
            x += metrics.h_advance(id) * scale;
        }
    }
    GlyphRun::new(native, size, glyphs, Hinting::None).unwrap()
}
