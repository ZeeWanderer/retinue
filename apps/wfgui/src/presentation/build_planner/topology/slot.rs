use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(in crate::presentation::build_planner) enum Kind {
    Mod,
    Arcane,
    Shard,
}

#[derive(Clone, PartialEq)]
pub(in crate::presentation::build_planner) struct Slot {
    pub kind: Kind,
    pub upgrade: Value,
    pub slot: Value,
    pub polarity: String,
    pub shard: Value,
    pub scale: f32,
    pub expanded_height: f32,
}

impl Slot {
    pub fn measure(&mut self, ui: &ControlHost) {
        if self.kind == Kind::Mod {
            self.expanded_height =
                ModText::new(ui, &self.upgrade, Frame::from(&self.upgrade)).height;
        }
    }
    pub fn mod_origin(&self, x: f32, y: f32) -> (f32, f32) {
        (x + 6.0 * self.scale, y)
    }
    pub fn paint(&self, ui: &mut ControlHost, x: f32, y: f32) {
        let s = self.scale;
        match self.kind {
            Kind::Mod => {
                let (x, y) = self.mod_origin(x, y);
                mod_card(ui, &self.upgrade, &self.slot, &self.polarity, x, y, s, 0.0);
            }
            Kind::Arcane => arcane_card(ui, &self.upgrade, &self.slot, x, y, s),
            Kind::Shard => {
                ui.image(
                    ui.rect(x + 25.0 * s, y, 60.0 * s, 60.0 * s),
                    asset(&self.shard["asset"]).as_ref(),
                );
                ui.text(
                    ui.rect(x, y + 62.0 * s, 110.0 * s, 24.0 * s),
                    if self.shard.is_null() {
                        "Empty"
                    } else {
                        text(&self.shard, "name")
                    },
                    12.0 * s,
                    MUTED,
                );
            }
        }
    }
    pub fn tooltip(&self) -> String {
        if self.upgrade.is_null() {
            format!(
                "{} - {}",
                text(&self.slot, "label"),
                if self.slot["unlocked"] == false {
                    "Locked"
                } else {
                    "Empty"
                }
            )
        } else {
            format!(
                "{} - Rank {:.0}/{:.0}\n{}",
                text(&self.upgrade, "name"),
                number(&self.upgrade, "rank"),
                number(&self.upgrade, "max_rank"),
                entries(&self.upgrade["effects"])
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }
}
