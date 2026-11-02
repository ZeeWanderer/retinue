use super::{
    Control, composition::TITLE, composition::control_key, paint_view::PaintView,
    shell_model::ShellModel,
};
use retinue_ui::{
    layout::anchors::AnchorPoint, prelude::*, semantics::Role, semantics::WindowAction,
    view::Insets,
};

#[derive(Clone, PartialEq)]
pub struct TitleView(pub ShellModel);

impl TitleView {
    pub fn new(model: ShellModel) -> Self {
        Self(model)
    }
}

pub fn control(model: &ShellModel, control: Control) -> Element {
    let view = PaintView(model.paint(control_key(control)))
        .role(Role::Control)
        .interactive();
    match control {
        Control::Minimize => view.window_action(WindowAction::Minimize),
        Control::Maximize => view.window_action(WindowAction::ToggleMaximized),
        Control::Close => view.window_action(WindowAction::Close),
        _ => view,
    }
}

impl View for TitleView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let model = &self.0;
        PaintView(model.paint(TITLE))
            .overlay_aligned(
                AnchorPoint::TOP_LEADING,
                control(model, Control::Left)
                    .frame(30.0, 30.0)
                    .offset(36.0, 5.0)
                    .id(control_key(Control::Left)),
            )
            .overlay_aligned(
                AnchorPoint::TOP_TRAILING,
                ui! {
                    HStack(0.0) {
                        for (button, width, gap) in [
                            (Control::Smaller, 25.0, 38.0), (Control::Larger, 25.0, 14.0),
                            (Control::Right, 30.0, 7.0), (Control::Minimize, 30.0, 0.0),
                            (Control::Maximize, 30.0, 0.0), (Control::Close, 30.0, 0.0),
                        ] {
                            let top = (40.0 - width) / 2.0;
                            control(model, button).frame(width, width)
                                .id(control_key(button))
                                .padding(Insets { top, right: gap,
                                    bottom: top, left: 0.0 })
                        }
                    }.height(40.0)
                        .padding(Insets { right: 10.0, ..Insets::default() })
                },
            )
            .role(Role::Title)
            .window_action(WindowAction::Move)
    }
}
