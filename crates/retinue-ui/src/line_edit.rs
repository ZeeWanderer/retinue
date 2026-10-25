use crate::{
    Rect,
    painter::{Painter, RasterImage},
};
use cosmic_text::{
    Action, Attrs, Buffer, Color, Cursor, Edit, Editor, Family, FontSystem, Metrics, Motion,
    Selection, Shaping, SwashCache, Wrap, fontdb,
};
use image::{Pixel, Rgba, RgbaImage};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
pub struct Colors {
    pub text: [u8; 4],
    pub selection: [u8; 4],
}

pub struct LineEdit {
    fonts: FontSystem,
    editor: Editor<'static>,
    glyphs: SwashCache,
    family: String,
    frame: Option<(u32, u32, f32)>,
    colors: Option<Colors>,
    raster: Option<RasterImage>,
    changed: Instant,
    last_click: Option<(Instant, f32)>,
    caret_on: bool,
    masked: Option<Box<LineEdit>>,
}

impl LineEdit {
    pub fn new(font: &[u8], family: &str) -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_data(font.to_vec());
        let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(12.0, 16.0));
        buffer.set_wrap(Wrap::None);
        Self {
            fonts,
            editor: Editor::new(buffer),
            glyphs: SwashCache::new(),
            family: family.into(),
            frame: None,
            colors: None,
            raster: None,
            changed: Instant::now(),
            last_click: None,
            caret_on: true,
            masked: None,
        }
    }

    pub fn password(font: &[u8], family: &str) -> Self {
        let mut editor = Self::new(font, family);
        editor.masked = Some(Box::new(Self::new(font, family)));
        editor
    }

    fn sync_mask(&self, display: &mut LineEdit) {
        let text = self.text();
        let hidden = "*".repeat(text.chars().count());
        let cursor = Cursor::new(0, text[..self.editor.cursor().index].chars().count());
        let selection = match self.editor.selection() {
            Selection::Normal(c) => {
                Selection::Normal(Cursor::new(0, text[..c.index].chars().count()))
            }
            _ => Selection::None,
        };
        if display.text() != hidden {
            display.set_text(&hidden, false);
        }
        if display.editor.cursor() != cursor || display.editor.selection() != selection {
            display.editor.set_cursor(cursor);
            display.editor.set_selection(selection);
            display.changed();
        }
        display.changed = self.changed;
        display.caret_on = self.caret_on;
    }

    fn adopt_mask_selection(&mut self, display: &LineEdit) {
        let text = self.text();
        let index = |n| {
            text.char_indices()
                .map(|(i, _)| i)
                .nth(n)
                .unwrap_or(text.len())
        };
        self.editor
            .set_cursor(Cursor::new(0, index(display.editor.cursor().index)));
        self.editor.set_selection(match display.editor.selection() {
            Selection::Normal(c) => Selection::Normal(Cursor::new(0, index(c.index))),
            _ => Selection::None,
        });
        self.changed();
    }

    pub fn text(&self) -> String {
        self.editor
            .with_buffer(|buffer| buffer.lines[0].text().to_owned())
    }

    pub fn set_text(&mut self, text: &str, select: bool) {
        assert!(!text.chars().any(char::is_control));
        self.editor.with_buffer_mut(|buffer| {
            buffer.set_text(
                text,
                &Attrs::new().family(Family::Name(&self.family)),
                Shaping::Advanced,
                None,
            )
        });
        self.editor.set_cursor(Cursor::new(0, text.len()));
        self.editor.set_selection(if select {
            Selection::Normal(Cursor::new(0, 0))
        } else {
            Selection::None
        });
        self.last_click = None;
        self.changed();
    }

    pub fn selected_text(&self) -> Option<String> {
        if self.masked.is_some() {
            return None;
        }
        self.editor.copy_selection().filter(|s| !s.is_empty())
    }

    pub fn edit_stamp(&self) -> Instant {
        self.changed
    }

    pub fn insert(&mut self, text: &str, accept: impl FnOnce(&str) -> bool) -> bool {
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        let mut candidate = self.text();
        let cursor = self.editor.cursor();
        let (start, end) = self.editor.selection_bounds().unwrap_or((cursor, cursor));
        candidate.replace_range(start.index..end.index, text);
        if !accept(&candidate) {
            return false;
        }
        self.editor.insert_string(text, None);
        self.changed();
        true
    }

    pub fn delete_selection(&mut self) -> bool {
        let changed = self.editor.delete_selection();
        if changed {
            self.changed();
        }
        changed
    }

    pub fn key(&mut self, symbol: u32, control: bool, shift: bool) -> bool {
        let motion = match symbol {
            0xff51 if control => Some(Motion::PreviousWord),
            0xff53 if control => Some(Motion::NextWord),
            0xff51 => Some(Motion::Left),
            0xff53 => Some(Motion::Right),
            0xff50 => Some(Motion::Home),
            0xff57 => Some(Motion::End),
            _ => None,
        };
        if let Some(motion) = motion {
            if shift {
                if self.editor.selection() == Selection::None {
                    self.editor
                        .set_selection(Selection::Normal(self.editor.cursor()));
                }
            } else if matches!(motion, Motion::Left | Motion::Right)
                && let Some((start, end)) = self.editor.selection_bounds()
            {
                self.editor
                    .set_cursor(if motion == Motion::Left { start } else { end });
                self.editor.set_selection(Selection::None);
                self.changed();
                return true;
            } else {
                self.editor.set_selection(Selection::None);
            }
            self.editor.action(&mut self.fonts, Action::Motion(motion));
        } else {
            match symbol {
                0x61 | 0x41 if control => {
                    self.editor
                        .set_selection(Selection::Normal(Cursor::new(0, 0)));
                    self.editor.set_cursor(Cursor::new(0, self.text().len()));
                }
                0xff08 | 0xffff => {
                    if control && self.editor.selection() == Selection::None {
                        self.editor
                            .set_selection(Selection::Normal(self.editor.cursor()));
                        self.editor.action(
                            &mut self.fonts,
                            Action::Motion(if symbol == 0xff08 {
                                Motion::PreviousWord
                            } else {
                                Motion::NextWord
                            }),
                        );
                    }
                    self.editor.action(
                        &mut self.fonts,
                        if symbol == 0xff08 {
                            Action::Backspace
                        } else {
                            Action::Delete
                        },
                    );
                }
                _ => return false,
            }
        }
        self.changed();
        true
    }

    pub fn click(&mut self, x: f32, shift: bool) {
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.click(x, shift);
            self.adopt_mask_selection(&display);
            self.masked = Some(display);
            return;
        }
        let x = x + self.horizontal_scroll();
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(at, previous)| {
            now.duration_since(at) < Duration::from_millis(400) && (x - previous).abs() < 4.0
        });
        self.last_click = if double { None } else { Some((now, x)) };
        if shift && self.editor.selection() == Selection::None {
            self.editor
                .set_selection(Selection::Normal(self.editor.cursor()));
        }
        self.editor.action(
            &mut self.fonts,
            if shift {
                Action::Drag { x: x as i32, y: 1 }
            } else if double {
                Action::DoubleClick { x: x as i32, y: 1 }
            } else {
                Action::Click { x: x as i32, y: 1 }
            },
        );
        self.changed();
    }

    pub fn drag(&mut self, x: f32) {
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.drag(x);
            self.adopt_mask_selection(&display);
            self.masked = Some(display);
            return;
        }
        let x = x + self.horizontal_scroll();
        self.editor
            .action(&mut self.fonts, Action::Drag { x: x as i32, y: 1 });
        self.changed();
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
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.layout(rect, size);
            self.masked = Some(display);
        }
        let frame = (rect.width, rect.height, size);
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.editor.with_buffer_mut(|buffer| {
                buffer.set_metrics_and_size(
                    Metrics::new(size, rect.height as f32),
                    Some(rect.width.saturating_sub(2) as f32),
                    Some(rect.height as f32),
                );
            });
            self.raster = None;
            let cursor = self.editor.cursor();
            self.editor.with_buffer_mut(|buffer| {
                buffer.shape_until_cursor(&mut self.fonts, cursor, false)
            });
            self.clamp_horizontal_scroll();
        }
    }

    pub fn paint(&mut self, painter: &mut Painter<'_>, rect: Rect, size: f32, colors: Colors) {
        if let Some(mut display) = self.masked.take() {
            self.sync_mask(&mut display);
            display.paint(painter, rect, size, colors);
            self.masked = Some(display);
            return;
        }
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        self.layout(rect, size);
        if self.colors != Some(colors) {
            self.colors = Some(colors);
            self.raster = None;
        }
        if self.raster.is_none() {
            let mut bitmap = RgbaImage::new(rect.width, rect.height);
            let color = |c: [u8; 4]| Color::rgba(c[2], c[1], c[0], c[3]);
            self.editor.shape_as_needed(&mut self.fonts, false);
            let horizontal = self.horizontal_scroll().round() as i32;
            self.editor.draw(
                &mut self.fonts,
                &mut self.glyphs,
                color(colors.text),
                Color::rgba(0, 0, 0, 0),
                color(colors.selection),
                color(colors.text),
                |x, y, w, h, c| {
                    let x = x - horizontal;
                    let pixel = Rgba([c.r(), c.g(), c.b(), c.a()]);
                    for iy in y.max(0)..(y + h as i32).min(rect.height as i32) {
                        for ix in x.max(0)..(x + w as i32).min(rect.width as i32) {
                            bitmap.get_pixel_mut(ix as u32, iy as u32).blend(&pixel);
                        }
                    }
                },
            );
            match RasterImage::from_rgba(bitmap) {
                Ok(image) => self.raster = Some(image),
                Err(error) => {
                    painter.fail(error);
                    return;
                }
            }
        }
        if let Some(image) = &self.raster {
            painter.draw_image(image, rect.x, rect.y, rect.width, rect.height);
        }
        if self.caret_on
            && let Some(x) = self.caret_x()
        {
            let x = rect.x + (x.max(0.0) as u32).min(rect.width.saturating_sub(1));
            let pad = rect.height.saturating_sub(size.ceil() as u32) / 2;
            painter.fill_rounded_rect(
                x,
                rect.y + pad,
                (size / 12.0).round().max(1.0) as u32,
                rect.height.saturating_sub(pad * 2),
                0,
                colors.text,
            );
        }
    }

    fn changed(&mut self) {
        self.editor.shape_as_needed(&mut self.fonts, false);
        self.clamp_horizontal_scroll();
        self.raster = None;
        self.changed = Instant::now();
        self.caret_on = true;
    }

    fn horizontal_scroll(&self) -> f32 {
        self.editor.with_buffer(|buffer| buffer.scroll().horizontal)
    }

    fn clamp_horizontal_scroll(&mut self) {
        self.editor.with_buffer_mut(|buffer| {
            let Some(width) = buffer.size().0 else {
                return;
            };
            let content = buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0_f32, f32::max);
            let mut scroll = buffer.scroll();
            // Cosmic-text reveals the caret but does not remove overscroll after resize/deletion.
            scroll.horizontal = scroll.horizontal.min((content - width).max(0.0));
            buffer.set_scroll(scroll);
        });
    }

    fn caret_x(&self) -> Option<f32> {
        self.editor
            .cursor_position()
            .map(|(x, _)| x as f32 - self.horizontal_scroll())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> LineEdit {
        LineEdit::new(
            include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf"),
            "Roboto",
        )
    }

    #[test]
    fn password_mask_tracks_unicode_editing_without_exposing_clipboard_text() {
        let mut e = LineEdit::password(
            include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf"),
            "Roboto",
        );
        e.set_text("a\u{00e9}\u{1f512}z", false);
        e.layout(
            Rect {
                x: 0,
                y: 0,
                width: 300,
                height: 32,
            },
            14.0,
        );
        assert_eq!(e.masked.as_ref().unwrap().text(), "****");
        e.key(0xff51, false, false);
        e.key(0xff08, false, false);
        assert_eq!(e.text(), "a\u{00e9}z");
        e.key(0x61, true, false);
        assert_eq!(e.selected_text(), None);
        e.insert("replacement", |_| true);
        assert_eq!(e.text(), "replacement");
        e.layout(
            Rect {
                x: 0,
                y: 0,
                width: 300,
                height: 32,
            },
            14.0,
        );
        assert_eq!(e.masked.as_ref().unwrap().text(), "***********");
        e.click(0.0, false);
        e.insert("x", |_| true);
        assert_eq!(e.text(), "xreplacement");
    }

    #[test]
    fn resize_and_shorter_text_remove_overscroll_without_moving_caret() {
        let mut e = editor();
        let small = Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 32,
        };
        e.layout(small, 14.0);
        e.set_text("Ash Prime Neuroptics Blueprint", false);
        assert!(e.horizontal_scroll() > 0.0);
        let cursor = e.editor.cursor();
        e.layout(
            Rect {
                width: 400,
                ..small
            },
            14.0,
        );
        assert_eq!(e.horizontal_scroll(), 0.0);
        assert_eq!(e.editor.cursor(), cursor);
        e.layout(small, 14.0);
        assert!(e.horizontal_scroll() > 0.0);
        e.set_text("Ash", false);
        assert_eq!(e.horizontal_scroll(), 0.0);
        e.layout(Rect { x: 200, ..small }, 14.0);
        assert_eq!(e.horizontal_scroll(), 0.0);
    }
    #[test]
    fn caret_deadline_tracks_edit_phase_and_missed_blinks() {
        let mut editor = editor();
        let start = Instant::now();
        editor.changed = start;
        assert_eq!(editor.next_blink(start), start + Duration::from_millis(500));
        let late = start + Duration::from_millis(750);
        assert_eq!(editor.next_blink(late), late);
        editor.caret_on = false;
        assert_eq!(editor.next_blink(late), start + Duration::from_secs(1));
        let late = start + Duration::from_millis(1250);
        assert_eq!(editor.next_blink(late), late);
    }
    #[test]
    fn insertion_selection_and_directional_delete() {
        let mut e = editor();
        e.set_text("1234", false);
        e.key(0xff51, false, false);
        e.insert("5", |_| true);
        assert_eq!(e.text(), "12354");
        e.key(0xff51, false, true);
        assert_eq!(e.selected_text().as_deref(), Some("5"));
        e.key(0xffff, false, false);
        assert_eq!(e.text(), "1234");
        e.key(0xff08, false, false);
        assert_eq!(e.text(), "124");
        e.key(0xff50, false, false);
        e.key(0xffff, false, false);
        assert_eq!(e.text(), "24");
    }
    #[test]
    fn rejected_paste_preserves_selection_and_value() {
        let mut e = editor();
        e.set_text("15", true);
        assert!(!e.insert("not a number", |s| s.parse::<f64>().is_ok()));
        assert!(!e.insert("5\n6", |_| true));
        assert_eq!(e.selected_text().as_deref(), Some("15"));
        assert!(e.insert("-12.5", |s| s.parse::<f64>().is_ok()));
        assert_eq!(e.text(), "-12.5");
    }
    #[test]
    fn collapsed_editor_does_not_create_an_empty_native_image() {
        let mut e = editor();
        let mut pixels = vec![0; 16 * 16 * 4];
        let mut p = Painter::new(&mut pixels, 16, 16).unwrap();
        e.paint(
            &mut p,
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 18,
            },
            12.0,
            Colors {
                text: [255; 4],
                selection: [255; 4],
            },
        );
        p.finish().unwrap();
        assert!(pixels.iter().all(|v| *v == 0));
    }

    #[test]
    fn fractional_scale_hit_testing_and_overflow() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut e = editor();
            e.set_text("1234567890123456", false);
            let mut pixels = vec![0; 300 * 100 * 4];
            let mut p = Painter::new(&mut pixels, 300, 100).unwrap();
            let rect = Rect {
                x: 4,
                y: 4,
                width: (50.0 * scale) as u32,
                height: (18.0 * scale) as u32,
            };
            let colors = Colors {
                text: [255; 4],
                selection: [120, 60, 40, 255],
            };
            e.paint(&mut p, rect, 12.0 * scale, colors);
            assert!(e.caret_x().unwrap() <= rect.width as f32);
            e.key(0xff50, false, false);
            e.paint(&mut p, rect, 12.0 * scale, colors);
            e.click(0.0, false);
            assert_eq!(e.editor.cursor().index, 0);
            e.drag(rect.width as f32 / 2.0);
            assert!(e.selected_text().is_some());
            p.finish().unwrap();
            assert!(pixels.iter().any(|v| *v != 0));
        }
    }
}
