use super::{foundry_model::Model, grid_content_view::GridContentView};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub struct GridView {
    pub model: Model,
}
#[retinue_ui::authoring::view]
impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let model = &self.model;
        let viewport = model.viewport.get();
        let offset = model.offset.get();
        ScrollViewport::new(compose(GridContentView {
            model: model.clone(),
        }))
        .axes(ScrollAxes::Both)
        .position(-offset.0, -offset.1)
        .frame(frame(viewport.width, viewport.height))
        .offset(viewport.x, viewport.y)
    }
}
