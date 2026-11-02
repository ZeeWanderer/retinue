use super::{
    MOTION, composition::CENTER, composition::LEFT, composition::RIGHT, composition::page_key,
    dock_layout::DockLayout, navigation_view::NavigationView, paint_view::PaintView,
    shell_model::ShellModel, shell_model::Viewport,
};
use retinue_ui::{drawing::ExternalDrawing, prelude::*, semantics::Role};

#[derive(Clone, PartialEq)]
pub struct BodyView(pub ShellModel);

impl BodyView {
    pub fn new(model: ShellModel) -> Self {
        Self(model)
    }
}

impl View for BodyView {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        let model = &self.0;
        let v = cx.environment().get::<Viewport>();
        let animation = Animation::new(MOTION).easing(Easing::InOutCubic);
        let left = cx.animated_with("left", model.left.get(), animation);
        let right = cx.animated_with("right", model.right.get(), animation);
        let left = if model.compact_left.get() { 0.0 } else { left };
        let right = if model.right_available.get() {
            right
        } else {
            0.0
        };
        let left_width = 52.0 + 90.0 * left;
        let layout = DockLayout {
            width: v.width(),
            height: v.body(),
            inset: 6.0,
            left: left_width,
            left_gap: 7.0,
            right: 400.0 * right,
            right_gap: 7.0 * right,
            docked: model.right_fits.get(),
        };
        let page = page_key(model.page.get());
        ui! {
            LayoutView(layout) {
                NavigationView { model: model.clone(), width: left_width }.clipped().id(LEFT)
                Canvas(ExternalDrawing(page)).id(page)
                    .background { PaintView(model.paint(CENTER)) }
                    .clipped().role(Role::Content).id(CENTER)
                PaintView(model.paint(RIGHT)).clipped().id(RIGHT)
            }
        }
    }
}
