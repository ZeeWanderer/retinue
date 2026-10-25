use super::*;
use retinue_ui::popover::Popover;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Prime,
    Mastered,
    Owned,
    Vaulted,
    Ready,
    Mastery,
    Crafting,
    Favorite,
    Shards,
    Incarnon,
}

impl Filter {
    pub const ALL: [Self; 10] = [
        Self::Prime,
        Self::Mastered,
        Self::Owned,
        Self::Vaulted,
        Self::Ready,
        Self::Mastery,
        Self::Crafting,
        Self::Favorite,
        Self::Shards,
        Self::Incarnon,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Prime => "Type",
            Self::Mastered => "Mastery",
            Self::Owned => "Owned",
            Self::Vaulted => "Vaulted",
            Self::Ready => "Ready to build",
            Self::Mastery => "Enough mastery",
            Self::Crafting => "Used for crafting",
            Self::Favorite => "Favorite",
            Self::Shards => "Contains archon shards",
            Self::Incarnon => "Incarnon",
        }
    }

    pub fn label(self, value: bool) -> &'static str {
        match (self, value) {
            (Self::Prime, true) => "Prime",
            (Self::Prime, false) => "Normal",
            (Self::Mastered, true) => "Mastered",
            (Self::Mastered, false) => "Unmastered",
            (_, true) => "Yes",
            (_, false) => "No",
        }
    }
}

#[derive(Default)]
pub struct Filters([Option<bool>; Filter::ALL.len()]);

impl Filters {
    pub fn get(&self, filter: Filter) -> Option<bool> {
        self.0[filter as usize]
    }

    pub fn toggle(&mut self, filter: Filter, value: bool) {
        let current = &mut self.0[filter as usize];
        *current = (Some(value) != *current).then_some(value);
    }

    pub fn count(&self) -> usize {
        self.0.iter().flatten().count()
    }

    pub fn matches(
        &self,
        card: &Card,
        rank: Option<u64>,
        prefs: &crate::preferences::Preferences,
    ) -> bool {
        Filter::ALL.into_iter().all(|filter| {
            let Some(expected) = self.get(filter) else {
                return true;
            };
            let actual = match filter {
                Filter::Prime => Some(card.is_prime),
                Filter::Mastered => Some(card.mastered),
                Filter::Owned => Some(card.owned),
                Filter::Vaulted => card.is_prime.then_some(card.vaulted),
                Filter::Ready => Some(card.ready_to_build),
                Filter::Mastery => rank.map(|rank| rank >= card.mastery_requirement),
                Filter::Crafting => Some(card.used_for_crafting),
                Filter::Favorite => Some(
                    prefs.is_favorite(&card.id)
                        || card
                            .components
                            .iter()
                            .any(|part| prefs.is_favorite(&part.id)),
                ),
                Filter::Shards => Some(card.has_archon_shards),
                Filter::Incarnon => Some(card.has_incarnon),
            };
            actual == Some(expected)
        })
    }
}

#[derive(Default)]
pub(super) struct FilterPanel {
    pub popup: Popover,
    pub scroll: Scroll,
    pub offset: u32,
    pub drag: Option<f64>,
}

impl Foundry {
    pub fn filters_open(&self) -> bool {
        self.filter_panel.popup.is_open()
    }

    pub fn open_filters(&mut self) {
        self.close_component();
        self.cancel_pointer();
        self.filter_panel.popup.open();
        self.layout_filters();
        self.focus_filter(Action::FilterChoice(Filter::Prime, true));
    }

    pub fn close_filters(&mut self) {
        if self.filters_open() {
            self.filter_panel.popup.close();
            self.cancel_pointer();
            self.focus = Some(Action::Filters);
        }
    }

    pub(super) fn layout_filters(&mut self) {
        let anchor = self
            .controls()
            .into_iter()
            .find(|(a, _)| *a == Action::Filters)
            .map(|(_, r)| r)
            .unwrap_or_default();
        let margin = self.px(8.0);
        self.filter_panel.popup.layout(
            anchor,
            Rect {
                x: margin,
                y: margin,
                width: self.size.0.saturating_sub(2 * margin),
                height: self.size.1.saturating_sub(2 * margin),
            },
            (self.px(276.0), self.px(500.0)),
            self.px(8.0),
        );
        self.filter_panel.scroll.resize(
            (Filter::ALL.len() as u32 * self.px(64.0)) as f32,
            self.filter_viewport().height as f32,
            Instant::now(),
        );
        self.filter_panel.offset = self.filter_panel.scroll.offset(Instant::now()).round() as u32;
    }

