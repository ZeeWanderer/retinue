mod equipment_view;
use super::*;

fn node(id: impl std::hash::Hash) -> Key {
    key("build.equipment", id)
}
const LIST: Key = Key::new("build.equipment.list", 0);
const BAR: Key = Key::new("build.equipment.scroll", 0);

pub(super) struct List {
    rows: Items<(usize, build_planner_model::EquipmentRow)>,
    configs: Items<(usize, String)>,
    offset: State<f32>,
    height: State<f32>,
    filter: Option<(String, String)>,
    indices: Vec<usize>,
}
impl List {
    pub fn view(
        &self,
        metrics: State<Metrics>,
        model: State<Rc<build_planner_model::Model>>,
        fonts: Fonts,
    ) -> Element {
        equipment_view::EquipmentView {
            metrics,
            model,
            fonts,
            rows: self.rows.clone(),
            configs: self.configs.clone(),
            offset: self.offset.clone(),
            height: self.height.clone(),
        }
        .into_element()
    }

    pub fn new() -> Self {
        Self {
            rows: Items::default(),
            configs: Items::default(),
            offset: State::new(0.0),
            height: State::new(0.0),
            filter: None,
            indices: Vec::new(),
        }
    }
    pub fn sync(
        &mut self,
        m: &Metrics,
        data: &build_planner_model::Equipment,
        b: &mut Builds,
        changed: bool,
        now: Instant,
    ) {
        if changed {
            self.filter = None;
        }
        self.configs
            .replace(
                data.configs
                    .iter()
                    .cloned()
                    .map(|c| (node(("config", c.0)), c)),
            )
            .expect("unique configs");
        if b.tab != "equipment" || m.sidebar <= 100.0 {
            self.rows.replace([]).unwrap();
            return;
        }
        let filter = (b.search.to_lowercase(), b.class.clone());
        if self.filter.as_ref() != Some(&filter) {
            self.indices = data
                .definitions
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    (filter.1 == "all" || d.class == filter.1)
                        && (filter.0.is_empty() || d.name.to_lowercase().contains(&filter.0))
                })
                .map(|(i, _)| i)
                .collect();
            self.filter = Some(filter);
        }
        self.height.set(self.indices.len() as f32 * 47.0);
        let viewport = m.equipment_list();
        b.list_scroll.resize(
            (self.indices.len() as f32 * 47.0 * m.scale).round(),
            viewport.height * m.scale,
            now,
        );
        b.list_offset = b.list_scroll.offset(now).round() as u32;
        self.offset.set(b.list_offset as f32 / m.scale);
        b.list_viewport = Some(Rect {
            x: (viewport.x * m.scale).round() as u32,
            y: (viewport.y * m.scale).round() as u32,
            width: (viewport.width * m.scale).round() as u32,
            height: (viewport.height * m.scale).round() as u32,
        });
        let offset = self.offset.get();
        self.rows
            .replace(
                visible_rows(offset, viewport.height, 47.0, self.indices.len())
                    .map(|i| (i, &data.definitions[self.indices[i]]))
                    .map(|(i, row)| (node(("row", &row.id)), (i, row.clone()))),
            )
            .expect("unique owned definitions");
    }
}

pub(super) fn bind(
    ui: &mut ControlHost,
    b: &Builds,
    data: &build_planner_model::Equipment,
    list: &List,
) {
    if b.tab != "equipment" {
        return;
    }
    ui.view_control(node("collapse.icon"), |ui, r| {
        ui.tinted_bitmap(
            r,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/ui/panel-left.png"
            )),
            TEXT,
        )
    });
    ui.view_hit(
        node("collapse"),
        "build:collapse",
        "Toggle equipment list",
        true,
    );
    ui.view_control(node("search"), |ui, r| {
        ui.search_field(
            "build:search",
            r,
            &b.search,
            "Equipment",
            crate::presentation::widgets::search::Anchor::Leading,
        );
    });
    ui.view_control(node("class"), |ui, r| {
        ui.choice(
            "build:class",
            r,
            &b.class,
            &[
                ("all", "All equipment"),
                ("warframe", "Warframes"),
                ("primary", "Primary"),
                ("secondary", "Secondary"),
                ("melee", "Melee"),
                ("archwing", "Archwings"),
                ("archgun", "Archguns"),
                ("archmelee", "Archmelee"),
                ("companion", "Companions"),
                ("necramech", "Necramechs"),
            ],
        )
    });
    for &id in list.rows.order().keys() {
        let (_, row) = list.rows.get(id).unwrap();
        ui.view_control(id, |ui, r| {
            if row.id == b.definition {
                ui.fill(r, 6.0, ACCENT);
            }
            let width = r.width as f32 / ui.scale;
            ui.image(ui.rect(5.0, 3.0, 37.0, 37.0), row.asset.as_ref());
            ui.text(
                ui.rect(49.0, 0.0, width - 56.0, 44.0),
                &row.name,
                13.0,
                TEXT,
            );
            ui.hit(format!("build:equipment:{}", row.id), r, &row.name, true);
        });
    }
    ui.view_control(BAR, |ui, r| {
        ui.scrollbar(
            "build:list-scroll",
            r,
            b.list_scroll.maximum(),
            b.list_offset as f32,
        )
    });
    ui.view_control(node("instance"), |ui, r| {
        ui.choice("build:instance", r, &b.instance, &choices(&data.copies))
    });
    for &id in list.configs.order().keys() {
        let (index, name) = list.configs.get(id).unwrap();
        ui.view_control(id, |ui, r| {
            ui.button(
                format!("build:config:{index}"),
                r,
                &name,
                b.config == index,
                true,
            )
        });
    }
    ui.view_control(node("new"), |ui, r| {
        ui.button("build:new", r, "Create group", true, data.owned)
    });
    ui.view_control(node("add"), |ui, r| {
        ui.button(
            "build:add-config",
            r,
            "Add configuration",
            false,
            data.owned,
        )
    });
    ui.view_control(node("find"), |ui, r| {
        ui.button("build:find", r, "Find builds", false, true)
    });
}
