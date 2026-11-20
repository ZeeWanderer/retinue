use super::{LineEdit, Rect};
use parley::editing::Selection;

pub(super) struct Original {
    pub text: String,
    selection: Selection,
}

impl LineEdit {
    /// Update only the displayed draft. Platform cursor positions are UTF-8 bytes.
    pub fn preedit(&mut self, draft: &str, cursor: Option<(usize, usize)>) -> bool {
        if draft.is_empty() {
            return self.cancel_preedit();
        }
        if self.is_password()
            || draft.chars().any(char::is_control)
            || cursor.is_some_and(|(a, b)| !draft.is_char_boundary(a) || !draft.is_char_boundary(b))
        {
            return false;
        }
        self.composition.get_or_insert_with(|| {
            Box::new(Original {
                text: self.editor.raw_text().to_owned(),
                selection: *self.editor.raw_selection(),
            })
        });
        self.edit(|driver| driver.set_compose(draft, cursor));
        self.changed(true);
        true
    }

    pub fn cancel_preedit(&mut self) -> bool {
        let Some(original) = self.composition.take() else {
            return false;
        };
        self.edit(|driver| driver.clear_compose());
        self.editor.set_text(&original.text);
        self.edit(|driver| driver.set_selection(original.selection));
        self.changed(true);
        true
    }

    /// Window-relative exclusion rectangle for the platform candidate popup.
    /// The field and returned rectangle are both in device pixels.
    pub fn ime_area(&self, field: Rect) -> Option<Rect> {
        if self.is_password() || field.is_empty() {
            return None;
        }
        if let Some((previous, area)) = self.ime_bounds.get()
            && previous == field
        {
            return Some(area);
        }
        let layout = self.editor.try_layout()?;
        let area = self.editor.ime_cursor_area();
        let y = (field.height as f32 - layout.height() * self.scale) * 0.5;
        let x = |v: f64| ((v as f32 - self.scroll) * self.scale).clamp(0.0, field.width as f32);
        let y = |v: f64| (v as f32 * self.scale + y).clamp(0.0, field.height as f32);
        let left = (x(area.x0).floor() as u32).min(field.width - 1);
        let top = (y(area.y0).floor() as u32).min(field.height - 1);
        let area = Rect {
            x: field.x + left,
            y: field.y + top,
            width: (x(area.x1).ceil() as u32).saturating_sub(left).max(1),
            height: (y(area.y1).ceil() as u32).saturating_sub(top).max(1),
        };
        self.ime_bounds.set(Some((field, area)));
        Some(area)
    }
}
