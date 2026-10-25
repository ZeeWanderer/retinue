use super::*;
pub(super) use crate::search::Motion as SearchMotion;

#[derive(Clone, Copy)]
pub struct Transfer {
    stamp: Instant,
    cut: bool,
}

impl Foundry {
    pub fn query(&self) -> String {
        self.search.borrow().text()
    }

    pub(super) fn request_clipboard(&mut self, key: &str) {
        use retinue_ui::desktop::clipboard::Operation;
        self.input_error = None;
        let edit = self.search.borrow();
        let operation = if key == "v" {
            Operation::Read
        } else if let Some(text) = edit.selected_text() {
            Operation::Write(text)
        } else {
            return;
        };
        self.clipboard_request = Some((
            Transfer {
                stamp: edit.edit_stamp(),
                cut: key == "x",
            },
            operation,
        ));
    }

    pub fn clipboard_reply(&mut self, transfer: Transfer, result: Result<Option<String>, String>) {
        if self.focus != Some(Action::Search) || self.search.borrow().edit_stamp() != transfer.stamp
        {
            return;
        }
        match result {
            Err(error) => self.input_error = Some(format!("Clipboard: {error}")),
            Ok(text) => {
                let changed = if transfer.cut {
                    self.search.borrow_mut().delete_selection()
                } else if let Some(text) = text {
                    self.search
                        .borrow_mut()
                        .insert(text.trim(), |s| s.chars().count() <= 256)
                } else {
                    false
                };
                if changed {
                    self.filter(true);
                    self.layout_search(Instant::now(), true);
                }
            }
        }
    }

    pub fn focus_action(&mut self, action: Option<Action>) {
        self.focus = action;
        self.layout_search(Instant::now(), true);
    }

    pub(super) fn layout_search(&mut self, now: Instant, animate: bool) -> bool {
        let text = self.search.borrow().text();
        let active = matches!(self.focus, Some(Action::Search | Action::ClearSearch));
        let content_width = text_width(&self.regular, &text, 14.0);
        let changed =
            self.search_motion
                .update(active || !text.is_empty(), content_width, now, animate);
        if self.scale > 0.0 {
            let rect = self.search_rect();
            self.search.borrow_mut().layout(rect, 14.0 * self.scale);
        }
        changed
    }

    pub(super) fn search_bounds(&self) -> Rect {
        self.layout_controls()
            .into_iter()
            .find(|(a, _)| *a == Action::Search)
            .unwrap()
            .1
    }

    pub(super) fn search_rect(&self) -> Rect {
        let r = self.search_bounds();
        Rect {
            x: r.x + self.px(32.0),
            width: r.width.saturating_sub(self.px(64.0)),
            ..r
        }
    }

    pub(super) fn paint_search(&self, p: &mut Painter<'_>, r: Rect) {
        p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.height / 2, CARD);
        self.icon(
            p,
            Badge::Search,
            Rect {
                width: self.px(32.0),
                ..r
            },
        );
        let sr = self.search_rect();
        if sr.width == 0 {
            return;
        }
        p.viewport(sr, (0, 0), |p| {
            if self.focus == Some(Action::Search) {
                if self.search.borrow().text().is_empty() {
                    self.text(p, "Search Foundry", sr, 14.0, MUTED);
                }
                self.search.borrow_mut().paint(
                    p,
                    sr,
                    14.0 * self.scale,
                    Colors {
                        text: TEXT,
                        selection: rgb(0x694ba5),
                    },
                );
            } else {
                let text = self.search.borrow().text();
                self.text(
                    p,
                    if text.is_empty() {
                        "Search Foundry"
                    } else {
                        &text
                    },
                    sr,
                    14.0,
                    if text.is_empty() { MUTED } else { TEXT },
                );
            }
        });
    }
}
