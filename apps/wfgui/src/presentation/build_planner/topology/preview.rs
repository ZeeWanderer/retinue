use super::*;

#[derive(Clone, PartialEq)]
struct Card {
    key: String,
    upgrade: Value,
    slot: Value,
    polarity: String,
    origin: (f32, f32),
    scale: f32,
    expanded_height: f32,
}

#[derive(Clone, PartialEq)]
pub(in crate::presentation::build_planner) struct Frame {
    card: Card,
    scale: f32,
    expansion: f32,
    pub bounds: retinue_render::Bounds,
    pub active: bool,
}
impl Frame {
    pub fn key(&self) -> &str {
        &self.card.key
    }
    pub fn paint(&self, ui: &mut ControlHost, x: f32, y: f32) {
        mod_card(
            ui,
            &self.card.upgrade,
            &self.card.slot,
            &self.card.polarity,
            x,
            y,
            self.scale,
            self.expansion,
        );
    }
}

#[derive(Default)]
pub struct Preview {
    cards: Vec<Card>,
    active: Option<String>,
    visible: Vec<AnimatedCard>,
}

struct AnimatedCard {
    card: Card,
    expansion: ScalarTransition,
    value: f32,
}

impl Preview {
    pub(in crate::presentation::build_planner) fn covers(
        &self,
        slot: &str,
        upgrade: &Value,
    ) -> bool {
        self.visible.iter().any(|animated| {
            animated.value > 0.0
                && text(&animated.card.slot, "id") == slot
                && animated.card.upgrade == *upgrade
        })
    }
    pub fn begin(&mut self) {
        self.cards.clear();
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.visible
            .iter()
            .filter_map(|animated| animated.expansion.deadline(now, Duration::from_millis(16)))
            .min()
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut changed = false;
        self.visible.retain_mut(|animated| {
            let value = animated.expansion.value(now);
            changed |= animated.value != value;
            animated.value = value;
            let keep = value > 0.0 || animated.expansion.deadline(now, Duration::ZERO).is_some();
            changed |= !keep;
            keep
        });
        changed
    }
    pub(in crate::presentation::build_planner) fn card_at(
        &mut self,
        upgrade: &Value,
        slot: &Value,
        polarity: &str,
        origin: (f32, f32),
        scale: f32,
        expanded_height: f32,
    ) {
        if upgrade.is_null() {
            return;
        }
        self.cards.push(Card {
            key: format!("build:mod:{}", text(slot, "id")),
            upgrade: upgrade.clone(),
            slot: slot.clone(),
            polarity: polarity.into(),
            origin,
            scale,
            expanded_height,
        });
    }
    pub(in crate::presentation::build_planner) fn prepare(
        &mut self,
        ui: &ControlHost,
        enabled: bool,
        now: Instant,
    ) {
        self.visible.retain_mut(|animated| {
            if let Some(card) = self
                .cards
                .iter()
                .find(|c| c.key == animated.card.key && c.upgrade == animated.card.upgrade)
            {
                animated.card = card.clone();
                true
            } else {
                false
            }
        });
        let held = self.visible.iter().find(|animated| {
            if self.active.as_ref() != Some(&animated.card.key) {
                return false;
            }
            let card = &animated.card;
            let (x, y, s) = placement(
                card.origin,
                card.scale,
                animated.value,
                card.expanded_height,
                (ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale),
            );
            ui.rect(
                x,
                y,
                200.0 * s,
                canvas_height(card.expanded_height, animated.value) * s,
            )
            .contains(ui.pointer().0, ui.pointer().1)
        });
        let candidate = if enabled && !ui.popup_open() {
            held.map(|animated| &animated.card)
                .or_else(|| {
                    self.cards
                        .iter()
                        .find(|c| ui.hover.as_ref() == Some(&c.key))
                })
                .cloned()
        } else {
            None
        };
        self.retarget(candidate, now);
    }

    pub(in crate::presentation::build_planner) fn frames(&self, ui: &ControlHost) -> Vec<Frame> {
        self.visible
            .iter()
            .filter(|animated| animated.value > 0.0)
            .map(|animated| {
                let card = &animated.card;
                let (x, y, s) = placement(
                    card.origin,
                    card.scale,
                    animated.value,
                    card.expanded_height,
                    (ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale),
                );
                Frame {
                    card: card.clone(),
                    scale: s,
                    expansion: animated.value,
                    bounds: retinue_render::Bounds::new(
                        x,
                        y,
                        200.0 * s,
                        canvas_height(card.expanded_height, animated.value) * s,
                    ),
                    active: self.active.as_ref() == Some(&card.key),
                }
            })
            .collect()
    }

