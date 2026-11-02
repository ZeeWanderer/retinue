use super::{
    body_view::BodyView, composition::BODY, composition::TITLE, composition::TOOLTIP,
    paint_view::PaintView, shell_model::ShellModel, shell_model::Viewport, title_view::TitleView,
};
use retinue_ui::{prelude::*, semantics::Role};

#[derive(Clone, PartialEq)]
pub struct ShellView(pub ShellModel);

impl View for ShellView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let model = &self.0;
        let v = model.viewport.get();
        ui! {
            VStack(6.0) {
                TitleView(model.clone()).frame(v.width(), 40.0).id(TITLE)
                BodyView(model.clone()).id(BODY).frame(v.width(), v.body())
            }
            .frame(v.width(), v.height())
            .overlay {
                if model.tooltip.get() {
                    PaintView(model.paint(TOOLTIP)).id(TOOLTIP).role(Role::Overlay)
                }
            }
            .role(Role::Window)
            .environment::<Viewport>(v)
        }
    }
}
