use super::{Font, GlyphRun, PreparedFont};
#[cfg(test)]
use crate::PaintProgram;
use crate::painter::PainterError;
use std::{collections::HashMap, sync::Arc};

pub(crate) type FontKey = ((u64, u32), u32);

#[derive(Clone, Debug)]
pub(crate) struct FontRequest {
    font: Font,
    size: f32,
}

impl FontRequest {
    pub fn from_run(run: &GlyphRun) -> Self {
        Self {
            font: run.font().clone(),
            size: run.size(),
        }
    }

    pub fn key(&self) -> FontKey {
        (self.font.identity(), self.size.to_bits())
    }
}

/// Immutable leases for retained consumers, independent of this frame's repairs.
#[derive(Default)]
pub(crate) struct PreparedFonts {
    fonts: HashMap<FontKey, Result<Arc<PreparedFont>, PainterError>>,
    failed: bool,
}

impl PreparedFonts {
    #[cfg(test)]
    pub fn update<'a>(
        &self,
        programs: impl IntoIterator<Item = &'a PaintProgram>,
    ) -> (Self, usize) {
        self.update_requests(programs.into_iter().flat_map(PaintProgram::fonts))
    }

    pub(crate) fn update_requests<'a>(
        &self,
        requests: impl IntoIterator<Item = &'a FontRequest>,
    ) -> (Self, usize) {
        let mut fonts = HashMap::new();
        let mut prepared = 0;
        let mut failed = false;
        for request in requests {
            fonts.entry(request.key()).or_insert_with(|| {
                if let Some(Ok(font)) = self.fonts.get(&request.key()) {
                    return Ok(font.clone());
                }
                prepared += 1;
                let result = PreparedFont::new(&request.font, request.size).map(Arc::new);
                failed |= result.is_err();
                result
            });
        }
        (Self { fonts, failed }, prepared)
    }

    pub fn has_failures(&self) -> bool {
        self.failed
    }

    pub fn get(&self, run: &GlyphRun) -> Result<&PreparedFont, PainterError> {
        match self
            .fonts
            .get(&(run.font().identity(), run.size().to_bits()))
        {
            Some(Ok(font)) => Ok(font),
            Some(Err(error)) => Err(*error),
            None => Err(PainterError::invalid("unprepared glyph font")),
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.fonts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Color, Command,
        text::{Blob, FontData, Glyph, Hinting},
    };
    use std::sync::Arc;

    fn run(size: f32) -> GlyphRun {
        let font = Font::new(FontData::new(
            Blob::new(Arc::new(
                include_bytes!("../../../../assets/fonts/roboto/Roboto-Regular.ttf").as_slice(),
            )),
            0,
        ))
        .unwrap();
        GlyphRun::new(
            font,
            size,
            vec![Glyph {
                id: 42,
                x: 0.0,
                y: 20.0,
            }],
            Hinting::None,
        )
        .unwrap()
    }

    fn program(run: &GlyphRun) -> PaintProgram {
        PaintProgram::new(
            1,
            (80.0, 40.0),
            1.0,
            Arc::new(vec![Command::GlyphRun {
                run: run.clone(),
                origin: (0.0, 0.0),
                color: Color::rgb(0xffffff),
            }]),
        )
        .unwrap()
    }

    #[test]
    fn leases_follow_active_consumers_and_share_face_size() {
        let a = run(16.0);
        let b = GlyphRun::new(a.font().clone(), 20.0, a.glyphs().to_vec(), Hinting::None).unwrap();
        let a1 = program(&a);
        let a2 = program(&a);
        let b = program(&b);
        let (fonts, count) = PreparedFonts::default().update([&a1, &a2, &b]);
        assert_eq!(count, 2);
        assert_eq!(fonts.len(), 2);
        let (next, count) = fonts.update([&a2]);
        assert_eq!(count, 0);
        assert_eq!(next.len(), 1);
        assert!(std::ptr::eq(next.get(&a).unwrap(), fonts.get(&a).unwrap()));
        let (empty, count) = next.update([]);
        assert_eq!(count, 0);
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn a_failed_preparation_is_retried_not_retained_as_a_valid_lease() {
        let run = run(16.0);
        let old = PreparedFonts {
            fonts: HashMap::from([(
                FontRequest::from_run(&run).key(),
                Err(PainterError::invalid("test font failure")),
            )]),
            failed: true,
        };
        assert!(old.has_failures());
        assert!(old.get(&run).is_err());
        let (new, count) = old.update([&program(&run)]);
        assert_eq!(count, 1);
        assert!(!new.has_failures());
        assert!(new.get(&run).is_ok());
    }

    #[test]
    fn nested_resource_scopes_restore_and_unprepared_text_is_an_error() {
        let a = run(16.0);
        let b = run(20.0);
        let (a_fonts, _) = PreparedFonts::default().update([&program(&a)]);
        let (b_fonts, _) = PreparedFonts::default().update([&program(&b)]);
        let mut pixels = vec![0; 120 * 60 * 4];
        let mut painter = crate::painter::Painter::new(&mut pixels, 120, 60).unwrap();
        painter.with_text_fonts(&Arc::new(a_fonts), |p| {
            p.with_text_fonts(&Arc::new(b_fonts), |p| {
                p.draw_retained_glyph_run(&b, (40.0, 0.0), 1.0, Color::rgb(0xffffff));
            });
            p.draw_retained_glyph_run(&a, (0.0, 0.0), 1.0, Color::rgb(0xffffff));
        });
        painter.finish().unwrap();
        let mut painter = crate::painter::Painter::new(&mut pixels, 120, 60).unwrap();
        painter.draw_retained_glyph_run(&a, (0.0, 0.0), 1.0, Color::rgb(0xffffff));
        assert!(painter.finish().is_err());
    }

    #[test]
    fn prepared_font_tables_are_shared_read_only_worker_inputs() {
        fn transferable<T: Send + Sync>() {}
        transferable::<PreparedFonts>();
    }
}
