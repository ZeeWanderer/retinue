use super::*;
use retinue_ui::popover::Popover;

#[derive(Default)]
pub(super) struct ComponentPanel {
    pub popup: Popover,
    target: Option<(String, String, usize)>,
    layout: Option<crate::menu::Layout>,
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
    pub fn keyboard_context_menu(&mut self) -> bool {
        if let Some(Action::Component(row, part) | Action::ComponentRelic(row, part)) = self.focus {
            self.open_component(row, part);
            true
        } else {
            false
        }
    }
    pub fn dismiss_popup_at(&mut self, point: (f64, f64)) -> bool {
        if self.component_open() && !self.component_panel.popup.bounds.contains(point.0, point.1) {
            self.close_component();
            return true;
        }
        if self.filters_open() && !self.filter_panel.popup.bounds.contains(point.0, point.1) {
            self.close_filters();
            self.cancel_pointer();
            self.focus_action(Some(Action::Filters));
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
            if c.relic_drop || c.owned_relic {
                actions.push(Action::ComponentRelics);
            }
            if c.market_name.as_ref().is_some_and(|s| !s.is_empty()) {
                actions.push(Action::ComponentMarket);
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
        self.resolved_composition().controls(true)
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
        let component = &self.snapshot.items[row].components[part];
        let mut details = vec![format!(
            "{} owned / {} required",
            component.owned, component.required
        )];
        if component.blueprint_owned > 0 {
            details.push(format!("{} blueprints owned", component.blueprint_owned));
        }
        if component.owned_relic {
            details.push("In owned relics".into());
        }
        let layout = crate::menu::Layout::new(
            anchor,
            Rect {
                x: self.px(8.0),
                y: self.px(8.0),
                width: self.size.0.saturating_sub(self.px(16.0)),
                height: self.size.1.saturating_sub(self.px(16.0)),
            },
            &component.name,
            &details,
            self.component_actions().len(),
            &self.regular,
            self.scale,
        );
        self.component_panel.popup.bounds = layout.bounds;
        self.component_panel.layout = Some(layout);
    }

    pub(super) fn component_controls(&self) -> Vec<(Action, Rect)> {
        let Some(layout) = &self.component_panel.layout else {
            return vec![];
        };
        self.component_actions()
            .into_iter()
            .zip(layout.rows.iter().copied())
            .collect()
    }

    pub(super) fn component_label(&self, action: Action) -> String {
        match action {
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
            Action::ComponentSearch => "Show in Foundry".into(),
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
            InputKey::Named(NamedKey::Escape | NamedKey::Tab) => self.close_component(),
            InputKey::Character("f") if modifiers.control_key() => {
                self.close_component();
                self.focus_action(Some(Action::Search));
                self.search.borrow_mut().key(0x61, true, false);
            }
            InputKey::Named(
                NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::Home | NamedKey::End,
            ) => {
                let controls = self.component_controls();
                if controls.is_empty() {
                    return;
                }
                let previous = key == InputKey::Named(NamedKey::ArrowUp);
                let i = controls
                    .iter()
                    .position(|(a, _)| Some(*a) == self.focus)
                    .unwrap_or(0);
                let next = if key == InputKey::Named(NamedKey::Home) {
                    0
                } else if key == InputKey::Named(NamedKey::End) {
                    controls.len() - 1
                } else if previous {
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
        p: &mut Canvas,
        focus: retinue_ui::focus_visible::FocusVisible,
    ) {
        let Some(layout) = &self.component_panel.layout else {
            return;
        };
        let controls = self.component_controls();
        let labels: Vec<_> = controls
            .iter()
            .map(|(a, _)| self.component_label(*a))
            .collect();
        let items: Vec<_> = labels.iter().map(|s| (s.as_str(), true)).collect();
        layout.paint(
            p,
            &self.regular,
            &items,
            controls
                .iter()
                .position(|(_, r)| r.contains(self.pointer.0, self.pointer.1)),
            controls
                .iter()
                .position(|(a, _)| focus.shows(self.focus == Some(*a), false)),
        );
    }
}
