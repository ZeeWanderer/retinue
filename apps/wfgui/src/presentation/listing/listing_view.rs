use super::*;

#[derive(PartialEq)]
pub(super) struct ListingPageView<S, G, O> {
    pub model: Model,
    pub fonts: Fonts,
    pub summary: S,
    pub grid: G,
    pub overlays: O,
}

#[view]
impl<S: View + Clone + PartialEq, G: View + Clone + PartialEq, O: View + Clone + PartialEq> View
    for ListingPageView<S, G, O>
{
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let m = self.model.metrics.get();
        let (message, error) = self.model.message.get();
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), Bounds::new(0.0, 10.0, m.width, (m.height - 10.0).max(0.0)))
                if m.page == Page::Mastery { compose(self.summary.clone()) }
                compose(toolbar::ToolbarView { model: self.model.clone(), fonts: self.fonts.clone() })
                place(empty(), m.bounds(10.0, 12.0, m.width - 20.0, m.grid_y() - 12.0))
                    .named(TOOLBAR).role(retinue_ui::semantics::Role::Toolbar)
                compose(self.grid.clone())
                if self.model.order.get().is_empty() {
                    place(label(message, self.fonts.regular.clone(), 16.0,
                        if error { BAD } else { MUTED }, false),
                        m.bounds(24.0, m.grid_y() + 25.0, m.width - 48.0, 40.0))
                    if error {
                        place(slot(RETRY), m.bounds(24.0, m.grid_y() + 72.0, 100.0, 32.0)).interactive()
                    }
                }
                place(slot(SCROLL), m.scroll_area()).interactive()
                compose(self.overlays.clone())
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, m.height))
        }
    }
}
