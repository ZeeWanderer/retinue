use super::*;

#[derive(PartialEq)]
pub(super) struct BuildPageView {
    pub metrics: State<Metrics>,
    pub state: State<PageState>,
    pub fonts: Fonts,
    pub equipment: Element,
    pub discover: Element,
    pub groups: Element,
    pub content: Element,
    pub previews: Element,
    pub dialog: Element,
}

impl View for BuildPageView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        let s = self.state.get();
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), m.bounds(0.0, 10.0, m.width, m.height - 10.0))
                ZStack() {
                    place(slot(node("warning.icon")), m.bounds(17.0, 17.0, 18.0, 18.0))
                    control(node("warning"), m.bounds(10.0, 10.0, 30.0, 30.0))
                    for (i, id) in ["groups", "equipment", "discover"].into_iter().enumerate() {
                        control(node(id), m.bounds(
                            46.0 + m.tabs[..i].iter().map(|w| w + 6.0).sum::<f32>(),
                            10.0, m.tabs[i], 32.0))
                    }
                    control(node("account"), m.bounds(m.width - 146.0, 10.0, 132.0, 32.0))
                }.alignment(Alignment::Start).frame_aligned(m.width, 42.0, AnchorPoint::TOP_LEADING).role(Role::Toolbar)
                match s.tab.as_str() {
                    "equipment" => self.equipment.clone(),
                    "discover" => self.discover.clone(),
                    _ => self.groups.clone(),
                }
                self.content.clone()
                if let Some(error) = &s.error {
                    place(label(&self.fonts, error, 13.0, BAD, false, m.scale), m.bounds(16.0, m.height - 28.0, m.width - 32.0, 24.0))
                }
                if let Some(notice) = &s.notice {
                    place(label(&self.fonts, notice, 13.0, GOOD, false, m.scale), m.bounds(16.0, 48.0, m.width - 32.0, 24.0))
                }
                self.previews.clone()
                self.dialog.clone()
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