    pub(super) fn filter_viewport(&self) -> Rect {
        let r = self.filter_panel.popup.bounds;
        Rect {
            x: r.x + self.px(12.0),
            y: r.y + self.px(40.0),
            width: r.width.saturating_sub(self.px(32.0)),
            height: r.height.saturating_sub(self.px(88.0)),
        }
    }

    pub(super) fn filter_choice_rect(&self, filter: Filter, value: bool) -> Rect {
        let width = self.filter_viewport().width.saturating_sub(self.px(10.0)) / 2;
        Rect {
            x: if value { 0 } else { width + self.px(10.0) },
            y: filter as u32 * self.px(64.0) + self.px(23.0),
            width,
            height: self.px(32.0),
        }
    }

    pub(super) fn filter_enabled(&self, action: Action) -> bool {
        match action {
            Action::FilterChoice(Filter::Mastery, _) => {
                self.snapshot.profile["player_level"].as_u64().is_some()
            }
            Action::ClearFilters => self.filters.count() != 0,
            _ => true,
        }
    }

    pub(super) fn filter_scrollbar(&self) -> Rect {
        let v = self.filter_viewport();
        let maximum = self.filter_panel.scroll.maximum();
        let height = (v.height as f32 * v.height as f32 / (maximum + v.height as f32).max(1.0))
            .round() as u32;
        let height = height.max(self.px(24.0)).min(v.height);
        Rect {
            x: v.x + v.width + self.px(7.0),
            y: v.y
                + ((v.height - height) as f32 * self.filter_panel.offset as f32 / maximum.max(1.0))
                    .round() as u32,
            width: self.px(4.0),
            height,
        }
    }

    pub(super) fn filter_controls(&self) -> Vec<(Action, Rect)> {
        let r = self.filter_panel.popup.bounds;
        let v = self.filter_viewport();
        let mut controls = vec![(
            Action::CloseFilters,
            Rect {
                x: r.x + r.width.saturating_sub(self.px(36.0)),
                y: r.y + self.px(4.0),
                width: self.px(32.0),
                height: self.px(32.0),
            },
        )];
        for filter in Filter::ALL {
            for value in [true, false] {
                let choice = self.filter_choice_rect(filter, value);
                let top = v.y as i64 + choice.y as i64 - self.filter_panel.offset as i64;
                let y = top.max(v.y as i64) as u32;
                let bottom = (top + choice.height as i64).min((v.y + v.height) as i64);
                controls.push((
                    Action::FilterChoice(filter, value),
                    Rect {
                        x: v.x + choice.x,
                        y,
                        width: choice.width,
                        height: (bottom - y as i64).max(0) as u32,
                    },
                ));
            }
        }
        controls.push((
            Action::ClearFilters,
            Rect {
                x: r.x + self.px(12.0),
                y: r.y + r.height.saturating_sub(self.px(42.0)),
                width: r.width.saturating_sub(self.px(24.0)),
                height: self.px(30.0),
            },
        ));
        controls
    }

    pub(super) fn filter_hit(&self, p: (f64, f64)) -> Option<Action> {
        self.filter_controls()
            .into_iter()
            .find(|(a, r)| self.filter_enabled(*a) && r.contains(p.0, p.1))
            .map(|(a, _)| a)
            .or_else(|| {
                let v = self.filter_viewport();
                (self.filter_panel.scroll.maximum() > 0.0
                    && Rect {
                        x: v.x + v.width,
                        y: v.y,
                        width: self.px(16.0),
                        height: v.height,
                    }
                    .contains(p.0, p.1))
                .then_some(Action::FilterScroll)
            })
    }

    pub(super) fn focus_filter(&mut self, action: Action) {
        self.focus = Some(action);
        if let Action::FilterChoice(filter, _) = action {
            let now = Instant::now();
            let top = filter as u32 * self.px(64.0);
            self.filter_panel.scroll.reveal(
                top as f32,
                (top + self.px(56.0)) as f32,
                self.filter_viewport().height as f32,
                now,
            );
            self.filter_panel.offset = self.filter_panel.scroll.offset(now).round() as u32;
        }
    }

