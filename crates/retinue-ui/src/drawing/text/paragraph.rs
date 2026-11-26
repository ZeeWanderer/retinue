use super::{TextStyle, font::TextFace, shaping::ShapedText};
use crate::{
    Bounds,
    drawing::DrawingContext,
    layout::{Available, LayoutSize},
};
use parley::{Alignment, AlignmentOptions, ContentWidths};
use retinue_render::{
    Color, Command,
    text::{Font, GlyphRun, TextError},
};
use std::{
    cell::{Ref, RefCell},
    sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAlignment {
    #[default]
    Leading,
    Center,
    Trailing,
}

impl TextAlignment {
    fn native(self) -> Alignment {
        match self {
            Self::Leading => Alignment::Start,
            Self::Center => Alignment::Center,
            Self::Trailing => Alignment::End,
        }
    }
    fn fraction(self, rtl: bool) -> f32 {
        match (self, rtl) {
            (Self::Center, _) => 0.5,
            (Self::Leading, false) | (Self::Trailing, true) => 0.0,
            _ => 1.0,
        }
    }
}

pub struct Paragraph {
    spec: ParagraphSpec,
    state: RefCell<ParagraphState>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct ParagraphSpec {
    text: Arc<str>,
    font: Font,
    font_size: f32,
    line_limit: Option<usize>,
}

#[derive(Default)]
pub(crate) struct ParagraphState {
    content: Option<Result<Content, TextError>>,
}

struct Content {
    widths: ContentWidths,
    state: Placement,
}

struct Placement {
    full: Option<ShapedText>,
    visible: Option<TextLayout>,
    key: Option<(u32, TextAlignment)>,
    error: Option<TextError>,
}

impl Paragraph {
    pub fn new(text: impl Into<Arc<str>>, style: &TextStyle, line_limit: Option<usize>) -> Self {
        Self {
            spec: ParagraphSpec::new(text, style, line_limit),
            state: RefCell::default(),
        }
    }

    fn initialize(&self, first: Option<(Available, TextAlignment)>) -> Result<(), TextError> {
        if self.state.borrow().content.is_none() {
            self.state.borrow_mut().content(&self.spec, first)?;
        }
        self.state.borrow().current().map(|_| ())
    }

    pub fn ideal_width(&self) -> Result<f32, TextError> {
        self.initialize(None)?;
        Ok(self.state.borrow().current()?.widths.max)
    }

    pub fn minimum_width(&self) -> Result<f32, TextError> {
        self.initialize(None)?;
        Ok(self.state.borrow().current()?.widths.min)
    }

    /// Reuses shaped text for a new placement. Release the returned borrow before
    /// requesting a different width or alignment; recorded glyph runs own their data.
    pub fn layout(
        &self,
        width: f32,
        alignment: TextAlignment,
    ) -> Result<Ref<'_, TextLayout>, TextError> {
        validate_width(width)?;
        self.initialize(Some((Available::Limit(width), alignment)))?;
        let state = self.state.borrow();
        let content = state.current()?;
        if !content.placed(
            column_width(content.widths, Available::Limit(width)),
            alignment,
        )? {
            drop(state);
            self.state
                .borrow_mut()
                .layout(&self.spec, width, alignment)?;
        }
        Ok(Ref::map(self.state.borrow(), |state| {
            state.placed().unwrap()
        }))
    }
}

impl ParagraphSpec {
    pub(crate) fn new(
        text: impl Into<Arc<str>>,
        style: &TextStyle,
        line_limit: Option<usize>,
    ) -> Self {
        Self {
            text: text.into(),
            font: style.font.native(),
            font_size: style.size,
            line_limit,
        }
    }

    fn shape(&self, text: &str) -> Result<ShapedText, TextError> {
        #[cfg(feature = "layout-profiling")]
        let _scope = crate::profiling::Scope::text(crate::profiling::layout::TextPart::Shape);
        TextFace::cached(&self.font)?.shape(text, self.font_size)
    }
}

impl ParagraphState {
    fn content(
        &mut self,
        spec: &ParagraphSpec,
        first: Option<(Available, TextAlignment)>,
    ) -> Result<&mut Content, TextError> {
        self.content
            .get_or_insert_with(|| {
                #[cfg(feature = "layout-profiling")]
                let _scope =
                    crate::profiling::Scope::text(crate::profiling::layout::TextPart::Shape);
                let face = TextFace::cached(&spec.font)?;
                let layout = face.build(&spec.text, spec.font_size)?;
                // Intrinsic widths depend on shaped clusters, not on positioned glyph runs.
                let widths = layout.calculate_content_widths();
                let width = first.map(|(proposal, _)| column_width(widths, proposal));
                let alignment = first.map(|(_, alignment)| alignment.native());
                let full = face.position(&spec.text, layout, width, alignment)?;
                #[cfg(feature = "layout-profiling")]
                if _scope.recorded() {
                    crate::profiling::layout::paragraphs::built(
                        &spec.text,
                        spec.font.identity(),
                        spec.font_size,
                        width,
                        first.map(|(_, alignment)| alignment as u8),
                        spec.line_limit,
                    );
                }
                Ok(Content {
                    widths,
                    state: Placement {
                        full: Some(full),
                        visible: None,
                        key: None,
                        error: None,
                    },
                })
            })
            .as_mut()
            .map_err(|error| *error)
    }

    fn current(&self) -> Result<&Content, TextError> {
        self.content
            .as_ref()
            .expect("initialized paragraph")
            .as_ref()
            .map_err(|error| *error)
    }

    pub(crate) fn placed(&self) -> Result<&TextLayout, TextError> {
        let state = &self.current()?.state;
        if let Some(error) = state.error {
            return Err(error);
        }
        state
            .visible
            .as_ref()
            .ok_or(TextError::UnsupportedLayout("paragraph not placed"))
    }

    pub(crate) fn layout(
        &mut self,
        spec: &ParagraphSpec,
        width: f32,
        alignment: TextAlignment,
    ) -> Result<&TextLayout, TextError> {
        self.layout_for(spec, Available::Limit(width), alignment)
    }

    pub(crate) fn layout_for(
        &mut self,
        spec: &ParagraphSpec,
        proposal: Available,
        alignment: TextAlignment,
    ) -> Result<&TextLayout, TextError> {
        if let Available::Limit(width) = proposal {
            validate_width(width)?;
        }
        let content = self.content(spec, Some((proposal, alignment)))?;
        let width = column_width(content.widths, proposal);
        if content.placed(width, alignment)? {
            return Ok(content.state.visible.as_ref().unwrap());
        }
        let key = (width.to_bits(), alignment);
        let state = &mut content.state;
        if let Err(error) = state.place(spec, width, alignment) {
            state.error = Some(error);
            return Err(error);
        }
        state.key = Some(key);
        Ok(state.visible.as_ref().unwrap())
    }
}

fn validate_width(width: f32) -> Result<(), TextError> {
    if width.is_nan() || width < 0.0 {
        return Err(TextError::UnsupportedLayout("invalid paragraph width"));
    }
    Ok(())
}

fn column_width(widths: ContentWidths, proposal: Available) -> f32 {
    let width = match proposal {
        Available::MinContent => widths.min,
        Available::Ideal => widths.max,
        Available::Limit(width) => width,
    }
    .max(widths.min)
    .min(widths.max);
    // A scale round trip can undershoot the intrinsic width by one ULP.
    if width.next_up() >= widths.max {
        widths.max
    } else {
        width
    }
}

impl Content {
    fn placed(&self, width: f32, alignment: TextAlignment) -> Result<bool, TextError> {
        if let Some(error) = self.state.error {
            return Err(error);
        }
        Ok(self.state.key == Some((width.to_bits(), alignment)))
    }
}

impl Placement {
    fn place(
        &mut self,
        paragraph: &ParagraphSpec,
        width: f32,
        alignment: TextAlignment,
    ) -> Result<(), TextError> {
        // Only an elided display needs a second layout. Unclamped placement moves
        // the existing shaping back out of the previous display; it never clones it.
        let full = self
            .full
            .take()
            .or_else(|| self.visible.take().map(|layout| layout.shaped))
            .unwrap();
        self.visible = None;
        let full = full.reflow(Some(width), alignment.native(), AlignmentOptions::default())?;
        let truncated = paragraph
            .line_limit
            .is_some_and(|limit| full.layout().len() > limit);
        let (text, shaped) = if truncated {
            let shown = elide(paragraph, &full, width, alignment)?;
            self.full = Some(full);
            shown
        } else {
            (paragraph.text.clone(), full)
        };
        let hidden = paragraph.line_limit == Some(0);
        let layout = shaped.layout();
        let size = if hidden {
            LayoutSize::new(0.0, 0.0)
        } else {
            // The column is not the ink width: using shortened lines as the next
            // proposal can change wrapping/elision during final placement.
            LayoutSize::new(width, layout.height())
        };
        let baseline = if hidden {
            0.0
        } else {
            layout.first_baseline().unwrap_or(0.0)
        };
        let last_baseline = if hidden {
            0.0
        } else {
            layout.last_baseline().unwrap_or(0.0)
        };
        self.visible = Some(TextLayout {
            text,
            shaped,
            size,
            baseline,
            last_baseline,
            width,
            alignment,
            hidden,
        });
        Ok(())
    }
}

fn elide(
    paragraph: &ParagraphSpec,
    full: &ShapedText,
    width: f32,
    alignment: TextAlignment,
) -> Result<(Arc<str>, ShapedText), TextError> {
    #[cfg(feature = "layout-profiling")]
    let _scope = crate::profiling::Scope::text(crate::profiling::layout::TextPart::Elide);
    let limit = paragraph.line_limit.unwrap();
    let range = limit
        .checked_sub(1)
        .and_then(|line| full.layout().get(line))
        .map(|line| line.text_range());
    let (start, mut end) = range.map_or((0, 0), |range| {
        (
            range.start,
            range.start + paragraph.text[range].trim_end().len(),
        )
    });
    loop {
        let text: Arc<str> = if limit == 0 {
            "".into()
        } else {
            format!("{}\u{2026}", &paragraph.text[..end]).into()
        };
        let shaped = paragraph.shape(&text)?.reflow(
            Some(width),
            alignment.native(),
            AlignmentOptions::default(),
        )?;
        if limit == 0 || (shaped.layout().len() <= limit && shaped.layout().width() <= width) {
            return Ok((text, shaped));
        }
        if end == start {
            let text: Arc<str> = paragraph.text[..start].into();
            let shaped = paragraph.shape(&text)?.reflow(
                Some(width),
                alignment.native(),
                AlignmentOptions::default(),
            )?;
            return Ok((text, shaped));
        }
        end = start
            + paragraph.text[start..end]
                .grapheme_indices(true)
                .next_back()
                .unwrap()
                .0;
    }
}

impl PartialEq for Paragraph {
    fn eq(&self, other: &Self) -> bool {
        self.spec == other.spec
    }
}

pub struct TextLayout {
    text: Arc<str>,
    shaped: ShapedText,
    pub size: LayoutSize,
    pub baseline: f32,
    pub last_baseline: f32,
    width: f32,
    alignment: TextAlignment,
    hidden: bool,
}

pub(crate) struct ParagraphDrawing {
    runs: Arc<[GlyphRun]>,
    width: f32,
    alignment: TextAlignment,
    rtl: bool,
    hidden: bool,
}

impl ParagraphDrawing {
    pub(crate) fn draw(&self, context: &mut DrawingContext, bounds: Bounds, color: Color) {
        if self.hidden {
            return;
        }
        let origin = (
            bounds.x + (bounds.width - self.width).max(0.0) * self.alignment.fraction(self.rtl),
            bounds.y,
        );
        for run in self.runs.iter() {
            context.emit(Command::GlyphRun {
                origin,
                run: run.clone(),
                color,
            });
        }
    }
}

impl TextLayout {
    pub(crate) fn drawing(&self) -> ParagraphDrawing {
        ParagraphDrawing {
            runs: self.shaped.shared_runs(),
            width: self.width,
            alignment: self.alignment,
            rtl: self.shaped.layout().is_rtl(),
            hidden: self.hidden,
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.shaped
            .layout()
            .lines()
            .take(if self.hidden { 0 } else { usize::MAX })
            .map(|line| {
                // Parley shapes a synthetic space for empty-text line metrics.
                if self.text.is_empty() {
                    ""
                } else {
                    self.text[line.text_range()].trim_end()
                }
            })
    }

    pub fn shaped(&self) -> &ShapedText {
        &self.shaped
    }

    pub fn origin(&self, bounds: Bounds) -> (f32, f32) {
        let extra = (bounds.width - self.width).max(0.0);
        (
            bounds.x + extra * self.alignment.fraction(self.shaped.layout().is_rtl()),
            bounds.y,
        )
    }

    pub fn draw(&self, context: &mut DrawingContext, bounds: Bounds, color: Color) {
        if !self.hidden {
            context.shaped_text(&self.shaped, self.origin(bounds), color);
        }
    }
}

#[cfg(test)]
mod tests;
