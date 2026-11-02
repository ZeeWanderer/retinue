use super::*;

#[derive(PartialEq)]
pub(in super::super) struct GroupsView {
    pub metrics: State<Metrics>,
    pub model: State<Rc<build_planner_model::Model>>,
    pub fonts: Fonts,
}

impl View for GroupsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        let model = self.model.get();
        let d = &model.groups;
        let c = |id, rect| control(node(id), rect);
        let centered = |text: &str, size, color| {
            fitted_label(text, self.fonts.regular.clone(), size, color, true, m.scale)
        };
        ui! {
            ZStack() {
                if d.choices.is_empty() {
                    place(centered(if d.loading { "Loading build groups..." } else { "No build groups" }, 24.0, TEXT),
                        m.bounds(30.0, empty_top(&m), m.width - 60.0, 38.0))
                    place(centered("Choose equipment and add the builds you want it to support.", 15.0, MUTED),
                        m.bounds(30.0, empty_top(&m) + 48.0, m.width - 60.0, 36.0))
                    c("equipment", m.bounds((m.width - 332.0) / 2.0, empty_top(&m) + 96.0, 170.0, 36.0))
                    c("discover", m.bounds((m.width - 332.0) / 2.0 + 182.0, empty_top(&m) + 96.0, 150.0, 36.0))
                } else {
                    c("group", m.bounds(14.0, 78.0, m.width - 300.0, 32.0))
                    c("rename", m.bounds(m.width - 276.0, 78.0, 90.0, 32.0))
                    c("delete", m.bounds(m.width - 176.0, 78.0, 80.0, 32.0))
                    c("calculate", m.bounds(m.width - 86.0, 78.0, 72.0, 32.0))
                    if d.selected {
                        c("instance", m.bounds(14.0, 119.0, (m.width - 28.0).min(420.0), 30.0))
                        for (i, (id, _)) in OPTIONS.into_iter().enumerate() {
                            c(id, m.bounds(14.0 + i as f32 * (m.width - 28.0) / 3.0,
                                157.0, (m.width - 28.0) / 3.0, 28.0))
                        }
                        c("member", m.bounds(14.0, 195.0, m.width - 234.0, 33.0))
                        c("remove", m.bounds(m.width - 210.0, 195.0, 90.0, 33.0))
                        c("planned", m.bounds(m.width - 110.0, 195.0, 96.0, 33.0))
                        c("add", m.bounds(14.0, 240.0, 160.0, 31.0))
                        c("find", m.bounds(184.0, 240.0, 120.0, 31.0))
                    } else {
                        place(label(&self.fonts, "Choose a group", 16.0, MUTED, false, m.scale),
                            m.bounds(24.0, 132.0, m.width - 48.0, 32.0))
                    }
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
