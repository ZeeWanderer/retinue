use super::{
    Control, composition::TITLE, composition::control_key, paint_view::PaintView,
    shell_model::ShellModel,
};
use crate::presentation::widgets::compiled::frame;
use retinue_ui::{
    authoring::*,
    layout::{Insets, anchors::AnchorPoint},
    semantics::Role,
    semantics::WindowAction,
};

#[derive(Clone, PartialEq)]
pub struct TitleView(pub ShellModel);

pub fn control(model: &ShellModel, control: Control) -> impl Body + use<> {
    compose(PaintView(model.paint(control_key(control))))
        .role(Role::Control)
        .interactive()
        .window_action(match control {
            Control::Minimize => Some(WindowAction::Minimize),
            Control::Maximize => Some(WindowAction::ToggleMaximized),
            Control::Close => Some(WindowAction::Close),
            _ => None,
        })
}

#[view]
impl View for TitleView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let model = &self.0;
        compose(PaintView(model.paint(TITLE)))
            .overlay_aligned(
                AnchorPoint::TOP_LEADING,
                control(model, Control::Left)
                    .frame(frame(30.0, 30.0))
                    .offset(36.0, 5.0)
                    .named(control_key(Control::Left)),
            )
            .overlay_aligned(
                AnchorPoint::TOP_TRAILING,
                ui! {
                    HStack(0.0) {
                        ForEach([
                            (Control::Smaller, 25.0, 38.0), (Control::Larger, 25.0, 14.0),
                            (Control::Right, 30.0, 7.0), (Control::Minimize, 30.0, 0.0),
                            (Control::Maximize, 30.0, 0.0), (Control::Close, 30.0, 0.0),
                        ], |(button, _, _)| control_key(*button)) { |(button, width, gap)|
                            let top = (40.0 - width) / 2.0;
                            control(model, button).frame(frame(width, width))
                                .named(control_key(button))
                                .padding(Insets { top, right: gap,
                                    bottom: top, left: 0.0 })
                        }
                    }.frame(Frame::new().height(40.0))
                        .padding(Insets { right: 10.0, ..Insets::default() })
                },
            )
            .role(Role::Title)
            .window_action(WindowAction::Move)
    }
}
