mod discover_view;
use super::*;
fn node(id: impl std::hash::Hash) -> Key {
    key("build.discover", id)
}
const BAR: Key = Key::new("build.discover.scroll", 0);
pub(super) struct List {
    rows: Items<(usize, build_planner_model::SourceRow)>,
    offset: State<f32>,
}
impl List {
    pub fn view(
        &self,
        metrics: State<Metrics>,
        model: State<Rc<build_planner_model::Model>>,
        state: State<PageState>,
        fonts: Fonts,
    ) -> Element {
        discover_view::DiscoverView {
            metrics,
            model,
            state,
            fonts,
            rows: self.rows.clone(),
            offset: self.offset.clone(),
        }
        .into_element()
    }

    pub fn new() -> Self {
        Self {
            rows: Items::default(),
            offset: State::new(0.0),
        }
    }
    pub fn sync(
        &self,
        m: &Metrics,
        data: &build_planner_model::Discover,
        b: &mut Builds,
        now: Instant,
    ) {
        if b.tab != "discover" || data.detail {
            self.rows.replace([]).unwrap();
            return;
        }
        let viewport = m.source_list();
        b.scroll.resize(
            ((data.rows.len() as f32 * 74.0 + 20.0) * m.scale).round(),
            viewport.height * m.scale,
            now,
        );
        b.offset = b.scroll.offset(now).round() as u32;
        self.offset.set(b.offset as f32 / m.scale);
        let offset = self.offset.get();
        self.rows
            .replace(
                visible_rows(offset, viewport.height, 74.0, data.rows.len())
                    .map(|i| (i, &data.rows[i]))
                    .map(|(i, r)| (node(("row", &r.id)), (i, r.clone()))),
            )
            .expect("unique source rows");
    }
}
pub(super) fn bind(
    ui: &mut ControlHost,
    b: &Builds,
    d: &build_planner_model::Discover,
    list: &List,
) {
    if b.tab != "discover" {
        return;
    }
    ui.view_control(node("query"), |ui, r| {
        ui.search_field(
            "build:query",
            r,
            &b.query,
            "Find equipment or build",
            crate::presentation::widgets::search::Anchor::Leading,
        );
    });
    ui.view_control(node("scope"), |ui, r| {
        ui.choice(
            "build:scope",
            r,
            &b.scope,
            &[
                ("public", "Public builds"),
                ("favorites", "My favorites"),
                ("user", "My builds"),
            ],
        )
    });
    ui.view_control(node("search"), |ui, r| {
        ui.button("build:search-source", r, "Search", true, true)
    });
    ui.view_control(node("ordering"), |ui, r| {
        ui.choice(
            "build:ordering",
            r,
            &b.ordering,
            &[
                ("score", "Top rated"),
                ("updated_at", "Recently updated"),
                ("created_at", "Newest"),
            ],
        )
    });
    ui.view_control(node("back"), |ui, r| {
        ui.button("build:back", r, "Back", false, true)
    });
    ui.view_control(node("added"), |ui, r| {
        ui.button("build:open-added", r, "Open group", false, true)
    });
    ui.view_control(node("add"), |ui, r| {
        ui.button("build:add-source", r, "Add to group", true, true)
    });
    ui.view_control(node("website"), |ui, r| {
        ui.button("build:website", r, "Overframe", false, true)
    });
    for &id in list.rows.order().keys() {
        let (_, row) = list.rows.get(id).unwrap();
        ui.view_control(id, |ui, r| {
            ui.fill(r, 6.0, controls::CARD);
            let width = r.width as f32 / ui.scale;
            ui.bold(
                ui.rect(12.0, 5.0, width - 24.0, 28.0),
                &row.title,
                16.0,
                TEXT,
            );
            ui.text(
                ui.rect(12.0, 34.0, width - 24.0, 24.0),
                &row.subtitle,
                13.0,
                MUTED,
            );
            ui.hit(&row.action, r, &row.title, true);
        });
    }
    ui.view_control(BAR, |ui, r| {
        ui.scrollbar("build:scroll", r, b.scroll.maximum(), b.offset as f32)
    });
    ui.view_control(node("previous"), |ui, r| {
        ui.button("build:previous", r, "Previous", false, b.source_offset > 0)
    });
    ui.view_control(node("next"), |ui, r| {
        ui.button("build:next", r, "Next", false, d.rows.len() >= 30)
    });
}
