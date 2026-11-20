use super::*;
use retinue_render::{Bounds, Command};
use std::sync::Arc;

pub(super) enum Ink<'a> {
    Fill(Bounds, [u8; 4]),
    Glyph(&'a GlyphRun, (f32, f32), [u8; 4]),
}

impl LineEdit {
    pub fn paint(&mut self, painter: &mut Painter<'_>, rect: Rect, size: f32, colors: Colors) {
        if rect.is_empty() {
            return;
        }
        if !size.is_finite() || size <= 0.0 {
            painter.fail(TextError::InvalidSize.into());
            return;
        }
        self.layout(rect, size);
        let mut fonts = std::mem::take(&mut self.fonts);
        painter.local(rect, |painter, _| {
            if let Err(error) = self.visit_ink(rect, size, 1.0, colors, &mut |ink| match ink {
                Ink::Fill(rect, color) => painter.fill_rounded_rect(
                    rect.x as u32,
                    rect.y as u32,
                    rect.width as u32,
                    rect.height as u32,
                    0,
                    color,
                ),
                Ink::Glyph(run, origin, color) => {
                    use std::collections::hash_map::Entry;
                    let font = match fonts.entry((run.font().identity(), run.size().to_bits())) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => match PreparedFont::new(run.font(), run.size()) {
                            Ok(font) => entry.insert(font),
                            Err(error) => {
                                painter.fail(error);
                                return;
                            }
                        },
                    };
                    painter.draw_glyph_run(
                        font,
                        run,
                        (origin.0 as f64, origin.1 as f64),
                        1.0,
                        Color::from_bgra(color),
                    )
                }
            }) {
                painter.fail(error.into());
            }
        });
        self.fonts = fonts;
    }

    /// Captures current editor ink, sharing glyph storage without retaining editing state.
    pub fn commands(
        &mut self,
        rect: Rect,
        size: f32,
        colors: Colors,
    ) -> Result<Vec<Command>, TextError> {
        self.commands_scaled(rect, size, 1.0, colors)
    }

    pub(crate) fn commands_scaled(
        &mut self,
        rect: Rect,
        size: f32,
        scale: f32,
        colors: Colors,
    ) -> Result<Vec<Command>, TextError> {
        let mut commands = Vec::new();
        self.visit_ink(rect, size, scale, colors, &mut |ink| {
            commands.push(match ink {
                Ink::Fill(rect, color) => Command::Fill {
                    bounds: rect,
                    radius: 0.0,
                    color: Color::from_bgra(color),
                },
                Ink::Glyph(run, origin, color) => Command::GlyphRun {
                    origin,
                    run: run.clone(),
                    color: Color::from_bgra(color),
                },
            })
        })?;
        if commands.is_empty() {
            return Ok(commands);
        }
        Ok(vec![Command::Translate {
            offset: (rect.x as f32 / scale, rect.y as f32 / scale),
            commands: Arc::new(vec![Command::Clip {
                bounds: Bounds::new(
                    0.0,
                    0.0,
                    rect.width as f32 / scale,
                    rect.height as f32 / scale,
                ),
                commands: Arc::new(commands),
            }]),
        }])
    }
}
