use retinue_render::{Bounds, Color, Key, painter::CachedFont};
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};
use std::rc::Rc;

#[cfg(test)]
mod tests;

pub fn slot(key: Key) -> impl Body {
    compose(crate::presentation::controls::view_host::paint_slot::PaintSlot(key))
}

pub fn place(view: impl Body, bounds: Bounds) -> impl Body {
    view.frame(
        Frame::new()
            .width(bounds.width)
            .height(bounds.height)
            .alignment(AnchorPoint::LEADING),
    )
    .offset(bounds.x, bounds.y)
}

pub fn frame(width: f32, height: f32) -> Frame {
    Frame::new()
        .width(width)
        .height(height)
        .alignment(AnchorPoint::TOP_LEADING)
}

pub fn fill(color: [u8; 4], radius: f32) -> retinue_ui::view::Rectangle {
    Rectangle(Color::from_bgra(color)).corner_radius(radius)
}

pub fn label(
    text: impl AsRef<str>,
    font: Rc<CachedFont>,
    size: f32,
    color: [u8; 4],
    center: bool,
) -> Framed<Text> {
    Text(text.as_ref())
        .font(font)
        .font_size(size)
        .color(Color::from_bgra(color))
        .line_limit(1)
        .alignment(if center {
            retinue_ui::drawing::text::TextAlignment::Center
        } else {
            retinue_ui::drawing::text::TextAlignment::Leading
        })
        .frame(Frame::new().fill_width().alignment(if center {
            AnchorPoint::CENTER
        } else {
            AnchorPoint::LEADING
        }))
}

pub fn aligned_label(
    text: impl AsRef<str>,
    font: Rc<CachedFont>,
    size: f32,
    color: [u8; 4],
    alignment: f32,
) -> Framed<Text> {
    use retinue_ui::drawing::text::TextAlignment;
    Text(text.as_ref())
        .font(font)
        .font_size(size)
        .color(Color::from_bgra(color))
        .line_limit(1)
        .alignment(if alignment >= 1.0 {
            TextAlignment::Trailing
        } else if alignment > 0.0 {
            TextAlignment::Center
        } else {
            TextAlignment::Leading
        })
        .frame(
            Frame::new()
                .fill_width()
                .fill_height()
                .alignment(if alignment >= 1.0 {
                    AnchorPoint::TRAILING
                } else if alignment > 0.0 {
                    AnchorPoint::CENTER
                } else {
                    AnchorPoint::LEADING
                }),
        )
}
