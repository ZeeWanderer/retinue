use super::*;

#[derive(Clone)]
struct Card {
    key: String,
    upgrade: Value,
    slot: Value,
    polarity: String,
    origin: (f32, f32),
    scale: f32,
}

#[derive(Default)]
pub struct Preview {
    cards: Vec<Card>,
    active: Option<Card>,
    expansion: ScalarTransition,
    value: f32,
}

impl Preview {
    pub fn begin(&mut self) {
        self.cards.clear();
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.expansion.deadline(now, Duration::from_millis(16))
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let value = self.expansion.value(now);
        let changed = self.value != value;
        self.value = value;
        changed
    }
    pub(super) fn card(
        &mut self,
        ui: &Ui,
        upgrade: &Value,
        slot: &Value,
        polarity: &str,
        x: f32,
        y: f32,
        scale: f32,
    ) {
        if upgrade.is_null() {
            return;
        }
        self.cards.push(Card {
            key: format!("build:mod:{}", text(slot, "id")),
            upgrade: upgrade.clone(),
            slot: slot.clone(),
            polarity: polarity.into(),
            origin: ui.screen_origin(x, y),
            scale,
        });
    }
    pub fn render(&mut self, ui: &mut Ui, enabled: bool) {
        let now = Instant::now();
        if let Some(active) = &self.active {
            if let Some(card) = self
                .cards
                .iter()
                .find(|c| c.key == active.key && c.upgrade == active.upgrade)
            {
                self.active = Some(card.clone());
            } else {
                self.clear();
                return;
            }
        }
        let held = self.active.as_ref().is_some_and(|card| {
            let (x, y, s) = placement(
                card.origin,
                card.scale,
                self.value,
                (ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale),
            );
            ui.rect(x, y, 200.0 * s, (110.0 + 170.0 * self.value) * s)
                .contains(ui.pointer().0, ui.pointer().1)
        });
        let candidate = self
            .cards
            .iter()
            .find(|c| ui.hover.as_ref() == Some(&c.key))
            .cloned();
        if enabled && !ui.popup_open() && (held || candidate.is_some()) {
            if !held {
                self.active = candidate;
                self.value = 0.0;
                self.expansion.set(0.0, now, Duration::ZERO);
            }
            self.expansion.set(1.0, now, Duration::from_millis(180));
        } else {
            self.expansion.set(0.0, now, Duration::from_millis(140));
        }
        if self.value <= 0.0 {
            return;
        }
        if let Some(card) = &self.active {
            let (x, y, s) = placement(
                card.origin,
                card.scale,
                self.value,
                (ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale),
            );
            mod_card(
                ui,
                &card.upgrade,
                &card.slot,
                &card.polarity,
                x,
                y,
                s,
                self.value,
            );
            ui.hit(
                "build:mod-preview",
                ui.rect(x, y, 200.0 * s, (110.0 + 170.0 * self.value) * s),
                "",
                true,
            );
            ui.region(
                "build.mod-preview",
                ui.rect(x, y, 200.0 * s, (110.0 + 170.0 * self.value) * s),
            );
        }
    }
}

fn placement(
    origin: (f32, f32),
    scale: f32,
    expansion: f32,
    bounds: (f32, f32),
) -> (f32, f32, f32) {
    let s = scale * (1.0 + 0.1 * expansion);
    let width = 200.0 * s;
    let height = (110.0 + 170.0 * expansion) * s;
    let x = (origin.0 + 100.0 * scale - width / 2.0).clamp(0.0, (bounds.0 - width).max(0.0));
    let y = (origin.1 + 55.0 * scale - height / 2.0).clamp(0.0, (bounds.1 - height).max(0.0));
    (x, y, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expansion_keeps_the_card_center_until_a_window_edge_constrains_it() {
        for scale in [1.0, 1.25, 2.0] {
            for expansion in [0.0, 0.5, 1.0] {
                let (x, y, s) = placement((500.0, 500.0), scale, expansion, (3000.0, 2000.0));
                assert!((x + 100.0 * s - (500.0 + 100.0 * scale)).abs() < 0.001);
                assert!(
                    (y + (110.0 + 170.0 * expansion) * s / 2.0 - (500.0 + 55.0 * scale)).abs()
                        < 0.001
                );
            }
        }
        let (x, y, _) = placement((0.0, 0.0), 1.0, 1.0, (600.0, 400.0));
        assert_eq!((x, y), (0.0, 0.0));
    }
}
