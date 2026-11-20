use super::{font::TextFace, shaping::ShapedText};
use parley::{Alignment, AlignmentOptions};
use retinue_render::{
    Bounds, Color, Command,
    painter::CachedFont,
    resources::{Resource, ResourceCache, ResourceSize},
    text::{Glyph, GlyphRun, TextError},
};
use rustc_hash::FxHasher;
use std::{cell::RefCell, hash::Hasher, ops::Range, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelAlignment {
    Top,
    Center,
    WrappedCenter,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    face: (u64, u32),
    size: u32,
    width: Option<u32>,
    text: u64,
}

struct Label {
    text: String,
    runs: Vec<GlyphRun>,
    lines: Vec<Range<usize>>,
    width: f32,
    height: f32,
    _face: Rc<TextFace>,
}

impl ResourceSize for Label {
    fn heap_bytes(&self) -> usize {
        self.text.capacity()
            + self.runs.capacity() * size_of::<GlyphRun>()
            + self.lines.capacity() * size_of::<Range<usize>>()
            + self
                .runs
                .iter()
                .map(|run| run.glyphs().len())
                .sum::<usize>()
                * size_of::<Glyph>()
    }
}

thread_local! {
    static LABELS: RefCell<ResourceCache<Key, Label>> =
        RefCell::new(ResourceCache::new(2 * 1024 * 1024));
}

fn label(
    font: &CachedFont,
    text: &str,
    size: f32,
    width: Option<f32>,
) -> Result<Resource<Label>, TextError> {
    if !size.is_finite() || size <= 0.0 {
        return Err(TextError::InvalidSize);
    }
    if width.is_some_and(|width| !width.is_finite() || width < 0.0) {
        return Err(TextError::UnsupportedLayout("invalid label width"));
    }
    let mut hash = FxHasher::default();
    hash.write(text.as_bytes());
    let key = Key {
        face: font.native().identity(),
        size: size.to_bits(),
        width: width.map(f32::to_bits),
        text: hash.finish(),
    };
    LABELS.with(|labels| {
        let mut labels = labels.borrow_mut();
        if let Some(label) = labels.get(&key)
            && label.text == text
        {
            return Ok(label);
        }
        let face = TextFace::for_font(font)?;
        let shaped: ShapedText = face.shape(text, size)?.reflow(
            width,
            if width.is_some() {
                Alignment::Center
            } else {
                Alignment::Start
            },
            AlignmentOptions::default(),
        )?;
        let value = Label {
            text: text.into(),
            width: shaped.layout().width(),
            height: shaped.layout().height(),
            lines: shaped
                .layout()
                .lines()
                .map(|line| line.text_range())
                .collect(),
            runs: shaped.runs().to_vec(),
            _face: face,
        };
        Ok(labels.insert(key, value))
    })
}

pub fn text_width(font: &CachedFont, text: &str, size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    label(font, text, size, None)
        .expect("valid text measurement")
        .width
}

pub fn wrap_text<'a>(font: &CachedFont, text: &'a str, size: f32, width: f32) -> Vec<&'a str> {
    if text.is_empty() {
        return vec![""];
    }
    label(font, text, size, Some(width))
        .expect("valid text measurement")
        .lines
        .iter()
        .map(|range| text[range.clone()].trim_end())
        .collect()
}

pub fn fit_text_size(
    font: &CachedFont,
    text: &str,
    preferred: f32,
    minimum: f32,
    width: u32,
) -> f32 {
    let mut size = preferred;
    while size > minimum && text_width(font, text, size) > width as f32 {
        size -= 0.5;
    }
    size
}

/// Records native glyphs in logical coordinates. Colour and position do not reshape text.
pub fn label_commands(
    font: &CachedFont,
    bounds: Bounds,
    size: f32,
    text: &str,
    color: Color,
    alignment: LabelAlignment,
) -> Result<Vec<Command>, TextError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let label = label(
        font,
        text,
        size,
        (alignment == LabelAlignment::WrappedCenter).then_some(bounds.width),
    )?;
    let y = bounds.y
        + match alignment {
            LabelAlignment::Center => (bounds.height - label.height) / 2.0,
            _ => 0.0,
        };
    let commands: Vec<_> = label
        .runs
        .iter()
        .map(|run| Command::GlyphRun {
            origin: (bounds.x, y),
            run: run.clone(),
            color,
        })
        .collect();
    Ok(if alignment == LabelAlignment::Center {
        vec![Command::Clip {
            bounds,
            commands: std::sync::Arc::new(commands),
        }]
    } else {
        commands
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_render::painter::load_overlay_font;

    #[test]
    fn colour_and_position_changes_reuse_the_same_native_glyphs() {
        let font = load_overlay_font().unwrap();
        let draw = |bounds, color| {
            label_commands(
                &font,
                bounds,
                18.0,
                "office AV",
                color,
                LabelAlignment::Center,
            )
            .unwrap()
        };
        let a = draw(Bounds::new(0.0, 0.0, 140.0, 30.0), Color::rgb(0xffffff));
        let b = draw(Bounds::new(20.0, 10.0, 200.0, 30.0), Color::rgb(0xffaaff));
        assert_eq!(a.len(), b.len());
        let (Command::Clip { commands: a, .. }, Command::Clip { commands: b, .. }) = (&a[0], &b[0])
        else {
            panic!("bounded labels must clip")
        };
        for (a, b) in a.iter().zip(b.iter()) {
            let (
                Command::GlyphRun {
                    origin: a_origin,
                    run: a,
                    ..
                },
                Command::GlyphRun {
                    origin: b_origin,
                    run: b,
                    ..
                },
            ) = (a, b)
            else {
                panic!("label must emit native glyphs")
            };
            assert_eq!(a_origin.0 + 20.0, b_origin.0);
            assert_eq!(a_origin.1 + 10.0, b_origin.1);
            assert!(std::ptr::eq(a.glyphs().as_ptr(), b.glyphs().as_ptr()));
        }
    }

    #[test]
    fn wrapping_and_invalid_sizes_use_the_same_shaper_as_paragraphs() {
        let font = load_overlay_font().unwrap();
        let wide = label(&font, "one two three four", 18.0, Some(300.0)).unwrap();
        let narrow = label(&font, "one two three four", 18.0, Some(50.0)).unwrap();
        assert!(narrow.height > wide.height);
        assert!(label(&font, "text", f32::NAN, None).is_err());
        assert!(label(&font, "text", 18.0, Some(-1.0)).is_err());
        assert_eq!(
            wrap_text(&font, "hello\n\nworld", 18.0, 500.0),
            ["hello", "", "world"]
        );
        assert_eq!(
            text_width(&font, "office", 18.0),
            label(&font, "office", 18.0, None).unwrap().width
        );
    }
}
