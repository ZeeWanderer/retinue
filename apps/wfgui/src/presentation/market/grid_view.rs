use super::*;

#[derive(PartialEq)]
pub(super) struct GridView {
    pub model: Model,
    pub fonts: Fonts,
}

#[view]
impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let m = self.model.metrics.get();
        place(
            ScrollViewport(compose(GridContentView {
                model: self.model.clone(),
                fonts: self.fonts.clone(),
            }))
            .content_insets(Insets {
                bottom: crate::presentation::style::SCROLL_CONTENT_INSET,
                ..Insets::default()
            })
            .position(0.0, self.model.offset.get()),
            m.viewport(),
        )
        .named(GRID)
        .reuse(CacheHint::Conditional)
    }
}

#[derive(PartialEq)]
struct GridContentView {
    model: Model,
    fonts: Fonts,
}

#[view]
impl View for GridContentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let m = self.model.metrics.get();
        let order = self.model.order.get();
        let grid = m.uniform_grid();
        let size = grid.extent(order.len()).expect("order extent");
        ui! {
            LayoutView(grid.arrange(order)) {
                ForEach(self.model.cells.order().keys().iter().copied(), |id| *id) { |id|
                compose(card::OrderCardView {
                    id,
                    card: self.model.cells.item(id).unwrap(),
                    metrics: self.model.metrics.clone(),
                    fonts: self.fonts.clone(),
                }).named(id)
                }
            }.frame(frame(size.width, size.height))
        }
    }
}
