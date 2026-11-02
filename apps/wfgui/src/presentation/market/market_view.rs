use super::*;

#[derive(PartialEq)]
pub(super) struct MarketPageView {
    pub model: Model,
    pub fonts: Fonts,
    pub dialog: Element,
}

impl View for MarketPageView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.get();
        let account = self.model.account.get();
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), Bounds::new(0.0, 10.0, m.width, (m.height - 10.0).max(0.0)))
                if account.authenticated == Some(true) {
                    toolbar::ToolbarView { model: self.model.clone(), fonts: self.fonts.clone() }
                    grid_view::GridView { model: self.model.clone(), fonts: self.fonts.clone() }
                    place(slot(SCROLL), m.viewport()).interactive()
                    if self.model.order.get().is_empty() {
                        place(fitted_label("No matching orders", self.fonts.regular.clone(), 17.0, MUTED, false, m.scale),
                            m.bounds(24.0, 210.0, m.width - 48.0, 35.0))
                    }
                    if let Some(error) = &account.error {
                        place(fitted_label(error, self.fonts.regular.clone(), 13.0, BAD, false, m.scale),
                            m.bounds(14.0, m.height - 30.0, m.width - 28.0, 26.0))
                    }
                    self.dialog.clone()
                } else {
                    account::AccountView { model: self.model.clone(), fonts: self.fonts.clone() }
                }
            }.alignment(Alignment::Start).frame(m.width, m.height)
        }
    }
}
