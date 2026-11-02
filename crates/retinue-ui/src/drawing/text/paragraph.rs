use super::TextStyle;
use crate::{Bounds, drawing::DrawingContext, layout::LayoutSize};
use retinue_render::{
    Color, Command,
    painter::{CachedFont, text_width, wrap_text},
};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAlignment {
    #[default]
    Leading,
    Center,
    Trailing,
}

pub struct Paragraph {
    pub text: Rc<str>,
    pub font: Rc<CachedFont>,
    pub font_size: f32,
    pub line_height: f32,
    pub line_limit: Option<usize>,
    cache: RefCell<Vec<(u32, Rc<TextLayout>)>>,
    ideal_width: OnceCell<f32>,
    minimum_width: OnceCell<f32>,
}

impl Paragraph {
    pub fn new(text: impl Into<Rc<str>>, style: &TextStyle, line_limit: Option<usize>) -> Self {
        let line_height = style
            .font
            .horizontal_line_metrics(style.size)
            .map_or(style.size, |metrics| metrics.new_line_size);
        Self {
            text: text.into(),
            font: style.font.clone(),
            font_size: style.size,
            line_height,
            line_limit,
            cache: RefCell::default(),
            ideal_width: OnceCell::new(),
            minimum_width: OnceCell::new(),
        }
    }

    pub fn ideal_width(&self) -> f32 {
        *self
            .ideal_width
            .get_or_init(|| text_width(&self.font, &self.text, self.font_size))
    }
    pub fn minimum_width(&self) -> f32 {
        *self.minimum_width.get_or_init(|| {
            self.text
                .graphemes(true)
                .map(|part| text_width(&self.font, part, self.font_size))
                .fold(0.0, f32::max)
        })
    }

    pub fn layout(&self, width: f32) -> Rc<TextLayout> {
        let width = width.max(0.0).min(self.ideal_width());
        let key = width.to_bits();
        let mut cache = self.cache.borrow_mut();
        if let Some(index) = cache.iter().position(|(candidate, _)| *candidate == key) {
            let entry = cache.remove(index);
            let layout = entry.1.clone();
            cache.push(entry);
            return layout;
        }
        let mut lines: Vec<String> = wrap_text(&self.font, &self.text, self.font_size, width)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let truncated = self.line_limit.is_some_and(|limit| lines.len() > limit);
        if let Some(limit) = self.line_limit {
            lines.truncate(limit);
        }
        if truncated && let Some(line) = lines.last_mut() {
            let ellipsis = "\u{2026}";
            let ends: Vec<_> = line
                .grapheme_indices(true)
                .map(|(at, _)| at)
                .chain(std::iter::once(line.len()))
                .collect();
            let count = ends.partition_point(|&end| {
                text_width(
                    &self.font,
                    &format!("{}{ellipsis}", &line[..end]),
                    self.font_size,
                ) <= width
            });
            line.truncate(ends[count.saturating_sub(1)]);
            line.push_str(ellipsis);
        }
        let widths: Vec<_> = lines
            .iter()
            .map(|line| text_width(&self.font, line, self.font_size))
            .collect();
        let size = LayoutSize::new(
            widths.iter().copied().fold(0.0, f32::max),
            self.line_height * lines.len() as f32,
        );
        let baseline = self
            .font
            .horizontal_line_metrics(self.font_size)
            .map_or(self.font_size, |metrics| metrics.ascent);
        let layout = Rc::new(TextLayout {
            lines,
            widths,
            size,
            baseline,
            font: self.font.clone(),
            font_size: self.font_size,
            line_height: self.line_height,
        });
        if cache.len() == 4 {
            cache.remove(0);
        }
        cache.push((key, layout.clone()));
        layout
    }
}

impl PartialEq for Paragraph {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.font.id() == other.font.id()
            && self.font_size == other.font_size
            && self.line_height == other.line_height
            && self.line_limit == other.line_limit
    }
}

pub struct TextLayout {
    pub lines: Vec<String>,
    pub widths: Vec<f32>,
    pub size: LayoutSize,
    pub baseline: f32,
    font: Rc<CachedFont>,
    font_size: f32,
    line_height: f32,
}

impl TextLayout {
    pub fn draw(
        &self,
        context: &mut DrawingContext,
        bounds: Bounds,
        color: Color,
        alignment: TextAlignment,
    ) {
        let fraction = match alignment {
            TextAlignment::Leading => 0.0,
            TextAlignment::Center => 0.5,
            TextAlignment::Trailing => 1.0,
        };
        for (i, (line, width)) in self.lines.iter().zip(&self.widths).enumerate() {
            context.emit(Command::Text {
                bounds: Bounds::new(
                    bounds.x + (bounds.width - width).max(0.0) * fraction,
                    bounds.y + self.line_height * i as f32,
                    *width,
                    self.line_height,
                ),
                size: self.font_size,
                text: line.clone(),
                font: self.font.clone(),
                color,
                align: retinue_render::TextAlignment::Top,
            });
        }
    }
}
