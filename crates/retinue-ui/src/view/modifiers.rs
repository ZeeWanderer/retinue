use super::{Element, IntoElement, Kind, LayoutDescription, LayoutKind, environment};
use super::{decoration::Decoration, properties::Properties};
use crate::{Key, environment::EnvironmentKey, scene::Algorithm};
use crate::{
    layout::{LayoutAlgorithm, anchors::AnchorPoint, values::LayoutValueKey},
    semantics::{Role, WindowAction},
};
use retinue_render::CacheHint;
use taffy::{AlignItems, Dimension, Size, Style};

pub use crate::layout::Insets;

fn frame(content: Element, style: Style) -> Element {
    aligned_frame(content, style, AnchorPoint::CENTER)
}

fn aligned_frame(content: Element, style: Style, alignment: AnchorPoint) -> Element {
    Element::new(Kind::Layout(LayoutDescription {
        kind: LayoutKind::Frame,
        algorithm: Algorithm::Custom(LayoutAlgorithm::new(super::frame::FrameLayout {
            alignment,
        })),
        style: Style {
            align_items: Some(AlignItems::CENTER),
            justify_items: Some(AlignItems::CENTER),
            flex_shrink: 0.0,
            ..style
        },
        children: vec![content],
        properties: Properties::default(),
    }))
}

fn properties(content: Element, properties: Properties) -> Element {
    Element::new(Kind::Layout(LayoutDescription {
        kind: LayoutKind::Properties,
        style: Style::default(),
        algorithm: Algorithm::Custom(LayoutAlgorithm::new(super::frame::FrameLayout {
            alignment: AnchorPoint::TOP_LEADING,
        })),
        children: vec![content],
        properties,
    }))
}

pub trait ViewExt: IntoElement + Sized {
    fn animation<T: PartialEq + 'static>(
        self,
        animation: impl Into<Option<crate::transition::Animation>>,
        trigger: T,
    ) -> Element {
        let mut element = self.into_element();
        element.animation = Some(crate::motion::AnimationScope::new(
            animation.into(),
            trigger,
        ));
        element
    }

    fn input(self, input: crate::input::InputOptions) -> Element {
        let mut element = Element::new(Kind::Layout(LayoutDescription {
            kind: LayoutKind::Input,
            style: Style::default(),
            algorithm: Algorithm::Custom(LayoutAlgorithm::new(super::frame::FrameLayout {
                alignment: AnchorPoint::TOP_LEADING,
            })),
            children: vec![self.into_element()],
            properties: Properties::default(),
        }));
        element.input = Some(input);
        element.attributes.hit = true;
        element
    }

    fn on_event(self, handler: crate::input::EventHandler) -> Element {
        self.input(crate::input::InputOptions::new(handler))
    }

    fn id(self, key: impl Into<Key>) -> Element {
        let mut element = self.into_element();
        element.key = Some(key.into());
        element
    }

    fn frame(self, width: f32, height: f32) -> Element {
        frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::length(width),
                    height: Dimension::length(height),
                },
                ..Style::default()
            },
        )
    }

    fn frame_aligned(self, width: f32, height: f32, alignment: AnchorPoint) -> Element {
        aligned_frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::length(width),
                    height: Dimension::length(height),
                },
                ..Style::default()
            },
            alignment,
        )
    }

    fn width(self, width: f32) -> Element {
        frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::length(width),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
        )
    }

    fn height(self, height: f32) -> Element {
        frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::auto(),
                    height: Dimension::length(height),
                },
                ..Style::default()
            },
        )
    }

    fn fill_width(self) -> Element {
        frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::percent(1.0),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
        )
    }

    fn fill_height(self) -> Element {
        frame(
            self.into_element(),
            Style {
                size: Size {
                    width: Dimension::auto(),
                    height: Dimension::percent(1.0),
                },
                ..Style::default()
            },
        )
    }

    fn padding(self, insets: impl Into<Insets>) -> Element {
        let insets = insets.into();
        Element::new(Kind::Layout(LayoutDescription {
            kind: LayoutKind::Padding,
            style: Style {
                padding: taffy::Rect {
                    left: taffy::LengthPercentage::length(insets.left),
                    right: taffy::LengthPercentage::length(insets.right),
                    top: taffy::LengthPercentage::length(insets.top),
                    bottom: taffy::LengthPercentage::length(insets.bottom),
                },
                flex_shrink: 0.0,
                ..Style::default()
            },
            algorithm: Algorithm::Custom(LayoutAlgorithm::new(super::frame::FrameLayout {
                alignment: AnchorPoint::TOP_LEADING,
            })),
            children: vec![self.into_element()],
            properties: Properties::default(),
        }))
    }

    fn background(self, background: impl IntoElement) -> Element {
        decorate(
            vec![background.into_element(), self.into_element()],
            1,
            AnchorPoint::CENTER,
        )
    }

    fn overlay(self, overlay: impl IntoElement) -> Element {
        self.overlay_aligned(AnchorPoint::CENTER, overlay)
    }

    fn overlay_aligned(self, alignment: AnchorPoint, overlay: impl IntoElement) -> Element {
        decorate(
            vec![self.into_element(), overlay.into_element()],
            0,
            alignment,
        )
    }

    fn offset(self, x: f32, y: f32) -> Element {
        properties(
            self.into_element(),
            Properties {
                offset: (x, y),
                ..Properties::default()
            },
        )
    }

    fn clipped(self) -> Element {
        properties(
            self.into_element(),
            Properties {
                clip: true,
                ..Properties::default()
            },
        )
    }

    fn hit_testing(self, enabled: bool) -> Element {
        let mut element = self.into_element();
        element.attributes.hit_testing = enabled;
        element
    }

    fn interactive(self) -> Element {
        let mut element = self.into_element();
        element.attributes.hit = true;
        element
    }

    fn role(self, role: Role) -> Element {
        let mut element = self.into_element();
        element.attributes.role = Some(role);
        element
    }

    fn window_action(self, action: WindowAction) -> Element {
        let mut element = self.into_element();
        element.attributes.window_action = Some(action);
        element.attributes.hit = true;
        element
    }

    fn reuse(self, hint: CacheHint) -> Element {
        let mut element = self.into_element();
        element.attributes.reuse = Some(hint);
        element
    }

    fn layout_value<K: LayoutValueKey>(self, value: K::Value) -> Element {
        let mut element = self.into_element();
        element.layout_values = element.layout_values.with::<K>(value);
        element
    }

    fn environment<K: EnvironmentKey>(self, value: K::Value) -> Element
    where
        K::Value: PartialEq,
    {
        environment::value::<K>(value, self.into_element())
    }

    fn environment_model<T: Clone + PartialEq + 'static>(self, model: T) -> Element {
        environment::model(model, self.into_element())
    }
}

impl<T: IntoElement> ViewExt for T {}

fn decorate(children: Vec<Element>, primary: usize, alignment: AnchorPoint) -> Element {
    Element::new(Kind::Layout(LayoutDescription {
        kind: LayoutKind::Decoration,
        style: Style::default(),
        children,
        algorithm: Algorithm::Custom(LayoutAlgorithm::new(Decoration { primary, alignment })),
        properties: Properties::default(),
    }))
}
