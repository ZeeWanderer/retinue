//! Parley layout and native glyph runs share font and placement geometry.

use parley::{Alignment, AlignmentOptions, Layout, PositionedLayoutItem};
use retinue_render::{
    Color, Command,
    painter::{Painter, PainterError},
    text::{Font, Glyph, GlyphRun, Hinting, PreparedFont, TextError},
};
use std::{collections::HashMap, sync::Arc};

/// Keeps cluster, caret and line geometry with the exact glyphs used for painting.
pub struct ShapedText {
    source: Arc<str>,
    layout: Layout<()>,
    runs: Arc<[GlyphRun]>,
    fonts: HashMap<(u64, u32), Font>,
    placement: Option<Placement>,
}

#[derive(Clone, Copy, PartialEq)]
struct Placement {
    width: Option<f32>,
    alignment: Alignment,
    overflow: bool,
    last_line: Option<Alignment>,
}

impl ShapedText {
    pub fn from_layout(source: &str, layout: Layout<()>) -> Result<Self, TextError> {
        Self::from_resolved(source, layout, HashMap::new())
    }

    pub(super) fn from_resolved(
        source: &str,
        layout: Layout<()>,
        mut fonts: HashMap<(u64, u32), Font>,
    ) -> Result<Self, TextError> {
        if source.len() != layout.text_len() {
            return Err(TextError::UnsupportedLayout(
                "source length differs from shaped text",
            ));
        }
        let source: Arc<str> = source.into();
        let runs = Self::resolve(&layout, &mut fonts, Some(&source))?;
        if !source.trim().is_empty() && runs.is_empty() {
            return Err(TextError::UnsupportedLayout(
                "nonempty text has no positioned font runs",
            ));
        }
        Ok(Self {
            source,
            layout,
            runs: runs.into(),
            fonts,
            placement: None,
        })
    }

    /// Rebreaks and positions existing shaping without consulting a font system.
    /// Ownership is consumed so a failed placement cannot expose stale glyphs
    /// alongside changed caret geometry. Recorded commands retain their old runs.
    pub fn reflow(
        mut self,
        width: Option<f32>,
        alignment: Alignment,
        options: AlignmentOptions,
    ) -> Result<Self, TextError> {
        if width.is_some_and(|width| !width.is_finite() || width < 0.0) {
            return Err(TextError::UnsupportedLayout(
                "line width must be nonnegative and finite",
            ));
        }
        let placement = Placement {
            width,
            alignment,
            overflow: options.align_when_overflowing,
            last_line: options.last_line_alignment,
        };
        if self.placement != Some(placement) {
            self.layout.break_all_lines(width);
            self.layout.align(alignment, options);
            self.runs = Self::resolve(&self.layout, &mut self.fonts, Some(&self.source))?.into();
            self.placement = Some(placement);
        }
        Ok(self)
    }

    pub(super) fn resolve(
        layout: &Layout<()>,
        fonts: &mut HashMap<(u64, u32), Font>,
        source: Option<&Arc<str>>,
    ) -> Result<Vec<GlyphRun>, TextError> {
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    return Err(TextError::UnsupportedLayout(
                        "inline boxes require separate drawing",
                    ));
                };
                let run = glyph_run.run();
                if !run.normalized_coords().is_empty() || run.synthesis().any() {
                    return Err(TextError::UnsupportedFont("variation or synthetic style"));
                }
                let data = run.font();
                let key = (data.data.id(), data.index);
                if let std::collections::hash_map::Entry::Vacant(entry) = fonts.entry(key) {
                    entry.insert(Font::new(data.clone())?);
                }
                let glyphs: Vec<_> = glyph_run
                    .positioned_glyphs()
                    .map(|glyph| Glyph {
                        id: glyph.id,
                        x: glyph.x as f64,
                        y: glyph.y as f64,
                    })
                    .collect();
                let glyphs =
                    GlyphRun::new(fonts[&key].clone(), run.font_size(), glyphs, Hinting::None)?;
                runs.push(if let Some(source) = source {
                    glyphs.with_source(source.clone(), run.text_range())?
                } else {
                    glyphs
                });
            }
        }
        Ok(runs)
    }

    pub fn layout(&self) -> &Layout<()> {
        &self.layout
    }
    pub fn runs(&self) -> &[GlyphRun] {
        &self.runs
    }

    pub(super) fn shared_runs(&self) -> Arc<[GlyphRun]> {
        self.runs.clone()
    }

    pub fn commands(&self, origin: (f32, f32), color: Color) -> impl Iterator<Item = Command> + '_ {
        self.runs.iter().map(move |run| Command::GlyphRun {
            origin,
            run: run.clone(),
            color,
        })
    }

    pub fn prepare(&self) -> Result<PreparedText<'_>, PainterError> {
        let mut fonts: Vec<PreparedFont> = Vec::new();
        let mut indices = HashMap::new();
        let mut run_fonts = Vec::with_capacity(self.runs.len());
        for run in self.runs.iter() {
            let key = (run.font().identity(), run.size().to_bits());
            let index = match indices.entry(key) {
                std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let index = fonts.len();
                    fonts.push(PreparedFont::new(run.font(), run.size())?);
                    entry.insert(index);
                    index
                }
            };
            run_fonts.push(index);
        }
        Ok(PreparedText {
            text: self,
            fonts,
            run_fonts,
        })
    }
}

/// Prepared native leases borrow immutable shaped text for direct drawing.
pub struct PreparedText<'a> {
    text: &'a ShapedText,
    fonts: Vec<PreparedFont>,
    run_fonts: Vec<usize>,
}

impl PreparedText<'_> {
    pub fn draw(&self, painter: &mut Painter<'_>, origin: (f64, f64), scale: f64, color: Color) {
        for (run, &font) in self.text.runs.iter().zip(&self.run_fonts) {
            painter.draw_glyph_run(&self.fonts[font], run, origin, scale, color);
        }
    }
}

#[cfg(test)]
mod tests;
