use super::{
    Body, Children, Compiler, Distribution, NodeSchema, Resolution, Select, ViewContext,
    compiler::{First, Second},
};
use crate::{
    ir::{Kind, Property, Result, Value},
    layout::{Layout, LayoutAlgorithm},
    scene::Algorithm,
};
use taffy::{AlignItems, FlexDirection, LengthPercentage, Size, Style};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Leading,
    #[default]
    Center,
    Trailing,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum VerticalAlignment {
    Top,
    #[default]
    Center,
    Bottom,
    FirstTextBaseline,
}

impl HorizontalAlignment {
    fn taffy(self) -> AlignItems {
        match self {
            Self::Leading => AlignItems::START,
            Self::Center => AlignItems::CENTER,
            Self::Trailing => AlignItems::END,
        }
    }
}
impl VerticalAlignment {
    fn taffy(self) -> AlignItems {
        match self {
            Self::Top => AlignItems::START,
            Self::Center => AlignItems::CENTER,
            Self::Bottom => AlignItems::END,
            Self::FirstTextBaseline => AlignItems::BASELINE,
        }
    }
}

macro_rules! stack {
    ($name:ident, $direction:ident, $alignment:ident, $doc:literal) => {
        #[doc = $doc]
        pub struct $name<C> {
            spacing: f32,
            alignment: $alignment,
            stretch: bool,
            distribution: Distribution,
            children: C,
        }
        impl<C: Children> $name<C> {
            pub fn new(spacing: f32, children: C) -> Self {
                Self {
                    spacing,
                    alignment: $alignment::default(),
                    stretch: false,
                    distribution: Distribution::Start,
                    children,
                }
            }
            /// Changes the gap between adjacent children without adding outer padding.
            pub fn spacing(mut self, spacing: f32) -> Self {
                self.spacing = spacing;
                self
            }
            /// Aligns children on the axis perpendicular to the stack's direction.
            pub fn alignment(mut self, alignment: $alignment) -> Self {
                self.alignment = alignment;
                self
            }
            /// Uses stretch instead of cross-axis alignment for the children.
            pub fn stretch_children(mut self) -> Self {
                self.stretch = true;
                self
            }
            /// Distributes extra space along the stack's main axis.
            pub fn distribution(mut self, distribution: Distribution) -> Self {
                self.distribution = distribution;
                self
            }
        }
        impl<C: Children> Body for $name<C> {
            type Values = ((f32, $alignment, bool, Distribution), C::Values);
            const SCHEMA: NodeSchema = NodeSchema {
                kind: Kind::Layout,
                children: C::SCHEMAS,
            };
            const NODES: usize = 1 + C::NODES;
            const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));
            fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
                let node = compiler.node();
                compiler.bind::<First<S>>(
                    node,
                    Property::Layout,
                    |&(spacing, alignment, stretch, distribution), _, _| {
                        Value::Layout(
                            Style {
                                flex_direction: FlexDirection::$direction,
                                justify_content: (distribution != Distribution::Start)
                                    .then(|| distribution.taffy()),
                                align_items: Some(if stretch {
                                    AlignItems::STRETCH
                                } else {
                                    alignment.taffy()
                                }),
                                flex_shrink: 0.0,
                                gap: Size {
                                    width: LengthPercentage::length(spacing),
                                    height: LengthPercentage::length(spacing),
                                },
                                ..Default::default()
                            },
                            Algorithm::Flex,
                        )
                    },
                );
                C::bind::<P, Second<S>>(compiler);
            }
            fn resolve(
                self,
                context: &mut ViewContext<'_>,
                id: crate::runtime::ViewId,
                resolution: &mut Resolution<'_>,
            ) -> Result<Self::Values> {
                Ok((
                    (
                        self.spacing,
                        self.alignment,
                        self.stretch,
                        self.distribution,
                    ),
                    resolution.at_layout(|resolution| {
                        self.children
                            .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
                    })?,
                ))
            }
        }
    };
}
stack!(
    HStack,
    Row,
    VerticalAlignment,
    "A horizontal row. See [the HStack constructor](fn@crate::authoring::HStack) for an example."
);
stack!(
    VStack,
    Column,
    HorizontalAlignment,
    "A vertical column. See [the VStack constructor](fn@crate::authoring::VStack) for an example."
);

/// A stack of overlapping children. See [the ZStack constructor](fn@crate::authoring::ZStack) for an example.
pub struct ZStack<C> {
    alignment: crate::layout::anchors::AnchorPoint,
    children: C,
}

impl<C: Children> ZStack<C> {
    pub fn new(children: C) -> Self {
        Self {
            alignment: crate::layout::anchors::AnchorPoint::CENTER,
            children,
        }
    }
    pub fn alignment(mut self, alignment: crate::layout::anchors::AnchorPoint) -> Self {
        self.alignment = alignment;
        self
    }
}

impl<C: Children> Body for ZStack<C> {
    type Values = (crate::layout::anchors::AnchorPoint, C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: C::SCHEMAS,
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |&alignment, _, _| {
            Value::Layout(
                Style::default(),
                Algorithm::Custom(LayoutAlgorithm::new(crate::layout::layered::Layered {
                    alignment,
                })),
            )
        });
        C::bind::<P, Second<S>>(compiler);
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.alignment,
            resolution.at_layout(|resolution| {
                self.children
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
            })?,
        ))
    }
}

pub struct LayoutView<C> {
    algorithm: LayoutAlgorithm,
    children: C,
}

impl<C: Children> LayoutView<C> {
    pub fn new<L: Layout + Send + Sync>(layout: L, children: C) -> Self
    where
        L::Cache: Send,
    {
        Self {
            algorithm: LayoutAlgorithm::new(layout),
            children,
        }
    }
}

impl<C: Children> Body for LayoutView<C> {
    type Values = (LayoutAlgorithm, C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: C::SCHEMAS,
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |layout, _, _| {
            Value::Layout(Style::default(), Algorithm::Custom(layout.clone()))
        });
        C::bind::<P, Second<S>>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((
            self.algorithm,
            resolution.at_layout(|resolution| {
                self.children
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
            })?,
        ))
    }
}
