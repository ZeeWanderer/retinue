use super::*;
use retinue_ui::{
    desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
    desktop::winit::keyboard::NamedKey,
};

pub(super) fn button(ui: &mut ControlHost, rect: Rect, count: usize, open: bool) {
    ui.fill(
        rect,
        16.0,
        crate::presentation::style::filter_color(
            count > 0 || open,
            ui.hover.as_deref() == Some("list:filters"),
        ),
    );
    let pad = ui.px(6.0);
    ui.icon(
        Rect {
            x: rect.x + pad,
            y: rect.y + pad,
            width: rect.width.saturating_sub(2 * pad),
            height: rect.height.saturating_sub(2 * pad),
        },
        "filter",
    );
    ui.hit(
        "list:filters",
        rect,
        if count == 0 {
            "Filters".into()
        } else {
            format!("Filters ({count} active)")
        },
        true,
    );
}

impl Listing {
    pub(super) fn filter_groups(
        &self,
    ) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        let mut groups = vec![
            ("prime", "Type", "Prime", "Normal"),
            ("mastered", "Mastery", "Mastered", "Unmastered"),
            ("vaulted", "Vaulted", "Yes", "No"),
        ];
        if self.page == Page::Inventory {
            groups.extend([
                ("duplicates", "More than one copy", "Yes", "No"),
                ("complete", "Set completion", "Complete", "Incomplete"),
            ]);
        }
        groups
    }

    fn filter_value(&mut self, id: &str) -> Option<&mut String> {
        match id {
            "prime" => Some(&mut self.prime),
            "mastered" => Some(&mut self.mastered),
            "vaulted" => Some(&mut self.vaulted),
            "duplicates" => Some(&mut self.duplicates),
            "complete" => Some(&mut self.complete),
            _ => None,
        }
    }

    pub(super) fn filter_count(&self) -> usize {
        [
            &self.prime,
            &self.mastered,
            &self.vaulted,
            &self.duplicates,
            &self.complete,
        ]
        .into_iter()
        .filter(|v| v.as_str() != "all")
        .count()
    }
}

impl Pages {
    pub(in crate::presentation) fn listing_filters_open(&self) -> bool {
        matches!(self.page, Page::Inventory | Page::Mastery) && self.listing().filters.is_open()
    }

    pub(in crate::presentation) fn close_listing_filters(&mut self) {
        if self.listing_filters_open() {
            self.listing_mut().filters.close();
            self.ui.cancel_activation();
            self.ui.focus = Some("list:filters".into());
            self.dirty = true;
        }
    }

    pub(in crate::presentation) fn dismiss_listing_filters(&mut self, point: (f64, f64)) -> bool {
        if self.listing_filters_open() && !self.listing().filters.bounds.contains(point.0, point.1)
        {
            self.close_listing_filters();
            self.layout();
            return true;
        }
        false
    }

    pub(in crate::presentation) fn listing_filter_signal(&mut self, signal: &Signal) -> bool {
        match signal.key.as_str() {
            "list:filters" => {
                if self.listing_filters_open() {
                    self.close_listing_filters();
                } else {
                    self.ui.cancel_activation();
                    self.listing_mut().menu = None;
                    self.listing_mut().filters.open();
                    self.ui.focus = Some("list:filter:prime:yes".into());
                }
            }
            "list:filters:close" => self.close_listing_filters(),
            "list:filters:scroll" => {
                if let Some(value) = signal.value.as_ref().and_then(|v| v.parse().ok()) {
                    self.listing_mut()
                        .filter_scroll
                        .to(value, false, Instant::now());
                    self.listing_mut().tick(Instant::now());
                }
            }
            "list:filters:clear" => {
                let listing = self.listing_mut();
                for (id, ..) in listing.filter_groups() {
                    *listing.filter_value(id).unwrap() = "all".into();
                }
                self.resort(true);
                self.start_prices(false);
            }
            key => {
                let Some((id, value)) = key
                    .strip_prefix("list:filter:")
                    .and_then(|v| v.split_once(':'))
                else {
                    return false;
                };
                let Some(current) = self.listing_mut().filter_value(id) else {
                    return false;
                };
                *current = if *current == value { "all" } else { value }.into();
                self.resort(true);
                self.start_prices(false);
            }
        }
        self.dirty = true;
        true
    }

    pub(in crate::presentation) fn listing_filter_key(
        &mut self,
        key: &Key<&str>,
        mods: ModifiersState,
    ) -> bool {
        if !self.listing_filters_open() {
            return false;
        }
        if *key == Key::Named(NamedKey::Escape) {
            self.close_listing_filters();
            self.layout();
            return true;
        }
        let delta = match key {
            Key::Named(NamedKey::Tab) => {
                if mods.shift_key() {
                    -1
                } else {
                    1
                }
            }
            Key::Named(NamedKey::ArrowLeft) => -1,
            Key::Named(NamedKey::ArrowRight) => 1,
            Key::Named(NamedKey::ArrowUp) => -2,
            Key::Named(NamedKey::ArrowDown) => 2,
            _ => return false,
        };
        let mut keys = vec!["list:filters:close".to_owned()];
        for (id, ..) in self.listing().filter_groups() {
            keys.extend([
                format!("list:filter:{id}:yes"),
                format!("list:filter:{id}:no"),
            ]);
        }
        if self.listing().filter_count() > 0 {
            keys.push("list:filters:clear".into());
        }
        let current = keys
            .iter()
            .position(|k| self.ui.focus.as_ref() == Some(k))
            .unwrap_or(0);
        let next = (current as i32 + delta).rem_euclid(keys.len() as i32) as usize;
        self.ui.focus_visible.keyboard_input();
        self.ui.focus = Some(keys[next].clone());
        if keys[next].starts_with("list:filter:") {
            let top = self.ui.px(((next - 1) / 2) as f32 * 64.0) as f32;
            let bottom = top + self.ui.px(56.0) as f32;
            let height = self.ui.bounds("page.filters.content").unwrap().height as f32;
            self.listing_mut()
                .filter_scroll
                .reveal(top, bottom, height, Instant::now());
            self.listing_mut().tick(Instant::now());
        }
        self.dirty = true;
        self.layout();
        true
    }

    pub(in crate::presentation) fn listing_filter_wheel(&mut self, delta: f32, smooth: bool) {
        if self
            .listing()
            .filters
            .bounds
            .contains(self.ui.pointer().0, self.ui.pointer().1)
        {
            self.listing_mut()
                .filter_scroll
                .by(delta, smooth, Instant::now());
        }
    }
}
