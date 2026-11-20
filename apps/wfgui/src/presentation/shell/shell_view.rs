use super::{
    body_view::BodyView, composition::BODY, composition::TITLE, composition::TOOLTIP,
    paint_view::PaintView, shell_model::ShellModel, shell_model::Viewport, title_view::TitleView,
};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::{authoring::*, semantics::Role};

#[derive(Clone, PartialEq)]
pub struct ShellView(pub ShellModel);

#[view]
impl View for ShellView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let model = &self.0;
        let v = model.viewport.get();
        ui! {
            VStack(6.0) {
                compose(TitleView(model.clone())).frame(frame(v.width(), 40.0)).named(TITLE)
                compose(BodyView(model.clone())).named(BODY).frame(frame(v.width(), v.body()))
            }
            .frame(frame(v.width(), v.height()))
            .overlay {
                if model.tooltip.get() {
                    compose(PaintView(model.paint(TOOLTIP))).named(TOOLTIP).role(Role::Overlay)
                }
            }
            .role(Role::Window)
            .environment::<Viewport>(v)
        }
    }
}
