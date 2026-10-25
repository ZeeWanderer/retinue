use super::*;

impl Foundry {
    pub fn cursor(&self) -> retinue_ui::desktop::winit::window::CursorIcon {
        use retinue_ui::desktop::winit::window::CursorIcon;
        match self.hit(self.pointer) {
            Some(Action::Search) if self.search_rect().contains(self.pointer.0, self.pointer.1) => {
                CursorIcon::Text
            }
            Some(Action::Scroll | Action::FilterScroll) | None => CursorIcon::Default,
            Some(action) if self.filter_enabled(action) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }

    pub(super) fn hit(&self, p: (f64, f64)) -> Option<Action> {
        if self.component_open() {
            return self
                .component_controls()
                .into_iter()
                .find(|(_, r)| r.contains(p.0, p.1))
                .map(|(a, _)| a);
        }
        if self.filters_open() {
            return self.filter_hit(p);
        }
        if let Some((a, _)) = self
            .controls()
            .into_iter()
            .rev()
            .find(|(_, r)| r.contains(p.0, p.1))
        {
            return Some(a);
        }
        if self.scroll.maximum() > 0.0
            && p.0 >= self.size.0.saturating_sub(self.px(18.0)) as f64
            && p.1 >= self.viewport.y as f64
        {
            return Some(Action::Scroll);
        }
        self.resolved_composition().hit(p)
    }
    pub fn pointer_move(&mut self, p: (f64, f64)) -> Damage {
        if self.component_open() {
            let old = self.hit(self.pointer);
            self.pointer = p;
            return if old != self.hit(p) {
                Damage::rect(self.component_panel.popup.bounds)
            } else {
                Damage::None
            };
        }
        if self.filters_open() {
            let old = self.filter_hit(self.pointer);
            self.pointer = p;
            if let Some(grab) = self.filter_panel.drag {
                let v = self.filter_viewport();
                let travel = v
                    .height
                    .saturating_sub(self.filter_scrollbar().height)
                    .max(1);
                let now = Instant::now();
                self.filter_panel.scroll.to(
                    ((p.1 - v.y as f64 - grab) / travel as f64) as f32
                        * self.filter_panel.scroll.maximum(),
                    false,
                    now,
                );
                return self.tick(now);
            }
            let damage = if old != self.filter_hit(p) {
                Damage::rect(self.filter_panel.popup.bounds)
            } else {
                Damage::None
            };
            let hint = (self.filter_hit(p) == Some(Action::CloseFilters))
                .then(|| "Close filters".to_owned());
            return damage.union(self.track_hint(hint));
        }
        let before = self.hovered_card();
        self.pointer = p;
        let after = self.hovered_card();
        let damage = if before != after {
            before
                .into_iter()
                .chain(after)
                .fold(Damage::None, |damage, i| {
                    damage.union(Damage::rect(self.screen_card(i)))
                })
        } else {
            Damage::None
        };
        if let Some(grab) = self.drag {
            let thumb = self.scrollbar();
            let travel = self.viewport.height.saturating_sub(thumb.height).max(1);
            self.scroll.to(
                ((p.1 - self.viewport.y as f64 - grab) / travel as f64) as f32
                    * self.scroll.maximum(),
                false,
                Instant::now(),
            );
            return self.tick(Instant::now());
        }
        if self.pressed == Some(Action::Search) {
            let r = self.search_rect();
            self.search.borrow_mut().drag((p.0 - r.x as f64) as f32);
            return Damage::rect(self.search_rect());
        }
        let hint = self.hint(p);
        damage.union(self.track_hint(hint))
    }
    fn track_hint(&mut self, hint: Option<String>) -> Damage {
        if hint != self.hover {
            self.hover = hint;
            self.hover_since = Instant::now();
            let was = self.tooltip;
            self.tooltip = false;
            if was {
                return Damage::Full;
            }
        }
        Damage::None
    }
    fn hint(&self, p: (f64, f64)) -> Option<String> {
        if let Some(action) = self.hit(p) {
            return Some(match action {
                Action::Category(i) => CATEGORIES[i].into(),
                Action::Refresh => "Refresh Foundry".into(),
                Action::Filters => "Filter Foundry".into(),
                Action::Search if self.search_rect().width == 0 => "Search Foundry (Ctrl+F)".into(),
                Action::ClearSearch => "Clear search".into(),
                Action::Favorite(i) => {
                    if self.preferences.is_favorite(&self.snapshot.items[i].id) {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    }
                    .into()
                }
                Action::ComponentRelic(_, _) => "Show this reward in Relic Planner".into(),
                Action::Component(r, p) => {
                    let part = &self.snapshot.items[r].components[p];
                    format!(
                        "{} | {} / {}{}{}",
                        part.name,
                        part.owned,
                        part.required,
                        if part.blueprint_owned > 0 {
                            " | Blueprint owned"
                        } else {
                            ""
                        },
                        if part.owned_relic {
                            " | In owned relics"
                        } else {
                            ""
                        }
                    )
                }
                _ => return None,
            });
        }
        if !self.viewport.contains(p.0, p.1) {
            return None;
        }
        for index in self.visible(0) {
            let r = self.card_rect(index);
            let c = &self.snapshot.items[self.filtered[index]];
            let x = p.0 - r.x as f64;
            let y = p.1 + self.offset as f64 - r.y as f64;
            if x >= 0.0 && x < self.grid.width as f64 && y >= 0.0 && y < self.grid.height as f64 {
                return Some(format!(
                    "{}{}{}{}",
                    c.name,
                    if c.mastered { " | Mastered" } else { "" },
                    if c.is_prime {
                        if c.vaulted {
                            " | Vaulted"
                        } else {
                            " | Unvaulted"
                        }
                    } else {
                        ""
                    },
                    if c.subsumed { " | Subsumed" } else { "" }
                ));
            }
        }
        None
    }
    pub fn pointer_button(&mut self, p: (f64, f64), down: bool) -> Damage {
        if down && self.component_open() && !self.component_panel.popup.bounds.contains(p.0, p.1) {
            self.close_component();
            return Damage::Full;
        }
        if down && self.filter_panel.popup.press(p) == retinue_ui::popover::Press::Dismissed {
            self.cancel_pointer();
            self.focus = Some(Action::Filters);
            return Damage::Full;
        }
        let hit = self.hit(p);
        if down {
            self.pressed = hit;
            if !self.popup_open() || hit.is_some() {
                self.focus_action(hit);
            }
            if hit == Some(Action::FilterScroll) {
                let thumb = self.filter_scrollbar();
                self.filter_panel.drag = Some(if thumb.contains(p.0, p.1) {
                    p.1 - thumb.y as f64
                } else {
                    thumb.height as f64 / 2.0
                });
                return self.pointer_move(p);
            }
            if hit == Some(Action::Search) {
                let r = self.search_rect();
                if r.contains(p.0, p.1) {
                    self.search
                        .borrow_mut()
                        .click((p.0 - r.x as f64) as f32, false);
                }
            }
            if hit == Some(Action::Scroll) {
                let thumb = self.scrollbar();
                self.drag = Some(if thumb.contains(p.0, p.1) {
                    p.1 - thumb.y as f64
                } else {
                    thumb.height as f64 / 2.0
                });
                return self.pointer_move(p);
            }
        } else {
            self.drag = None;
            self.filter_panel.drag = None;
            if let Some(action) = self.pressed.take().filter(|a| Some(*a) == hit) {
                self.activate(action);
            }
        }
        Damage::Full
    }
    pub fn activate(&mut self, action: Action) {
        match action {
            Action::Search => self.focus_action(Some(Action::Search)),
            Action::Component(r, p) => {
                if !self.component_market(r, p) {
                    self.open_component(r, p);
                }
            }
            Action::ComponentRelic(r, p) => self.component_relics(r, p),
            Action::ComponentFavorite
            | Action::ComponentSearch
            | Action::ComponentMarket
            | Action::ComponentRelics => self.component_activate(action),
            Action::ClearSearch => {
                self.search.borrow_mut().set_text("", false);
                self.focus_action(Some(Action::Search));
                self.filter(true);
            }
            Action::Category(i) => self.select_category(i, Instant::now()),
            Action::Filters => self.open_filters(),
            Action::CloseFilters => self.close_filters(),
            Action::FilterChoice(filter, value) if self.filter_enabled(action) => {
                self.filters.toggle(filter, value);
                self.filter(true);
            }
            Action::ClearFilters => {
                self.filters = Filters::default();
                self.filter(true);
                self.focus_filter(Action::FilterChoice(Filter::Prime, true));
            }
            Action::Refresh => {
                self.refresh += 1;
                self.error = None;
            }
            Action::Favorite(i) => {
                if let Some(card) = self.snapshot.items.get(i) {
                    self.preferences.toggle_favorite(&card.id);
                    let anchor = self.anchor();
                    self.filter(false);
                    self.restore_anchor(anchor);
                }
            }
            _ => (),
        }
    }
    pub fn cancel_pointer(&mut self) {
        self.pointer = (-1.0, -1.0);
        self.drag = None;
        self.filter_panel.drag = None;
        self.pressed = None;
        self.hover = None;
        self.tooltip = false;
    }
    pub fn wheel(&mut self, delta: f32, smooth: bool) {
        if self.component_open() {
            return;
        }
        if self.filters_open() {
            self.filter_panel.scroll.by(delta, smooth, Instant::now());
            return;
        }
        self.scroll.by(delta, smooth, Instant::now());
        self.hover = None;
        self.tooltip = false;
    }
    pub fn jump(&mut self, offset: f32) {
        self.scroll.to(offset, false, Instant::now());
        self.offset = self.scroll.offset(Instant::now()).round() as u32;
    }
    pub fn key(&mut self, key: InputKey<&str>, modifiers: ModifiersState) -> bool {
        if self.component_open() {
            self.component_key(key, modifiers);
            return true;
        }
        if self.filters_open() {
            self.filter_key(key, modifiers);
            return true;
        }
        if modifiers.control_key() && key == InputKey::Character("f") {
            self.focus_action(Some(Action::Search));
            self.search.borrow_mut().key(0x61, true, false);
            return true;
        }
        if self.focus != Some(Action::Search) {
            return false;
        }
        self.input_error = None;
        if modifiers.control_key()
            && let InputKey::Character(key @ ("c" | "x" | "v")) = key
        {
            self.request_clipboard(key);
            return true;
        }
        if key == InputKey::Named(NamedKey::Escape) {
            self.focus_action(None);
            return true;
        }
        let previous = self.search.borrow().text().to_owned();
        let changed = match key {
            InputKey::Character("a") if modifiers.control_key() => {
                self.search
                    .borrow_mut()
                    .key(0x61, true, modifiers.shift_key())
            }
            InputKey::Character(text) if !modifiers.control_key() && !modifiers.alt_key() => self
                .search
                .borrow_mut()
                .insert(text, |s| s.chars().count() <= 256),
            InputKey::Named(key) => {
                let symbol = match key {
                    NamedKey::Backspace => 0xff08,
                    NamedKey::Delete => 0xffff,
                    NamedKey::ArrowLeft => 0xff51,
                    NamedKey::ArrowRight => 0xff53,
                    NamedKey::Home => 0xff50,
                    NamedKey::End => 0xff57,
                    _ => return false,
                };
                self.search
                    .borrow_mut()
                    .key(symbol, modifiers.control_key(), modifiers.shift_key())
            }
            _ => false,
        };
        if self.search.borrow().text() != previous {
            self.layout_search(Instant::now(), true);
            self.filter(true);
        }
        changed
    }
    pub fn tick(&mut self, now: Instant) -> Damage {
        let offset = self.scroll.offset(now).round() as u32;
        let mut damage = Damage::None;
        if self.filters_open() {
            let offset = self.filter_panel.scroll.offset(now).round() as u32;
            if offset != self.filter_panel.offset {
                self.filter_panel.offset = offset;
                damage = Damage::rect(self.filter_panel.popup.bounds);
            }
        }
        let categories_changed = self.category_motion.sample(now);
        if self.layout_search(now, true) || categories_changed {
            damage = damage.union(Damage::rect(Rect {
                x: 0,
                y: 0,
                width: self.size.0,
                height: self.viewport.y,
            }));
        }
        if offset != self.offset {
            self.offset = offset;
            if self.component_open() {
                self.layout_component();
                damage = Damage::Full;
            }
            damage = damage.union(Damage::rect(Rect {
                x: 0,
                y: self.viewport.y,
                width: self.size.0,
                height: self.viewport.height,
            }));
        }
        if self.focus == Some(Action::Search) && self.search.borrow_mut().tick() {
            damage = damage.union(Damage::rect(self.search_rect()));
        }
        if self.hover.is_some()
            && !self.tooltip
            && now.saturating_duration_since(self.hover_since) >= Duration::from_millis(600)
        {
            self.tooltip = true;
            damage = Damage::Full;
        }
        damage
    }
    pub fn deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        [
            self.category_motion.deadline(now),
            self.search_motion.deadline(now),
            self.scroll.deadline(now),
            self.filters_open()
                .then(|| self.filter_panel.scroll.deadline(now))
                .flatten(),
            (self.focus == Some(Action::Search)).then(|| self.search.borrow().next_blink(now)),
            (self.hover.is_some() && !self.tooltip)
                .then_some(self.hover_since + Duration::from_millis(600)),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn accessibility(&self) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        let (base, controls) = self.accessible_controls();
        for (i, (action, r)) in controls.iter().enumerate() {
            let id = NodeId(base + i as u64);
            let mut node = Node::new(if *action == Action::Search {
                Role::TextInput
            } else {
                Role::Button
            });
            node.set_label(match action {
                Action::Category(i) => CATEGORIES[*i].to_owned(),
                Action::Search => "Search Foundry".into(),
                Action::ClearSearch => "Clear search".into(),
                Action::Filters => "Filter Foundry".into(),
                Action::CloseFilters => "Close filters".into(),
                Action::ClearFilters => "Clear filters".into(),
                Action::FilterChoice(filter, value) => {
                    format!("{}: {}", filter.title(), filter.label(*value))
                }
                Action::Refresh => "Refresh".into(),
                Action::ComponentFavorite
                | Action::ComponentSearch
                | Action::ComponentMarket
                | Action::ComponentRelics => self.component_label(*action),
                _ => "Foundry".into(),
            });
            node.set_bounds(accesskit::Rect {
                x0: r.x as f64,
                y0: r.y as f64,
                x1: (r.x + r.width) as f64,
                y1: (r.y + r.height) as f64,
            });
            if self.filter_enabled(*action) {
                node.add_action(accesskit::Action::Click);
                node.add_action(accesskit::Action::Focus);
            } else {
                node.set_disabled();
            }
            if let Action::FilterChoice(filter, value) = action {
                node.set_toggled(if self.filters.get(*filter) == Some(*value) {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }
            if *action == Action::Search {
                node.set_value(self.search.borrow().text());
                node.add_action(accesskit::Action::SetValue);
            }
            children.push(id);
            nodes.push((id, node));
        }
        for i in self.visible(0).filter(|_| !self.popup_open()) {
            let c = &self.snapshot.items[self.filtered[i]];
            let id = NodeId(1000 + self.filtered[i] as u64);
            let mut node = Node::new(Role::Label);
            node.set_label(format!(
                "{}{}{}",
                c.name,
                if c.owned { ", owned" } else { "" },
                if c.mastered { ", mastered" } else { "" }
            ));
            children.push(id);
            nodes.push((id, node));
        }
        if !self.popup_open() {
            for (action, r) in self.item_controls() {
                let (row, part) = match action {
                    Action::Favorite(row) => (row, 0),
                    Action::Component(row, part) => (row, part + 1),
                    _ => continue,
                };
                let id = NodeId(1_000_000 + row as u64 * 16 + part as u64);
                let c = &self.snapshot.items[row];
                let mut node = Node::new(Role::Button);
                if part == 0 {
                    node.set_label(format!("Favorite {}", c.name));
                    node.set_toggled(if self.preferences.is_favorite(&c.id) {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    });
                } else {
                    node.set_label(format!(
                        "{}: component details for {}",
                        c.components[part - 1].name,
                        c.name
                    ));
                }
                node.set_bounds(accesskit::Rect {
                    x0: r.x as f64,
                    y0: r.y as f64,
                    x1: (r.x + r.width) as f64,
                    y1: (r.y + r.height) as f64,
                });
                node.add_action(accesskit::Action::Focus);
                node.add_action(accesskit::Action::Click);
                children.push(id);
                nodes.push((id, node));
            }
        }
        let mut root = Node::new(if self.popup_open() {
            Role::Dialog
        } else {
            Role::GenericContainer
        });
        root.set_label(if self.component_open() {
            self.component_summary()
        } else if self.filters_open() {
            "Foundry filters".into()
        } else {
            "Foundry".into()
        });
        root.set_children(children);
        nodes.push((NodeId(2), root));
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(2))),
            tree_id: accesskit::TreeId::ROOT,
            focus: match self.focus.filter(|_| !self.popup_open()) {
                Some(Action::Favorite(row)) => NodeId(1_000_000 + row as u64 * 16),
                Some(Action::Component(row, part)) => {
                    NodeId(1_000_001 + row as u64 * 16 + part as u64)
                }
                _ => controls
                    .iter()
                    .position(|(a, _)| Some(*a) == self.focus)
                    .map_or(NodeId(2), |i| NodeId(base + i as u64)),
            },
        }
    }
    pub fn access(&mut self, request: accesskit::ActionRequest) {
        if let Some(id) = request.target_node.0.checked_sub(1_000_000) {
            let row = (id / 16) as usize;
            let part = (id % 16) as usize;
            let action = if part == 0 {
                Action::Favorite(row)
            } else {
                Action::Component(row, part - 1)
            };
            if !self.popup_open() && self.item_controls().iter().any(|(a, _)| *a == action) {
                self.focus_action(Some(action));
                if request.action == accesskit::Action::Click {
                    self.activate(action);
                }
            }
            return;
        }
        let (base, controls) = self.accessible_controls();
        let Some(index) = request.target_node.0.checked_sub(base) else {
            return;
        };
        let Some((action, _)) = controls.get(index as usize).copied() else {
            return;
        };
        if !self.filter_enabled(action) {
            return;
        }
        match request.action {
            accesskit::Action::Focus => self.focus_filter(action),
            accesskit::Action::Click => {
                self.focus_filter(action);
                self.activate(action);
            }
            accesskit::Action::SetValue if action == Action::Search => {
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    self.set_search(&value);
                }
            }
            _ => (),
        }
    }

    fn accessible_controls(&self) -> (u64, Vec<(Action, Rect)>) {
        if self.component_open() {
            (400, self.component_controls())
        } else if self.filters_open() {
            (300, self.filter_controls())
        } else {
            (200, self.controls())
        }
    }
}
