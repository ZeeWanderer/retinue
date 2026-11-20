use super::{
    Control, composition::LEFT, composition::control_key, paint_view::PaintView,
    shell_model::ShellModel, shell_model::Viewport, title_view::control,
};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};

#[derive(Clone, PartialEq)]
pub struct NavigationView {
    pub model: ShellModel,
    pub width: f32,
}

#[view]
impl View for NavigationView {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl Body {
        let v = cx.environment().get::<Viewport>();
        let width = self.width;
        let openness = ((width - 52.0) / 90.0).clamp(0.0, 1.0);
        let identity = 49.0 + 31.0 * openness;
        let start = 8.0 + identity + if width > 100.0 { 22.0 } else { 8.0 };
        let available = (v.body() - 60.0 - start).max(0.0);
        let step = (available / 6.0).min(49.0);
        let model = &self.model;
        compose(PaintView(model.paint(LEFT)))
            .overlay_aligned(
                AnchorPoint::TOP_LEADING,
                control(model, Control::Player)
                    .frame(frame((width - 6.0).max(0.0), identity))
                    .offset(3.0, 8.0)
                    .named(control_key(Control::Player)),
            )
            .overlay_aligned(
                AnchorPoint::TOP_LEADING,
                ui! {
                    VStack((step - 40.0).max(0.0)) {
                        ForEach([Control::Foundry, Control::Mastery, Control::Inventory,
                            Control::Relics, Control::Market, Control::Builds], |button| control_key(*button)) { |button|
                            control(model, button).frame(frame((width - 16.0).max(0.0), step.min(40.0)))
                                .named(control_key(button))
                        }
                    }.offset(8.0, start)
                },
            )
            .overlay_aligned(
                AnchorPoint::TOP_LEADING,
                control(model, Control::Settings)
                    .frame(frame((width - 16.0).max(0.0), 40.0))
                    .offset(8.0, (v.body() - 52.0).max(0.0))
                    .named(control_key(Control::Settings)),
            )
    }
}
