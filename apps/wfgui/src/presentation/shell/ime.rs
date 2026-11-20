use super::*;
use retinue_ui::desktop::winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::Ime,
    window::Window,
};

#[derive(Default)]
pub(super) struct Session {
    target: Option<(Page, bool, String)>,
    area: Option<Rect>,
    enabled: bool,
}

impl Shell {
    fn ime_field(&self) -> Option<(&str, Rect)> {
        if !self.window_focused || matches!(self.focus, Some(Focus::Chrome(_))) {
            return None;
        }
        let (name, mut rect) = if self.rail_focus {
            if !self.right_available || self.right_collapsed {
                return None;
            }
            self.pages.rail.ime_field()?
        } else {
            match self.page {
                Page::Settings => return None,
                Page::Foundry => ("", self.foundry.ime_area()?),
                _ => self.pages.ui.ime_field()?,
            }
        };
        let origin = if self.rail_focus {
            self.geometry.right
        } else {
            self.geometry.center
        };
        rect.x += origin.x;
        rect.y += origin.y;
        Some((name, rect))
    }

    fn deliver_ime(&mut self, target: &(Page, bool, String), event: &Ime) -> bool {
        if !target.1 && target.0 == Page::Foundry {
            return self.foundry.ime(event);
        }
        let controls = if target.1 {
            &mut self.pages.rail
        } else {
            &mut self.pages.ui
        };
        if !controls.ime(&target.2, event) {
            return false;
        }
        self.pages.dirty = true;
        self.pages.signals();
        self.pages.layout();
        true
    }

    fn same_field(&self, name: Option<&str>) -> bool {
        match &self.ime.target {
            Some((page, rail, id)) => {
                self.page == *page && self.rail_focus == *rail && Some(id.as_str()) == name
            }
            None => name.is_none(),
        }
    }

    pub fn sync_input_method(&mut self, window: &Window) -> Damage {
        let field = self.ime_field();
        let area = field.map(|(_, rect)| rect);
        let mut damage = Damage::None;
        if !self.same_field(field.map(|(name, _)| name)) {
            let next = field.map(|(key, _)| (self.page, self.rail_focus, key.to_owned()));
            if let Some(old) = self.ime.target.take() {
                window.set_ime_allowed(false);
                if self.deliver_ime(&old, &Ime::Disabled) {
                    damage = Damage::Full;
                }
            }
            self.ime.enabled = false;
            self.ime.area = None;
            self.ime.target = next;
            if self.ime.target.is_some() {
                window.set_ime_allowed(true);
            }
        }
        if area != self.ime.area {
            if let Some(r) = area {
                window.set_ime_cursor_area(
                    PhysicalPosition::new(r.x, r.y),
                    PhysicalSize::new(r.width, r.height),
                );
            }
            self.ime.area = area;
        }
        damage
    }

    pub(super) fn input_method_event(&mut self, event: &Ime) -> Damage {
        if !self.same_field(self.ime_field().map(|(name, _)| name)) {
            return Damage::None;
        }
        if matches!(event, Ime::Enabled | Ime::Disabled) {
            self.ime.enabled = matches!(event, Ime::Enabled);
        } else if !self.ime.enabled {
            return Damage::None;
        }
        let Some(target) = self.ime.target.take() else {
            return Damage::None;
        };
        let changed = self.deliver_ime(&target, event);
        self.ime.target = Some(target);
        if changed { Damage::Full } else { Damage::None }
    }
}
