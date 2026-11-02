use super::*;

#[derive(PartialEq)]
pub(super) struct DialogView {
    pub metrics: State<Metrics>,
    pub state: State<DialogState>,
    pub fonts: Fonts,
}

impl View for DialogView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let s = self.state.get();
        if s.kind.is_empty() {
            return empty();
        }
        let m = self.metrics.get();
        let rect = Bounds::new(
            (m.width - 380.0) / 2.0,
            (m.height - 238.0) / 2.0,
            380.0,
            238.0,
        );
        let c = |id, r| control(node(id), m.local(rect, r));
        let text_at = |text: &str, size, color, bold, r| {
            place(
                label(&self.fonts, text, size, color, bold, m.scale),
                m.local(rect, r),
            )
        };
        ui! {
            ZStack() {
                fill([0, 0, 0, 145], 0.0)
                place(ui! {
                    ZStack() {
                        fill(controls::POPOVER, 8.0)
                        text_at(&s.title, 20.0, TEXT, true, Bounds::new(16.0, 15.0, 348.0, 34.0))
                        match s.kind.as_str() {
                            "create" | "create-source" | "rename" => c("name", Bounds::new(16.0, 65.0, 348.0, 36.0)),
                            "source" | "config" => {
                                c("target", Bounds::new(16.0, 65.0, 348.0, 36.0))
                                if s.options.is_empty() {
                                    text_at("Create a group for this equipment first", 14.0, MUTED, false, Bounds::new(16.0, 111.0, 348.0, 42.0))
                                }
                                if s.kind == "source" {
                                    c("new", Bounds::new(16.0, 126.0, 160.0, 30.0))
                                }
                            },
                            _ => text_at("The saved group and its plan will be removed.", 14.0, MUTED, false, Bounds::new(16.0, 70.0, 348.0, 55.0)),
                        }
                        c("cancel", Bounds::new(16.0, 182.0, 162.0, 36.0))
                        c("confirm", Bounds::new(196.0, 182.0, 168.0, 36.0))
                    }.alignment(Alignment::Start)
                }, m.bounds(rect.x, rect.y, rect.width, rect.height)).role(Role::Overlay)
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
