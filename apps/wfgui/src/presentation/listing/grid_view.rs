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
        .named(VIEWPORT)
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
        let model = &self.model;
        let m = model.metrics.get();
        let order = model.order.get();
        let grid = m.uniform_grid();
        let extent = grid.extent(order.len()).expect("listing extent");
        ui! {
            LayoutView(grid.arrange(order)) {
                ForEach(model.cells.order().keys().iter().copied(), |id| *id) { |id|
                    let cell = model.cells.item(id).expect("visible cell");
                    Group() {
                    match m.page {
                    Page::Inventory => compose(inventory::InventoryCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        fonts: self.fonts.clone(),
                    }),
                    Page::Mastery => compose(mastery::MasteryCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        context: model.context.clone(),
                        fonts: self.fonts.clone(),
                    }),
                    Page::Relics => compose(relics::RelicCardView {
                        id,
                        cell,
                        metrics: model.metrics.clone(),
                        context: model.context.clone(),
                        fonts: self.fonts.clone(),
                    }),
                    _ => {},
                    }
                    }.named(id)
                }
            }.frame(frame(extent.width, extent.height)).named(CONTENT)
        }
    }
}
