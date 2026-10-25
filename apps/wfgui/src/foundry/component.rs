use super::*;
use retinue_ui::{painter::wrap_text, popover::Popover};

#[derive(Default)]
pub(super) struct ComponentPanel {
    pub popup: Popover,
    target: Option<(String, String, usize)>,
}

impl Foundry {
    pub fn context_menu(&mut self, p: (f64, f64)) -> bool {
        if let Some(Action::Component(row, part) | Action::ComponentRelic(row, part)) = self.hit(p)
        {
            self.open_component(row, part);
            return true;
        }
        false
    }
    pub(super) fn component_market(&mut self, row: usize, part: usize) -> bool {
        let component = &self.snapshot.items[row].components[part];
        let Some(item) = component.market_name.as_ref().filter(|s| !s.is_empty()) else {
            return false;
        };
        self.destination = Some(Destination::Market {
            item: item.clone(),
            buyers: component.required > 0 && component.owned >= component.required,
        });
        self.close_component();
        true
    }
    pub(super) fn component_relics(&mut self, row: usize, part: usize) {
        let c = &self.snapshot.items[row].components[part];
        self.destination = Some(Destination::Relics(
            c.market_name.clone().unwrap_or_else(|| c.name.clone()),
        ));
        self.close_component();
    }
    fn component_actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::ComponentFavorite, Action::ComponentSearch];
        if let Some((row, part)) = self.component_target() {
            let c = &self.snapshot.items[row].components[part];
            if c.market_name.as_ref().is_some_and(|s| !s.is_empty()) {
                actions.push(Action::ComponentMarket);
            }
            if c.relic_drop || c.owned_relic {
                actions.push(Action::ComponentRelics);
            }
        }
        actions
    }
    pub fn open_first_component(&mut self) -> bool {
        let first = self.item_controls().iter().find_map(|(a, _)| {
            if let Action::Component(row, part) = a {
                Some((*row, *part))
            } else {
                None
            }
        });
        if let Some((row, part)) = first {
            self.open_component(row, part);
            true
        } else {
            false
        }
    }
    pub fn item_controls(&self) -> Vec<(Action, Rect)> {
        self.visible(0)
            .flat_map(|i| {
                let row = self.filtered[i];
                let c = &self.snapshot.items[row];
                let card = self.card_rect(i);
                std::iter::once((Action::Favorite(row), self.title_rects(c).0))
                    .chain(
                        self.parts(c.components.len())
                            .into_iter()
                            .enumerate()
                            .flat_map(move |(part, rect)| {
                                std::iter::once((Action::Component(row, part), rect)).chain(
                                    c.components[part].owned_relic.then_some((
                                        Action::ComponentRelic(row, part),
                                        self.relic_badge(rect),
                                    )),
                                )
                            }),
                    )
                    .filter_map(move |(action, r)| {
                        let y = card.y as i64 + r.y as i64 - self.offset as i64;
                        (y >= self.viewport.y as i64
                            && y + r.height as i64
                                <= (self.viewport.y + self.viewport.height) as i64)
                            .then_some((
                                action,
                                Rect {
                                    x: card.x + r.x,
                                    y: y.max(0) as u32,
                                    ..r
                                },
                            ))
                    })
            })
            .collect()
    }

    pub fn popup_open(&self) -> bool {
        self.filters_open() || self.component_open()
    }

    pub fn close_popup(&mut self) {
        self.close_filters();
        self.close_component();
    }

    pub(super) fn component_open(&self) -> bool {
        self.component_panel.popup.is_open()
    }

    fn component_target(&self) -> Option<(usize, usize)> {
        let (parent, part, occurrence) = self.component_panel.target.as_ref()?;
        let row = self.snapshot.items.iter().position(|c| &c.id == parent)?;
        let part = self.snapshot.items[row]
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| &c.id == part)
            .nth(*occurrence)?
            .0;
        Some((row, part))
    }

    pub(super) fn open_component(&mut self, row: usize, part: usize) {
        let Some(c) = self.snapshot.items.get(row) else {
            return;
        };
        let Some(p) = c.components.get(part) else {
            return;
        };
        let occurrence = c.components[..part].iter().filter(|c| c.id == p.id).count();
        self.component_panel.target = Some((c.id.clone(), p.id.clone(), occurrence));
        self.close_filters();
        self.cancel_pointer();
        self.component_panel.popup.open();
        self.focus_action(Some(Action::ComponentFavorite));
        self.layout_component();
    }

    pub(super) fn close_component(&mut self) {
        if self.component_open() {
            self.component_panel.popup.close();
            self.cancel_pointer();
            let action = self
                .component_target()
                .map(|(r, p)| Action::Component(r, p));
            let visible = action.filter(|a| self.item_controls().iter().any(|(item, _)| item == a));
            self.focus_action(visible);
        }
    }

    pub(super) fn remap_item_focus(&mut self, old: &Snapshot) {
        let (row, part) = match self.focus {
            Some(Action::Favorite(r)) => (r, None),
            Some(Action::Component(r, p)) => (r, Some(p)),
            _ => return,
        };
        let action = old.items.get(row).and_then(|previous| {
            let row = self
                .snapshot
                .items
                .iter()
                .position(|c| c.id == previous.id)?;
            match part {
                None => Some(Action::Favorite(row)),
                Some(part) => {
                    let id = &previous.components.get(part)?.id;
                    let occurrence = previous.components[..part]
                        .iter()
                        .filter(|p| &p.id == id)
                        .count();
                    self.snapshot.items[row]
                        .components
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| &p.id == id)
                        .nth(occurrence)
                        .map(|(part, _)| Action::Component(row, part))
                }
            }
        });
        self.focus_action(
            action.filter(|a| self.item_controls().iter().any(|(item, _)| item == a)),
        );
    }

    pub(super) fn layout_component(&mut self) {
        if !self.component_open() {
            return;
        }
        let Some((row, part)) = self.component_target() else {
            self.close_component();
            return;
        };
        let Some(index) = self.filtered.iter().position(|i| *i == row) else {
            self.close_component();
            return;
        };
        let Some(mut anchor) = self
            .parts(self.snapshot.items[row].components.len())
            .get(part)
            .copied()
        else {
            self.close_component();
            return;
        };
        let card = self.card_rect(index);
        anchor.x += card.x;
        anchor.y = (anchor.y + card.y).saturating_sub(self.offset);
        let width = self
            .px(336.0)
            .min(self.size.0.saturating_sub(self.px(16.0)));
        let component = &self.snapshot.items[row].components[part];
        let name = &component.name;
        let metadata_rows =
            1 + u32::from(component.blueprint_owned > 0) + u32::from(component.owned_relic);
        let lines = wrap_text(
            &self.regular,
            name,
            17.0 * self.scale,
            width.saturating_sub(self.px(56.0)) as f32,
        )
        .len() as u32;
        self.component_panel.popup.layout(
            anchor,
            Rect {
                x: self.px(8.0),
                y: self.px(8.0),
                width: self.size.0.saturating_sub(self.px(16.0)),
                height: self.size.1.saturating_sub(self.px(16.0)),
            },
            (
                width,
                self.px(29.0 + self.component_actions().len() as f32 * 38.0)
                    + (lines + metadata_rows) * self.px(23.0),
            ),
            self.px(8.0),
        );
    }

    pub(super) fn component_controls(&self) -> Vec<(Action, Rect)> {
        let r = self.component_panel.popup.bounds;
        let mut controls = vec![(
            Action::CloseComponent,
            Rect {
                x: r.x + r.width.saturating_sub(self.px(36.0)),
                y: r.y + self.px(4.0),
                width: self.px(32.0),
                height: self.px(32.0),
            },
        )];
        let actions = self.component_actions();
        controls.extend(actions.iter().enumerate().map(|(i, action)| {
            (
                *action,
                Rect {
                    x: r.x + self.px(12.0),
                    y: r.y
                        + r.height
                            .saturating_sub(self.px(6.0 + (actions.len() - i) as f32 * 38.0)),
                    width: r.width.saturating_sub(self.px(24.0)),
                    height: self.px(32.0),
                },
            )
        }));
        controls
    }

    pub(super) fn component_label(&self, action: Action) -> String {
        match action {
            Action::CloseComponent => "Close component details".into(),
            Action::ComponentFavorite => {
                if self.component_target().is_some_and(|(r, p)| {
                    self.preferences
                        .is_favorite(&self.snapshot.items[r].components[p].id)
                }) {
                    "Remove from favorites".into()
                } else {
                    "Add to favorites".into()
                }
            }
            Action::ComponentSearch => "Search Foundry for this part".into(),
            Action::ComponentMarket => "View market listings".into(),
            Action::ComponentRelics => "Show in Relic Planner".into(),
            _ => String::new(),
        }
    }

    pub(super) fn component_summary(&self) -> String {
        let Some((r, p)) = self.component_target() else {
            return String::new();
        };
        let c = &self.snapshot.items[r].components[p];
        format!(
            "{}: {} owned, {} required. {} blueprints owned. {}",
            c.name,
            c.owned,
            c.required,
            c.blueprint_owned,
            if c.owned_relic {
                "In owned relics."
            } else {
                ""
            }
        )
    }

    pub(super) fn component_activate(&mut self, action: Action) {
        let Some((r, p)) = self.component_target() else {
            return;
        };
        match action {
            Action::ComponentMarket => {
                self.component_market(r, p);
            }
            Action::ComponentRelics => self.component_relics(r, p),
            Action::CloseComponent => self.close_component(),
            Action::ComponentFavorite => {
                self.preferences
                    .toggle_favorite(&self.snapshot.items[r].components[p].id);
                let anchor = self.anchor();
                self.filter(false);
                self.restore_anchor(anchor);
                self.layout_component();
            }
            Action::ComponentSearch => {
                let name = self.snapshot.items[r].components[p].name.clone();
                self.close_component();
                self.filters = Filters::default();
                self.set_category(0);
                self.set_search(&name);
                self.focus_action(Some(Action::Search));
                self.search.borrow_mut().key(0x61, true, false);
            }
            _ => (),
        }
    }

    pub(super) fn component_key(&mut self, key: InputKey<&str>, modifiers: ModifiersState) {
        match key {
            InputKey::Named(NamedKey::Escape) => self.close_component(),
            InputKey::Character("f") if modifiers.control_key() => {
                self.close_component();
                self.focus_action(Some(Action::Search));
                self.search.borrow_mut().key(0x61, true, false);
            }
            InputKey::Named(NamedKey::Tab | NamedKey::ArrowUp | NamedKey::ArrowDown) => {
                let controls = self.component_controls();
                let previous = key == InputKey::Named(NamedKey::ArrowUp)
                    || (key == InputKey::Named(NamedKey::Tab) && modifiers.shift_key());
                let i = controls
                    .iter()
                    .position(|(a, _)| Some(*a) == self.focus)
                    .unwrap_or(0);
                let next = if previous {
                    (i + controls.len() - 1) % controls.len()
                } else {
                    (i + 1) % controls.len()
                };
                self.focus_action(Some(controls[next].0));
            }
            InputKey::Named(NamedKey::Enter | NamedKey::Space) | InputKey::Character(" ") => {
                if let Some(action) = self.focus {
                    self.component_activate(action);
                }
            }
            _ => (),
        }
    }

    pub(super) fn paint_component(
        &self,
        p: &mut Painter<'_>,
        focus: retinue_ui::focus_visible::FocusVisible,
    ) {
        let Some((row, part)) = self.component_target() else {
            return;
        };
        let c = &self.snapshot.items[row].components[part];
        let r = self.component_panel.popup.bounds;
        p.fill_rounded_rect(
            r.x,
            r.y,
            r.width,
            r.height,
            self.px(8.0),
            crate::style::POPOVER,
        );
        let text_width = r.width.saturating_sub(self.px(56.0));
        let lines = wrap_text(&self.regular, &c.name, 17.0 * self.scale, text_width as f32);
        for (i, line) in lines.iter().enumerate() {
            self.text(
                p,
                line,
                Rect {
                    x: r.x + self.px(14.0),
                    y: r.y + self.px(10.0) + i as u32 * self.px(23.0),
                    width: text_width,
                    height: self.px(23.0),
                },
                17.0,
                TEXT,
            );
        }
        let rows = [
            format!("{} owned / {} required", c.owned, c.required),
            if c.blueprint_owned > 0 {
                format!("{} blueprints owned", c.blueprint_owned)
            } else {
                String::new()
            },
            if c.owned_relic {
                "In owned relics".into()
            } else {
                String::new()
            },
        ];
        for (i, text) in rows.iter().filter(|s| !s.is_empty()).enumerate() {
            self.text(
                p,
                text,
                Rect {
                    x: r.x + self.px(14.0),
                    y: r.y + self.px(16.0) + (lines.len() as u32 + i as u32) * self.px(23.0),
                    width: r.width.saturating_sub(self.px(28.0)),
                    height: self.px(23.0),
                },
                14.0,
                MUTED,
            );
        }
        for (action, r) in self.component_controls() {
            if action == Action::CloseComponent {
                self.icon(p, Badge::Close, r);
            } else {
                p.fill_rounded_rect(
                    r.x,
                    r.y,
                    r.width,
                    r.height,
                    self.px(6.0),
                    if r.contains(self.pointer.0, self.pointer.1) {
                        crate::style::MENU_HOVER
                    } else {
                        CARD
                    },
                );
                self.text(
                    p,
                    &self.component_label(action),
                    Rect {
                        x: r.x + self.px(10.0),
                        ..r
                    },
                    14.0,
                    TEXT,
                );
            }
            if focus.shows(self.focus == Some(action), false) {
                crate::style::focus_ring(p, r, self.scale);
            }
        }
    }
}
