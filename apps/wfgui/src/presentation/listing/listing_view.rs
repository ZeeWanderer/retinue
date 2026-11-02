use super::*;

#[derive(PartialEq)]
pub(super) struct ListingPageView {
    pub model: Model,
    pub fonts: Fonts,
    pub summary: Element,
    pub overlays: Element,
}

impl View for ListingPageView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.get();
        let (message, error) = self.model.message.get();
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), Bounds::new(0.0, 10.0, m.width, (m.height - 10.0).max(0.0)))
                if m.page == Page::Mastery { self.summary.clone() }
                toolbar::ToolbarView { model: self.model.clone(), fonts: self.fonts.clone() }
                place(empty(), m.bounds(10.0, 12.0, m.width - 20.0, m.grid_y() - 12.0))
                    .id(TOOLBAR).role(retinue_ui::semantics::Role::Toolbar)
                grid_view::GridView { model: self.model.clone(), fonts: self.fonts.clone() }
                if self.model.order.get().is_empty() {
                    place(fitted_label(message, self.fonts.regular.clone(), 16.0,
                        if error { BAD } else { MUTED }, false, m.scale),
                        m.bounds(24.0, m.grid_y() + 25.0, m.width - 48.0, 40.0))
                    if error {
                        place(slot(RETRY), m.bounds(24.0, m.grid_y() + 72.0, 100.0, 32.0)).interactive()
                    }
                }
                place(slot(SCROLL), m.scroll_area()).interactive()
                self.overlays.clone()
            }.alignment(Alignment::Start).frame(m.width, m.height)
        }
    }
}