    #[cfg(test)]
    fn render_at(&mut self, ui: &mut ControlHost, enabled: bool, now: Instant) {
        self.prepare(ui, enabled, now);
        for frame in self.frames(ui) {
            frame.paint(ui, frame.bounds.x, frame.bounds.y);
            if frame.active {
                let r = ui.rect(
                    frame.bounds.x,
                    frame.bounds.y,
                    frame.bounds.width,
                    frame.bounds.height,
                );
                ui.hit("build:mod-preview", r, "", true);
                ui.region("build.mod-preview", r);
            }
        }
    }

    fn retarget(&mut self, candidate: Option<Card>, now: Instant) {
        let key = candidate.as_ref().map(|card| card.key.clone());
        if self.active != key
            || key
                .as_ref()
                .is_some_and(|key| !self.visible.iter().any(|a| &a.card.key == key))
        {
            if let Some(card) = candidate {
                let animated = match self.visible.iter().position(|a| a.card.key == card.key) {
                    Some(index) => self.visible.remove(index),
                    None => AnimatedCard {
                        card,
                        expansion: ScalarTransition::default(),
                        value: 0.0,
                    },
                };
                self.visible.push(animated);
            }
            self.active = key;
        }
        for animated in &mut self.visible {
            let opening = self.active.as_ref() == Some(&animated.card.key);
            animated.expansion.set(
                if opening { 1.0 } else { 0.0 },
                now,
                Duration::from_millis(if opening { 180 } else { 140 }),
            );
        }
    }
}

fn canvas_height(expanded_height: f32, expansion: f32) -> f32 {
    110.0 + (expanded_height - 90.0) * expansion
}

