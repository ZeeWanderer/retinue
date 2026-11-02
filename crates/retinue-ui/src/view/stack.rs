use super::{Element, IntoChildren, IntoElement, Kind, LayoutDescription, LayoutKind};
use crate::{
    layout::{Layout, LayoutAlgorithm},
    scene::Algorithm,
};
use taffy::{AlignItems, FlexDirection, LengthPercentage, Size, Style};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment {
    Start,
    #[default]
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Distribution {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Distribution {
    fn taffy(self) -> taffy::JustifyContent {
        match self {
            Self::Start => taffy::JustifyContent::START,
            Self::Center => taffy::JustifyContent::CENTER,
            Self::End => taffy::JustifyContent::END,
            Self::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
            Self::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
            Self::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
        }
    }
}

impl Alignment {
    pub(crate) fn taffy(self) -> AlignItems {
        match self {
            Self::Start => AlignItems::START,
            Self::Center => AlignItems::CENTER,
            Self::End => AlignItems::END,
            Self::Stretch => AlignItems::STRETCH,
        }
    }
}

pub struct HStack(LayoutDescription);
pub struct VStack(LayoutDescription);
pub struct ZStack(LayoutDescription);

macro_rules! stack {
    ($name:ident, $direction:ident) => {
        impl $name {
            pub fn new(spacing: f32, children: impl IntoChildren) -> Self {
                Self(LayoutDescription {
                    kind: LayoutKind::$name,
                    style: Style {
                        flex_direction: FlexDirection::$direction,
                        align_items: Some(AlignItems::CENTER),
                        flex_shrink: 0.0,
                        gap: Size {
                            width: LengthPercentage::length(spacing),
                            height: LengthPercentage::length(spacing),
                        },
                        ..Style::default()
                    },
                    algorithm: Algorithm::Flex,
                    children: children.into_children(),
                    properties: Default::default(),
                })
            }

            pub fn spacing(mut self, spacing: f32) -> Self {
                self.0.style.gap = Size {
                    width: LengthPercentage::length(spacing),
                    height: LengthPercentage::length(spacing),
                };
                self
            }

            pub fn alignment(mut self, alignment: Alignment) -> Self {
                self.0.style.align_items = Some(alignment.taffy());
                self
            }

            pub fn distribution(mut self, distribution: Distribution) -> Self {
                self.0.style.justify_content = Some(distribution.taffy());
                self
            }
        }

        impl IntoElement for $name {
            fn into_element(self) -> Element {
                Element::new(Kind::Layout(self.0))
            }
        }
    };
}

stack!(HStack, Row);
stack!(VStack, Column);

impl ZStack {
    pub fn new(children: impl IntoChildren) -> Self {
        Self(LayoutDescription {
            kind: LayoutKind::ZStack,
            style: Style {
                align_items: Some(AlignItems::CENTER),
                justify_items: Some(AlignItems::CENTER),
                ..Style::default()
            },
            algorithm: Algorithm::Custom(LayoutAlgorithm::new(super::stack_layout::StackLayout {
                alignment: Alignment::Center,
            })),
            children: children.into_children(),
            properties: Default::default(),
        })
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.0.algorithm =
            Algorithm::Custom(LayoutAlgorithm::new(super::stack_layout::StackLayout {
                alignment,
            }));
        self
    }
}

impl IntoElement for ZStack {
    fn into_element(self) -> Element {
        Element::new(Kind::Layout(self.0))
    }
}

pub struct LayoutView(LayoutDescription);

impl LayoutView {
    pub fn new<L: Layout>(layout: L, children: impl IntoChildren) -> Self {
        Self::with_algorithm(LayoutAlgorithm::new(layout), children)
    }

    pub fn with_algorithm(layout: LayoutAlgorithm, children: impl IntoChildren) -> Self {
        Self(LayoutDescription {
            kind: LayoutKind::Custom,
            style: Style::default(),
            algorithm: Algorithm::Custom(layout),
            children: children.into_children(),
            properties: Default::default(),
        })
    }
}

impl IntoElement for LayoutView {
    fn into_element(self) -> Element {
        Element::new(Kind::Layout(self.0))
    }
}