    pub(super) fn filter_key(&mut self, key: InputKey<&str>, modifiers: ModifiersState) {
        match key {
            InputKey::Named(NamedKey::Escape) => self.close_filters(),
            InputKey::Character("f") if modifiers.control_key() => {
                self.close_filters();
                self.focus = Some(Action::Search);
            }
            InputKey::Named(NamedKey::Enter | NamedKey::Space) | InputKey::Character(" ") => {
                if let Some(action) = self.focus.filter(|a| self.filter_enabled(*a)) {
                    self.activate(action);
                }
            }
            InputKey::Named(key) => {
                let order: Vec<_> = self
                    .filter_controls()
                    .into_iter()
                    .map(|(a, _)| a)
                    .filter(|a| self.filter_enabled(*a))
                    .collect();
                let current = self
                    .focus
                    .and_then(|a| order.iter().position(|c| *c == a))
                    .unwrap_or(0);
                let next = match key {
                    NamedKey::Tab if modifiers.shift_key() => {
                        (current + order.len() - 1) % order.len()
                    }
                    NamedKey::Tab => (current + 1) % order.len(),
                    NamedKey::ArrowLeft | NamedKey::ArrowRight => {
                        if let Some(Action::FilterChoice(filter, value)) = self.focus {
                            self.focus_filter(Action::FilterChoice(filter, !value));
                        }
                        return;
                    }
                    NamedKey::ArrowUp => current.saturating_sub(2),
                    NamedKey::ArrowDown => (current + 2).min(order.len() - 1),
                    NamedKey::Home => 0,
                    NamedKey::End => order.len() - 1,
                    _ => return,
                };
                self.focus_filter(order[next]);
            }
            _ => (),
        }
    }

    pub(super) fn paint_filters(
        &self,
        p: &mut Painter<'_>,
        focus: retinue_ui::focus_visible::FocusVisible,
    ) {
        let r = self.filter_panel.popup.bounds;
        p.fill_rounded_rect(r.x, r.y, r.width, r.height, self.px(10.0), PANEL);
        p.outline(r, self.px(10.0), self.px(1.0).max(1), rgb(0x9097a8));
        self.text(
            p,
            "Filters",
            Rect {
                x: r.x + self.px(12.0),
                height: self.px(40.0),
                ..r
            },
            15.0,
            TEXT,
        );
        let hover = self.filter_hit(self.pointer);
        for (action, bounds) in self.filter_controls() {
            if matches!(action, Action::CloseFilters | Action::ClearFilters) {
                let enabled = self.filter_enabled(action);
                if hover == Some(action) || action == Action::ClearFilters {
                    p.fill_rounded_rect(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        bounds.height / 2,
                        if hover == Some(action) {
                            CARD_HOVER
                        } else {
                            CARD
                        },
                    );
                }
                if action == Action::CloseFilters {
                    self.icon(p, Badge::Close, bounds);
                } else {
                    self.filter_label(
                        p,
                        "Clear filters",
                        bounds,
                        if enabled { TEXT } else { MUTED },
                    );
                }
                if focus.shows(self.focus == Some(action), false) {
                    crate::style::focus_ring(p, bounds, self.scale);
                }
            }
        }
        let v = self.filter_viewport();
        p.viewport(
            v,
            (v.x as i32, v.y as i32 - self.filter_panel.offset as i32),
            |p| {
                for filter in Filter::ALL {
                    let enabled = self.filter_enabled(Action::FilterChoice(filter, true));
                    let title = if enabled {
                        filter.title()
                    } else {
                        "Enough mastery (unknown)"
                    };
                    let width = text_width(&self.regular, title, 15.0 * self.scale).ceil() as u32;
                    self.text(
                        p,
                        title,
                        Rect {
                            x: v.width.saturating_sub(width) / 2,
                            y: filter as u32 * self.px(64.0),
                            width: v.width,
                            height: self.px(19.0),
                        },
                        15.0,
                        if enabled { TEXT } else { MUTED },
                    );
                    for value in [true, false] {
                        let action = Action::FilterChoice(filter, value);
                        let bounds = self.filter_choice_rect(filter, value);
                        let selected = self.filters.get(filter) == Some(value);
                        let hovered = hover == Some(action);
                        p.fill_rounded_rect(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            bounds.height / 2,
                            if selected {
                                rgb(0x5923cb)
                            } else if hovered {
                                rgb(0x5e44af)
                            } else {
                                CARD
                            },
                        );
                        p.outline(
                            bounds,
                            bounds.height / 2,
                            self.px(2.0),
                            if hovered {
                                rgb(0x9297a3)
                            } else {
                                rgb(0x5e44af)
                            },
                        );
                        self.filter_label(
                            p,
                            filter.label(value),
                            bounds,
                            if enabled { TEXT } else { MUTED },
                        );
                        if focus.shows(self.focus == Some(action), false) {
                            crate::style::focus_capsule(p, bounds, self.scale);
                        }
                    }
                }
            },
        );
        if self.filter_panel.scroll.maximum() > 0.0 {
            let r = self.filter_scrollbar();
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.width / 2, rgb(0x727b92));
        }
    }

    fn filter_label(&self, p: &mut Painter<'_>, text: &str, r: Rect, color: [u8; 4]) {
        let width = text_width(&self.regular, text, 14.0 * self.scale).ceil() as u32;
        self.text(
            p,
            text,
            Rect {
                x: r.x + r.width.saturating_sub(width) / 2,
                ..r
            },
            14.0,
            color,
        );
    }
}