fn placement(
    origin: (f32, f32),
    scale: f32,
    expansion: f32,
    expanded_height: f32,
    bounds: (f32, f32),
) -> (f32, f32, f32) {
    let canvas_height = canvas_height(expanded_height, expansion);
    let s = (scale * (1.0 + 0.1 * expansion))
        .min(bounds.0 / 200.0)
        .min(bounds.1 / canvas_height);
    let width = 200.0 * s;
    let height = canvas_height * s;
    let x = (origin.0 + 100.0 * scale - width / 2.0).clamp(0.0, (bounds.0 - width).max(0.0));
    let y = (origin.1 + 55.0 * scale - height / 2.0).clamp(0.0, (bounds.1 - height).max(0.0));
    (x, y, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, x: f32) -> Card {
        Card {
            key: format!("build:mod:{id}"),
            upgrade: json!({"name":id, "effects":["+165% Damage"], "compat_name":"Shotgun"}),
            slot: json!({"id":id}),
            polarity: "none".into(),
            origin: (x, 250.0),
            scale: 1.0,
            expanded_height: 260.0,
        }
    }

    #[test]
    fn changing_cards_closes_the_old_one_while_opening_the_new_one() {
        let mut preview = Preview::default();
        let a = card("a", 100.0);
        let b = card("b", 350.0);
        let now = Instant::now();
        preview.retarget(Some(a.clone()), now);
        preview.tick(now + Duration::from_millis(200));
        let switched = now + Duration::from_millis(200);
        preview.retarget(Some(b.clone()), switched);
        assert_eq!(preview.visible.len(), 2);
        assert_eq!(preview.visible[0].value, 1.0);
        assert_eq!(preview.visible[1].value, 0.0);
        assert!(preview.covers("a", &a.upgrade));
        preview.tick(switched + Duration::from_millis(50));
        assert!(
            preview
                .visible
                .iter()
                .all(|a| a.value > 0.0 && a.value < 1.0)
        );
        assert!(preview.covers("b", &b.upgrade));
        preview.tick(switched + Duration::from_millis(200));
        assert_eq!(preview.visible.len(), 1);
        assert_eq!(preview.visible[0].card.key, b.key);
        assert!(!preview.covers("a", &a.upgrade));
        assert!(
            preview
                .deadline(switched + Duration::from_millis(200))
                .is_none()
        );
        preview.retarget(None, switched + Duration::from_millis(200));
        preview.tick(switched + Duration::from_millis(350));
        assert!(preview.visible.is_empty());
    }

    #[test]
    fn reversing_a_close_reuses_its_current_expansion_without_a_jump() {
        let mut preview = Preview::default();
        let a = card("a", 100.0);
        let b = card("b", 350.0);
        let now = Instant::now();
        preview.retarget(Some(a.clone()), now);
        preview.tick(now + Duration::from_millis(200));
        preview.retarget(Some(b.clone()), now + Duration::from_millis(200));
        let reversed = now + Duration::from_millis(250);
        preview.tick(reversed);
        let values: Vec<_> = preview.visible.iter().map(|a| a.value).collect();
        preview.retarget(Some(a.clone()), reversed);
        preview.tick(reversed);
        assert_eq!(preview.visible.len(), 2);
        assert_eq!(preview.visible[0].value, values[1]);
        assert_eq!(preview.visible[1].value, values[0]);
        assert_eq!(preview.visible[1].card.key, a.key);
        preview.tick(reversed + Duration::from_millis(200));
        assert_eq!(preview.visible.len(), 1);
        assert_eq!(preview.visible[0].card.key, a.key);
    }

    #[test]
    fn hover_handoff_renders_both_cards_but_only_the_active_one_intercepts_input() {
        for scale in [1.0, 1.25, 2.0] {
            let mut ui = ControlHost::new().unwrap();
            let mut preview = Preview::default();
            let a = card("a", 100.0);
            let b = card("b", 350.0);
            preview.cards = vec![a.clone(), b.clone()];
            let now = Instant::now();
            ui.begin((2000, 1500), scale);
            ui.pointer_move((150.0 * scale as f64, 280.0 * scale as f64));
            ui.hover = Some(a.key.clone());
            preview.render_at(&mut ui, true, now);
            preview.tick(now + Duration::from_millis(200));
            ui.pointer_move((450.0 * scale as f64, 280.0 * scale as f64));
            ui.hover = Some(b.key.clone());
            preview.render_at(&mut ui, true, now + Duration::from_millis(200));
            assert_eq!(preview.visible.len(), 2);
            let middle = now + Duration::from_millis(250);
            preview.tick(middle);
            ui.begin((2000, 1500), scale);
            preview.render_at(&mut ui, true, middle);
            let report = ui.layout_report();
            let names: Vec<_> = report["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|e| e["text"].as_str())
                .filter(|name| *name == "a" || *name == "b")
                .collect();
            assert!(names.contains(&"a") && names.contains(&"b"));
            let hit = ui.bounds("build:mod-preview").unwrap();
            assert!(hit.x as f32 > 300.0 * scale);
            assert_eq!(preview.active.as_ref(), Some(&b.key));
            super::super::tests::capture(&mut ui, 650.0, 500.0, "handoff");
            preview.cards[1].upgrade["name"] = json!("changed build");
            ui.begin((2000, 1500), scale);
            preview.render_at(&mut ui, true, middle);
            assert_eq!(
                preview.visible.last().unwrap().card.upgrade["name"],
                "changed build"
            );
            preview.cards.clear();
            preview.render_at(&mut ui, true, middle);
            assert!(preview.visible.is_empty());
            assert!(preview.active.is_none());
        }
    }

    #[test]
    fn expansion_keeps_the_card_center_until_a_window_edge_constrains_it() {
        for scale in [1.0, 1.25, 2.0] {
            for expansion in [0.0, 0.5, 1.0] {
                let (x, y, s) =
                    placement((500.0, 500.0), scale, expansion, 260.0, (3000.0, 2000.0));
                assert!((x + 100.0 * s - (500.0 + 100.0 * scale)).abs() < 0.001);
                assert!(
                    (y + (110.0 + 170.0 * expansion) * s / 2.0 - (500.0 + 55.0 * scale)).abs()
                        < 0.001
                );
            }
        }
        let (x, y, _) = placement((0.0, 0.0), 1.0, 1.0, 260.0, (600.0, 400.0));
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn long_descriptions_remain_inside_the_window() {
        let (x, y, scale) = placement((300.0, 300.0), 1.25, 1.0, 600.0, (800.0, 500.0));
        assert!(x >= 0.0 && x + 200.0 * scale <= 800.0);
        assert!(y >= 0.0 && y + canvas_height(600.0, 1.0) * scale <= 500.0);
    }
}
