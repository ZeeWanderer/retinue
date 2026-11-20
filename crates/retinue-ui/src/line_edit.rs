use crate::{Rect, drawing::text::font::TextFace};
use parley::{
    StyleProperty,
    editing::{PlainEditor, PlainEditorDriver},
};
use retinue_render::{
    Bounds, Color,
    painter::{CachedFont, Painter},
    text::{GlyphRun, PreparedFont, TextError},
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[cfg(feature = "desktop")]
mod accessibility;
mod composition;
mod drawing;

#[derive(Clone, Copy, PartialEq)]
pub struct Colors {
    pub text: [u8; 4],
    pub selection: [u8; 4],
}

#[derive(Clone)]
pub struct EditResources {
    face: Rc<TextFace>,
}

impl EditResources {
    pub fn new(font: &CachedFont) -> Result<Self, TextError> {
        Ok(Self {
            face: TextFace::for_font(font)?,
        })
    }

    pub fn editor(&self) -> LineEdit {
        LineEdit::with_resources(self.clone())
    }

    pub fn password(&self) -> LineEdit {
        let mut editor = self.editor();
        editor.masked = Some(Box::new(self.editor()));
        editor
    }
}

pub struct LineEdit {
    resources: EditResources,
    editor: PlainEditor<()>,
    composition: Option<Box<composition::Original>>,
    #[cfg(feature = "desktop")]
    access_runs: Vec<accessibility::Span>,
    ime_bounds: std::cell::Cell<Option<(Rect, Rect)>>,
    frame: Option<(f32, f32)>,
    size: f32,
    scale: f32,
    scroll: f32,
    glyphs: Option<Result<Vec<GlyphRun>, TextError>>,
    fonts: rustc_hash::FxHashMap<((u64, u32), u32), PreparedFont>,
    fallback_stack: Option<parley::FontFamily<'static>>,
    changed: Instant,
    last_click: Option<(Instant, f32)>,
    caret_on: bool,
    masked: Option<Box<LineEdit>>,
}

impl LineEdit {
    pub fn new(font: &CachedFont) -> Result<Self, TextError> {
        Ok(EditResources::new(font)?.editor())
    }

    fn with_resources(resources: EditResources) -> Self {
        let mut editor = resources.face.editor();
        resources
            .face
            .edit(&mut editor, |driver| driver.refresh_layout());
        Self {
            resources,
            editor,
            composition: None,
            #[cfg(feature = "desktop")]
            access_runs: Vec::new(),
            ime_bounds: Default::default(),
            frame: None,
            size: 12.0,
            scale: 1.0,
            scroll: 0.0,
            glyphs: None,
            fonts: Default::default(),
            fallback_stack: None,
            changed: Instant::now(),
            last_click: None,
            caret_on: true,
            masked: None,
        }
    }

    pub fn password(font: &CachedFont) -> Result<Self, TextError> {
        Ok(EditResources::new(font)?.password())
    }

    fn edit(&mut self, action: impl FnOnce(&mut PlainEditorDriver<'_, ()>)) {
        self.resources.face.edit(&mut self.editor, |driver| {
            action(driver);
            driver.refresh_layout();
        });
    }

    fn sync_mask(&self, display: &mut LineEdit) {
        let text = self.editor.raw_text();
        let length = text.graphemes(true).count();
        if display.editor.raw_text().len() != length {
            display.set_text(&"*".repeat(length), false);
        }
        let selection = self.editor.raw_selection();
        let index = |index| text[..index].graphemes(true).count();
        let anchor = index(selection.anchor().index());
        let focus = index(selection.focus().index());
        let selected = display.editor.raw_selection();
        if selected.anchor().index() != anchor || selected.focus().index() != focus {
            display.edit(|driver| driver.select_byte_range(anchor, focus));
            display.changed(false);
        }
        display.changed = self.changed;
        display.caret_on = self.caret_on;
    }

    fn adopt_mask_selection(&mut self, display: &LineEdit) {
        let text = self.editor.raw_text();
        let index = |n| {
            text.grapheme_indices(true)
                .nth(n)
                .map_or(text.len(), |(i, _)| i)
        };
        let selected = display.editor.raw_selection();
        let anchor = index(selected.anchor().index());
        let focus = index(selected.focus().index());
        self.edit(|driver| driver.select_byte_range(anchor, focus));
        self.changed(false);
    }

    pub fn text(&self) -> String {
        self.composition.as_ref().map_or_else(
            || self.editor.raw_text().to_owned(),
            |original| original.text.clone(),
        )
    }

    pub fn is_password(&self) -> bool {
        self.masked.is_some()
    }

    pub fn display_text(&self) -> std::borrow::Cow<'_, str> {
        let text = self.editor.raw_text();
        if self.is_password() {
            "*".repeat(text.graphemes(true).count()).into()
        } else {
            text.into()
        }
    }

    pub fn set_text(&mut self, text: &str, select: bool) {
        assert!(!text.chars().any(char::is_control));
        self.cancel_preedit();
        let changed = self.editor.raw_text() != text;
        if changed {
            self.editor.set_text(text);
        }
        self.edit(|driver| {
            if select {
                driver.select_all();
            } else {
                driver.move_to_text_end();
            }
        });
        self.last_click = None;
        self.changed(changed);
    }

    pub fn selected_text(&self) -> Option<String> {
        if self.masked.is_some() {
            return None;
        }
        self.editor
            .selected_text()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    pub fn edit_stamp(&self) -> Instant {
        self.changed
    }

    pub fn insert(&mut self, text: &str, accept: impl FnOnce(&str) -> bool) -> bool {
        self.cancel_preedit();
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        let mut candidate = self.text();
        candidate.replace_range(self.editor.raw_selection().text_range(), text);
        if !accept(&candidate) {
            return false;
        }
        self.edit(|driver| driver.insert_or_replace_selection(text));
        self.changed(true);
        true
    }

    pub fn delete_selection(&mut self) -> bool {
        self.cancel_preedit();
        if self.editor.raw_selection().is_collapsed() {
            return false;
        }
        self.edit(|driver| driver.delete_selection());
        self.changed(true);
        true
    }

    pub fn key(&mut self, symbol: u32, control: bool, shift: bool) -> bool {
        let action: fn(&mut PlainEditorDriver<'_, ()>) = match (symbol, control, shift) {
            (0xff51, true, true) => |d| d.select_word_left(),
            (0xff53, true, true) => |d| d.select_word_right(),
            (0xff51, true, false) => |d| d.move_word_left(),
            (0xff53, true, false) => |d| d.move_word_right(),
            (0xff51, false, true) => |d| d.select_left(),
            (0xff53, false, true) => |d| d.select_right(),
            (0xff51, false, false) => |d| d.move_left(),
            (0xff53, false, false) => |d| d.move_right(),
            (0xff50, _, true) => |d| d.select_to_text_start(),
            (0xff57, _, true) => |d| d.select_to_text_end(),
            (0xff50, _, false) => |d| d.move_to_text_start(),
            (0xff57, _, false) => |d| d.move_to_text_end(),
            (0x61 | 0x41, true, _) => |d| d.select_all(),
            (0xff08, true, _) => |d| d.backdelete_word(),
            (0xffff, true, _) => |d| d.delete_word(),
            (0xff08, false, _) => |d| d.backdelete(),
            (0xffff, false, _) => |d| d.delete(),
            _ => return false,
        };
        self.cancel_preedit();
        self.edit(action);
        self.changed(matches!(symbol, 0xff08 | 0xffff));
        true
    }

    pub fn click(&mut self, x: f32, shift: bool) {
        self.cancel_preedit();
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.click(x, shift);
            self.adopt_mask_selection(&display);
            self.masked = Some(display);
            return;
        }
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(at, previous)| {
            now.duration_since(at) < Duration::from_millis(400) && (x - previous).abs() < 4.0
        });
        self.last_click = if double { None } else { Some((now, x)) };
        let x = x / self.scale + self.scroll;
        self.edit(|driver| {
            if shift {
                driver.shift_click_extension(x, 1.0);
            } else if double {
                driver.select_word_at_point(x, 1.0);
            } else {
                driver.move_to_point(x, 1.0);
            }
        });
        self.changed(false);
    }

    pub fn drag(&mut self, x: f32) {
        self.cancel_preedit();
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.drag(x);
            self.adopt_mask_selection(&display);
            self.masked = Some(display);
            return;
        }
        let x = x / self.scale + self.scroll;
        self.edit(|driver| driver.extend_selection_to_point(x, 1.0));
        self.changed(false);
    }

    pub fn tick(&mut self) -> bool {
        let visible = self.changed.elapsed().as_millis() % 1000 < 500;
        let changed = self.caret_on != visible;
        self.caret_on = visible;
        changed
    }

    pub fn next_blink(&self, now: Instant) -> Instant {
        let visible = now.saturating_duration_since(self.changed).as_millis() % 1000 < 500;
        if visible != self.caret_on {
            now
        } else {
            crate::schedule::next_interval(self.changed, Duration::from_millis(500), now)
        }
    }

    pub fn layout(&mut self, rect: Rect, size: f32) {
        self.layout_scaled(rect, size, 1.0);
    }

    /// Accepts device-pixel geometry and input while retaining logical text layout.
    pub fn layout_scaled(&mut self, rect: Rect, size: f32, scale: f32) {
        if !size.is_finite() || size <= 0.0 || !scale.is_finite() || scale <= 0.0 {
            return;
        }
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.layout_scaled(rect, size, scale);
            self.masked = Some(display);
        }
        let size = size / scale;
        let frame = Some((rect.width as f32 / scale, rect.height as f32 / scale));
        if self.scale != scale || self.size != size || self.frame != frame {
            self.ime_bounds.set(None);
        }
        self.scale = scale;
        if self.size != size {
            self.size = size;
            self.editor
                .edit_styles()
                .insert(StyleProperty::FontSize(size));
            self.edit(|_| {});
            self.glyphs = None;
            self.fonts.clear();
        }
        self.frame = frame;
        self.reveal_caret();
    }

    fn visit_ink(
        &mut self,
        rect: Rect,
        size: f32,
        scale: f32,
        colors: Colors,
        emit: &mut impl FnMut(drawing::Ink<'_>),
    ) -> Result<(), TextError> {
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }
        if !size.is_finite() || size <= 0.0 || !scale.is_finite() || scale <= 0.0 {
            return Err(TextError::InvalidSize);
        }
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            let result = display.visit_ink(rect, size, scale, colors, emit);
            self.masked = Some(display);
            return result;
        }
        self.layout_scaled(rect, size, scale);
        let size = size / scale;
        let layout = self
            .editor
            .try_layout()
            .expect("editor layout refreshed after edits");
        let runs = self
            .glyphs
            .get_or_insert_with(|| self.resources.face.glyphs(layout))
            .as_ref()
            .map_err(|error| *error)?;
        let y = (rect.height as f32 / scale - layout.height()) * 0.5;
        let fill =
            |emit: &mut dyn FnMut(drawing::Ink<'_>), x0: f32, y0: f32, x1: f32, y1: f32, color| {
                let left = (x0 * scale).floor().clamp(0.0, rect.width as f32) / scale;
                let right = (x1 * scale).ceil().clamp(0.0, rect.width as f32) / scale;
                let top = (y0 * scale).floor().clamp(0.0, rect.height as f32) / scale;
                let bottom = (y1 * scale).ceil().clamp(0.0, rect.height as f32) / scale;
                emit(drawing::Ink::Fill(
                    Bounds::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0)),
                    color,
                ));
            };
        self.editor.selection_geometry_with(|selection, _| {
            fill(
                emit,
                selection.x0 as f32 - self.scroll,
                selection.y0 as f32 + y,
                selection.x1 as f32 - self.scroll,
                selection.y1 as f32 + y,
                colors.selection,
            );
        });
        for run in runs.iter() {
            emit(drawing::Ink::Glyph(run, (-self.scroll, y), colors.text));
        }
        if let Some(range) = self.editor.raw_compose() {
            use parley::{
                editing::{Cursor, Selection},
                layout::Affinity,
            };
            let marked = Selection::new(
                Cursor::from_byte_index(layout, range.start, Affinity::Downstream),
                Cursor::from_byte_index(layout, range.end, Affinity::Upstream),
            );
            marked.geometry_with(layout, |r, _| {
                let bottom = (r.y1 as f32 + y).min(rect.height as f32 / scale);
                fill(
                    emit,
                    r.x0 as f32 - self.scroll,
                    bottom - 1.0 / scale,
                    r.x1 as f32 - self.scroll,
                    bottom,
                    colors.text,
                );
            });
        }
        if self.caret_on
            && let Some(caret) = self.editor.cursor_geometry((size / 12.0).max(1.0))
        {
            fill(
                emit,
                caret.x0 as f32 - self.scroll,
                caret.y0 as f32 + y,
                caret.x1 as f32 - self.scroll,
                caret.y1 as f32 + y,
                colors.text,
            );
        }
        Ok(())
    }

    fn changed(&mut self, text: bool) {
        self.ime_bounds.set(None);
        if text {
            #[cfg(feature = "desktop")]
            self.access_runs.clear();
            self.glyphs = None;
            if self.masked.is_none() {
                let family = self.resources.face.families(self.editor.raw_text());
                let mixed = matches!(family, parley::FontFamily::List(_));
                if mixed || self.fallback_stack.is_some() {
                    let family = family.into_owned();
                    if mixed.then_some(&family) != self.fallback_stack.as_ref() {
                        self.fallback_stack = mixed.then(|| family.clone());
                        self.editor
                            .edit_styles()
                            .insert(StyleProperty::FontFamily(family));
                        self.edit(|_| {});
                    }
                }
            }
        }
        self.reveal_caret();
        self.changed = Instant::now();
        self.caret_on = true;
    }

    fn reveal_caret(&mut self) {
        let Some((width, _)) = self.frame else {
            return;
        };
        let layout = self
            .editor
            .try_layout()
            .expect("editor layout refreshed after edits");
        let width = (width - (self.size / 12.0).max(1.0)).max(0.0);
        let Some(caret) = self.editor.cursor_geometry(1.0) else {
            return;
        };
        self.scroll = self
            .scroll
            .min(caret.x0 as f32)
            .max(caret.x0 as f32 - width)
            .clamp(0.0, (layout.full_width() - width).max(0.0));
    }

    #[cfg(test)]
    fn caret_x(&self) -> Option<f32> {
        self.editor
            .cursor_geometry(1.0)
            .map(|caret| caret.x0 as f32 - self.scroll)
    }
}

#[cfg(test)]
mod tests;
