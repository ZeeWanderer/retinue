use super::{Font, Hinting, TextError};
use crate::Bounds;
use read_fonts::types::GlyphId;
use std::{
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};

/// One glyph's baseline origin; offsets and advances have already been applied.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug)]
pub struct GlyphRun(Arc<Run>);

#[derive(Clone, Debug)]
struct Run {
    font: Font,
    size: f32,
    glyphs: Arc<[Glyph]>,
    ink: Bounds,
    fingerprint: u64,
    source: Option<(Arc<str>, Range<usize>)>,
}

impl GlyphRun {
    pub fn new(
        font: Font,
        size: f32,
        glyphs: impl Into<Arc<[Glyph]>>,
        hinting: Hinting,
    ) -> Result<Self, TextError> {
        if !size.is_finite() || size <= 0.0 {
            return Err(TextError::InvalidSize);
        }
        if hinting != Hinting::None {
            return Err(TextError::UnsupportedHinting);
        }
        let glyphs = glyphs.into();
        let metrics = font.metrics.glyph_metrics().scaled_f32(size);
        let mut ink: Option<Bounds> = None;
        for glyph in glyphs.iter() {
            if glyph.id >= font.metrics.num_glyphs()
                || !glyph.x.is_finite()
                || !glyph.y.is_finite()
                || glyph.x.abs() > f32::MAX as f64
                || glyph.y.abs() > f32::MAX as f64
            {
                return Err(TextError::InvalidGlyph);
            }
            let Some(bounds) = metrics.extents(GlyphId::new(glyph.id)) else {
                continue;
            };
            if bounds.width <= 0.0 || bounds.height <= 0.0 {
                continue;
            }
            let b = Bounds::new(
                glyph.x as f32 + bounds.x_bearing,
                glyph.y as f32 - bounds.y_bearing,
                bounds.width,
                bounds.height,
            );
            if ![b.x, b.y, b.x + b.width, b.y + b.height]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err(TextError::InvalidGlyph);
            }
            ink = Some(match ink {
                None => b,
                Some(a) => a.union(b),
            });
        }
        let mut hash = std::hash::DefaultHasher::new();
        font.identity().hash(&mut hash);
        size.to_bits().hash(&mut hash);
        for glyph in glyphs.iter() {
            glyph.id.hash(&mut hash);
            for value in [glyph.x, glyph.y] {
                (if value == 0.0 { 0 } else { value.to_bits() }).hash(&mut hash);
            }
        }
        Ok(Self(Arc::new(Run {
            font,
            size,
            glyphs,
            ink: ink.unwrap_or(Bounds::new(0.0, 0.0, 0.0, 0.0)),
            fingerprint: hash.finish(),
            source: None,
        })))
    }

    /// Retains shaping provenance for inspection, not for painting or cost estimates.
    pub fn with_source(mut self, text: Arc<str>, range: Range<usize>) -> Result<Self, TextError> {
        if text.get(range.clone()).is_none() {
            return Err(TextError::UnsupportedLayout("invalid source text range"));
        }
        Arc::make_mut(&mut self.0).source = Some((text, range));
        Ok(self)
    }

    pub fn source(&self) -> Option<&str> {
        self.0
            .source
            .as_ref()
            .map(|(text, range)| &text[range.clone()])
    }

    pub fn font(&self) -> &Font {
        &self.0.font
    }
    pub fn size(&self) -> f32 {
        self.0.size
    }
    pub fn glyphs(&self) -> &[Glyph] {
        &self.0.glyphs
    }
    pub fn ink_bounds(&self) -> Bounds {
        self.0.ink
    }
    pub fn hinting(&self) -> Hinting {
        Hinting::None
    }

    pub(crate) fn fingerprint(&self) -> u64 {
        self.0.fingerprint
    }
}

impl PartialEq for GlyphRun {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.fingerprint() == other.fingerprint()
                && self.font() == other.font()
                && self.size() == other.size()
                && self.glyphs() == other.glyphs()
                && self.source() == other.source())
    }
}
