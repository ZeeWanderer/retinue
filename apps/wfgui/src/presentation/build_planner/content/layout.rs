use super::*;

pub(super) struct Layout {
    pub pieces: Vec<Piece>,
    pub height: f32,
    pub topology: Option<Bounds>,
}
impl Layout {
    fn add(&mut self, id: impl std::hash::Hash, bounds: Bounds, drawing: Drawing) {
        self.pieces.push(Piece {
            key: key("build.content.leaf", id),
            bounds,
            drawing,
        });
    }
    fn text(
        &mut self,
        id: impl std::hash::Hash,
        bounds: Bounds,
        text: impl Into<String>,
        size: f32,
        color: [u8; 4],
        bold: bool,
    ) {
        self.add(
            id,
            bounds,
            Drawing::Text {
                text: text.into(),
                size,
                color,
                bold,
                link: None,
            },
        );
    }
    fn plan(&mut self, plan: &build_planner_model::Plan, width: f32, mut y: f32) -> f32 {
        if plan.ready {
            self.text(
                "plan.title",
                Bounds::new(14.0, y, width - 28.0, 28.0),
                if plan.operations.is_empty() {
                    "Current polarities support every build"
                } else {
                    "Polarity changes"
                },
                17.0,
                GOOD,
                true,
            );
            y += 34.0;
            for region in entries(&plan.topology["regions"]) {
                let slots = entries(&region["slots"])
                    .iter()
                    .filter_map(|s| {
                        entries(&plan.polarities)
                            .iter()
                            .find(|p| p["slot_id"] == s["id"])
                    })
                    .collect::<Vec<_>>();
                if slots.is_empty() {
                    continue;
                }
                let columns = (number(region, "columns") as usize)
                    .clamp(1, 4)
                    .min(slots.len());
                let cell = ((width - 24.0) / columns as f32).min(158.0);
                let left = (width - cell * columns as f32) / 2.0;
                for (i, slot) in slots.iter().enumerate() {
                    self.add(
                        ("polarity", text(slot, "slot_id")),
                        Bounds::new(
                            left + (i % columns) as f32 * cell,
                            y + (i / columns) as f32 * 65.0,
                            cell,
                            58.0,
                        ),
                        Drawing::Polarity((*slot).clone()),
                    );
                }
                y += slots.len().div_ceil(columns) as f32 * 65.0 + 12.0;
            }
            for (i, operation) in plan.operations.iter().enumerate() {
                self.text(
                    ("operation", i),
                    Bounds::new(14.0, y, width - 28.0, 27.0),
                    operation,
                    14.0,
                    TEXT,
                    false,
                );
                y += 29.0;
            }
            if !plan.resources.is_empty() {
                self.text(
                    "resources",
                    Bounds::new(14.0, y, width - 28.0, 28.0),
                    &plan.resources,
                    14.0,
                    MUTED,
                    false,
                );
                y += 34.0;
            }
        } else if let Some(reason) = &plan.blocked {
            self.text(
                "blocked",
                Bounds::new(14.0, y, width - 28.0, 32.0),
                reason,
                15.0,
                BAD,
                false,
            );
            y += 42.0;
        }
        if let Some((label, valid)) = &plan.capacity {
            self.text(
                "capacity",
                Bounds::new(14.0, y, width - 28.0, 28.0),
                label,
                14.0,
                if *valid { GOOD } else { BAD },
                false,
            );
            y += 34.0;
        }
        y
    }
    fn topology(
        &mut self,
        ui: &ControlHost,
        model: &build_planner_model::Topology,
        width: f32,
        mut y: f32,
    ) -> f32 {
        let start = y;
        let upgrades = entries(&model.upgrades)
            .iter()
            .map(|u| (text(u, "topology_slot"), u))
            .collect::<HashMap<_, _>>();
        let polarities = entries(&model.polarities)
            .iter()
            .map(|p| (text(p, "slot_id"), text(p, "polarity")))
            .collect::<HashMap<_, _>>();
        for region in entries(&model.topology["regions"]) {
            let slots = entries(&region["slots"]);
            if slots.is_empty() {
                continue;
            }
            let kind = match text(region, "id") {
                "arcanes" => topology::Kind::Arcane,
                "shards" => topology::Kind::Shard,
                _ => topology::Kind::Mod,
            };
            let columns = (number(region, "columns") as usize).max(1).min(slots.len());
            let unit = match kind {
                topology::Kind::Arcane => 184.0,
                topology::Kind::Shard => 110.0,
                _ => 212.0,
            };
            let factor = ((width - 20.0) / (unit * columns as f32)).clamp(0.1, 1.0);
            let cell = unit * factor;
            let height = if kind == topology::Kind::Shard {
                90.0
            } else {
                120.0
            } * factor;
            let left = (width - cell * columns as f32) / 2.0;
            self.text(
                ("region", text(region, "id")),
                Bounds::new(12.0, y, width - 24.0, 24.0),
                text(region, "label"),
                14.0,
                MUTED,
                false,
            );
            y += 32.0;
            for (i, slot) in slots.iter().enumerate() {
                let id = text(slot, "id");
                let mut slot = topology::Slot {
                    kind,
                    upgrade: upgrades.get(id).copied().unwrap_or(&Value::Null).clone(),
                    slot: slot.clone(),
                    polarity: polarities.get(id).copied().unwrap_or("none").into(),
                    shard: entries(&model.shards)
                        .iter()
                        .find(|s| text(s, "topology_slot") == id || text(s, "slot_id") == id)
                        .or_else(|| entries(&model.shards).get(i))
                        .unwrap_or(&Value::Null)
                        .clone(),
                    scale: factor,
                    expanded_height: 0.0,
                };
                slot.measure(ui);
                self.add(
                    ("slot", id),
                    Bounds::new(
                        left + (i % columns) as f32 * cell,
                        y + (i / columns) as f32 * height,
                        cell,
                        height,
                    ),
                    Drawing::Slot(slot),
                );
            }
            y += height * slots.len().div_ceil(columns) as f32 + 12.0;
        }
        for (i, entry) in entries(&model.overrides).iter().enumerate() {
            let text = entry
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Ability override: {}", text(entry, "name")));
            self.text(
                ("override", i),
                Bounds::new(16.0, y, width - 32.0, 28.0),
                text,
                14.0,
                TEXT,
                false,
            );
            y += 30.0;
        }
        self.topology = Some(Bounds::new(0.0, start, width, y - start));
        y
    }
    fn notes(
        &mut self,
        ui: &ControlHost,
        notes: &mut notes::Notes,
        source: &str,
        width: f32,
        y: f32,
    ) -> f32 {
        let (runs, end) = notes.layout(ui, source, width, y);
        for (i, run) in runs.into_iter().enumerate() {
            self.add(
                ("notes", i),
                run.bounds,
                if run.rule {
                    Drawing::Rule
                } else {
                    Drawing::Text {
                        text: run.text,
                        size: run.size,
                        color: run.color,
                        bold: run.bold,
                        link: run.link,
                    }
                },
            );
        }
        end
    }
}
pub(super) fn build(
    ui: &ControlHost,
    model: &build_planner_model::Model,
    tab: &str,
    width: f32,
    notes: &mut notes::Notes,
) -> Layout {
    let mut layout = Layout {
        pieces: vec![],
        height: 0.0,
        topology: None,
    };
    layout.height = match tab {
        "equipment" => layout.topology(ui, &model.equipment.topology, width, 16.0),
        "discover" => {
            let end = layout.topology(ui, &model.discover.topology, width, 16.0);
            layout.notes(ui, notes, &model.discover.notes, width, end + 16.0)
        }
        _ => {
            let group = &model.groups;
            let mut y = layout.plan(&group.plan, width, 12.0);
            if group.members.is_empty() {
                layout.text(
                    "empty.member",
                    Bounds::new(16.0, y, width - 32.0, 40.0),
                    "Add a configuration or find a build for this group",
                    16.0,
                    MUTED,
                    false,
                );
                y += 60.0;
            } else {
                y = layout.topology(ui, &group.topology, width, y + 14.0);
            }
            layout.notes(ui, notes, &group.notes, width, y + 16.0)
        }
    };
    layout
}
