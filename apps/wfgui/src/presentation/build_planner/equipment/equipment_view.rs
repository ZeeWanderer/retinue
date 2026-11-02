use super::*;

#[derive(PartialEq)]
pub(super) struct EquipmentView {
    pub metrics: State<Metrics>,
    pub model: State<Rc<build_planner_model::Model>>,
    pub fonts: Fonts,
    pub rows: Items<(usize, build_planner_model::EquipmentRow)>,
    pub configs: Items<(usize, String)>,
    pub offset: State<f32>,
    pub height: State<f32>,
}

impl View for EquipmentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        let model = self.model.get();
        let d = &model.equipment;
        let (x, w) = (m.sidebar + 10.0, m.width - m.sidebar - 22.0);
        let c = |id, rect| control(node(id), rect);
        ui! {
            ZStack() {
                place(slot(node("collapse.icon")), m.bounds(14.0, 77.0, 22.0, 22.0))
                c("collapse", m.bounds(8.0, 71.0, 34.0, 34.0))
                if m.sidebar > 100.0 {
                    c("search", m.bounds(45.0, 73.0, m.sidebar - 53.0, 32.0))
                    c("class", m.bounds(12.0, 113.0, m.sidebar - 24.0, 29.0))
                    place(ScrollViewport::new(ui! {
                        ZStack() {
                            for &id in self.rows.order().keys() {
                                let (i, _) = self.rows.get(id).unwrap();
                                control(id, m.bounds(0.0, i as f32 * 47.0, m.sidebar - 14.0, 44.0))
                            }
                        }.alignment(Alignment::Start).frame_aligned(m.equipment_list().width, self.height.get(), AnchorPoint::TOP_LEADING)
                    }).position(0.0, self.offset.get()), m.equipment_list()).id(LIST).reuse(CacheHint::Conditional)
                    control(BAR, m.equipment_list())
                }
                if d.selected {
                    place(label(&self.fonts, &d.name, 22.0, TEXT, true, m.scale), m.bounds(x, 72.0, w, 31.0))
                    c("instance", m.bounds(x, 113.0, w.min(300.0), 31.0))
                    for &id in self.configs.order().keys() {
                        let (index, _) = self.configs.get(id).unwrap();
                        let cell = (w / d.configs.len().max(1) as f32 - 6.0).min(92.0);
                        let order = d.configs.iter().position(|(i, _)| *i == index).unwrap_or(0);
                        control(id, m.bounds(x + order as f32 * (cell + 6.0), 153.0, cell, 30.0))
                    }
                    c("new", m.bounds(x, 195.0, 125.0, 31.0))
                    c("add", m.bounds(x + 134.0, 195.0, 150.0, 31.0))
                    c("find", m.bounds(x + 293.0, 195.0, 110.0, 31.0))
                } else {
                    place(label(&self.fonts, if d.loading { "Loading equipment..." } else { "Choose equipment" },
                        18.0, MUTED, false, m.scale), m.bounds(x + 18.0, 100.0, w - 36.0, 40.0))
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
