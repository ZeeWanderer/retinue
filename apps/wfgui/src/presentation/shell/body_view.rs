use super::{
    MOTION, composition::CENTER, composition::LEFT, composition::RIGHT, composition::page_key,
    dock_layout::DockLayout, navigation_view::NavigationView, paint_view::PaintView,
    shell_model::ShellModel, shell_model::Viewport,
};
use retinue_ui::{
    authoring::*,
    drawing::ExternalDrawing,
    semantics::Role,
    transition::{Animation, Easing},
};

#[derive(Clone, PartialEq)]
pub struct BodyView(pub ShellModel);

#[view]
impl View for BodyView {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl Body {
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
                compose(NavigationView { model: model.clone(), width: left_width }).clipped().named(LEFT)
                Canvas(ExternalDrawing(page)).named(page)
                    .background { compose(PaintView(model.paint(CENTER))) }
                    .clipped().role(Role::Content).named(CENTER)
                compose(PaintView(model.paint(RIGHT))).clipped().named(RIGHT)
            }
        }
    }
}
