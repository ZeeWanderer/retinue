use super::{
    Body, Compiler, NodeSchema, Select, ViewContext,
    compiler::{First, Second},
    layout_root::LayoutRoot,
};
use crate::{
    ir::{Kind, Property, Recipe, Value},
    layout::{LayoutAlgorithm, anchors::AnchorPoint},
    scene::Algorithm,
};
use taffy::{Dimension, Size, Style};

#[derive(Clone, Copy, PartialEq)]
pub struct Frame {
    pub(super) width: Option<Dimension>,
    pub(super) height: Option<Dimension>,
    pub(super) alignment: AnchorPoint,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            alignment: AnchorPoint::CENTER,
        }
    }
}
impl Frame {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dimension::length(width));
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(Dimension::length(height));
        self
    }
    pub fn alignment(mut self, alignment: AnchorPoint) -> Self {
        self.alignment = alignment;
        self
    }
    pub fn fill_width(mut self) -> Self {
        self.width = Some(Dimension::percent(1.0));
        self
    }
    pub fn fill_height(mut self) -> Self {
        self.height = Some(Dimension::percent(1.0));
        self
    }
}

pub struct Framed<C> {
    frame: Frame,
    content: C,
}
pub struct Background<C, B> {
    content: C,
    background: B,
}
pub struct Offset<C> {
    offset: (f32, f32),
    content: C,
}
pub struct HitTesting<C> {
    enabled: bool,
    content: C,
}

pub struct Padded<C> {
    insets: crate::layout::Insets,
    content: C,
}

pub trait ViewExt: Body + Sized {
    fn overlay<O: Body>(self, overlay: O) -> super::overlay::Overlay<Self, O> {
        self.overlay_aligned(AnchorPoint::CENTER, overlay)
    }

    fn overlay_aligned<O: Body>(
        self,
        alignment: AnchorPoint,
        overlay: O,
    ) -> super::overlay::Overlay<Self, O> {
        super::overlay::Overlay {
            content: self,
            overlay,
            alignment,
        }
    }

    fn role(self, role: crate::semantics::Role) -> impl Body {
        super::metadata::attach::<_, super::metadata::Role>(self, role)
    }

    fn window_action(self, action: impl Into<Option<crate::semantics::WindowAction>>) -> impl Body {
        super::metadata::attach::<_, super::metadata::WindowAction>(self, action.into())
    }

    fn layout_value<K: crate::layout::values::LayoutValueKey>(self, value: K::Value) -> impl Body {
        super::metadata::attach::<_, super::metadata::LayoutValue<K>>(self, value)
    }

    /// Place this child in the local coordinates of its positioned stack.
    fn placed(self, bounds: crate::Bounds) -> impl Body {
        super::metadata::attach::<_, super::metadata::Placed>(self, Some(bounds))
    }

    fn clipped(self) -> impl Body {
        super::metadata::attach::<_, super::metadata::Clip>(self, true)
    }

    fn interactive(self) -> impl Body {
        super::metadata::attach::<_, super::metadata::Interactive>(self, true)
    }

    fn reuse(self, hint: retinue_render::CacheHint) -> impl Body {
        super::metadata::attach::<_, super::metadata::Reuse>(self, hint)
    }

    fn named(self, name: crate::Key) -> super::Named<Self> {
        super::Named {
            content: self,
            name,
        }
    }

    fn environment<K: crate::environment::EnvironmentKey>(self, value: K::Value) -> impl Body
    where
        K::Value: PartialEq,
    {
        super::scopes::environment(self, move |environment| {
            environment.set::<K>(value);
        })
    }

    fn environment_model<T: Clone + PartialEq + 'static>(self, model: T) -> impl Body {
        super::scopes::environment(self, move |environment| {
            environment.provide(model);
        })
    }

    fn input(self, options: crate::input::InputOptions) -> super::InputScope<Self> {
        super::InputScope::new(self, options)
    }

    fn on_event(self, handler: crate::input::EventHandler) -> super::InputScope<Self> {
        self.input(crate::input::InputOptions::new(handler))
    }

    fn padding(self, insets: impl Into<crate::layout::Insets>) -> Padded<Self> {
        Padded {
            insets: insets.into(),
            content: self,
        }
    }

    fn frame(self, frame: Frame) -> Framed<Self> {
        Framed {
            frame,
            content: self,
        }
    }
    fn background<B: Body>(self, background: B) -> Background<Self, B> {
        Background {
            content: self,
            background,
        }
    }
    fn offset(self, x: f32, y: f32) -> Offset<Self> {
        Offset {
            offset: (x, y),
            content: self,
        }
    }
    fn hit_testing(self, enabled: bool) -> HitTesting<Self> {
        HitTesting {
            enabled,
            content: self,
        }
    }
}
impl<B: Body> ViewExt for B {}

impl<C: Body> Body for Padded<C> {
    type Values = (crate::layout::Insets, C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: &[C::SCHEMA],
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));

    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |insets, _, _| {
            Value::Layout(
                Style {
                    padding: taffy::Rect {
                        left: taffy::LengthPercentage::length(insets.left),
                        right: taffy::LengthPercentage::length(insets.right),
                        top: taffy::LengthPercentage::length(insets.top),
                        bottom: taffy::LengthPercentage::length(insets.bottom),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                frame_algorithm(AnchorPoint::TOP_LEADING),
            )
        });
        C::bind::<P, Second<S>>(compiler);
    }

    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut super::Resolution<'_>,
    ) -> crate::ir::Result<Self::Values> {
        Ok((
            self.insets,
            resolution.at_layout(|resolution| {
                self.content
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
            })?,
        ))
    }
}

fn frame_algorithm(alignment: AnchorPoint) -> Algorithm {
    Algorithm::Custom(LayoutAlgorithm::new(crate::layout::layered::Layered {
        alignment,
    }))
}

pub(super) fn frame_style(frame: &Frame) -> Style {
    Style {
        align_items: Some(taffy::AlignItems::CENTER),
        justify_items: Some(taffy::AlignItems::CENTER),
        flex_shrink: 0.0,
        size: Size {
            width: frame.width.unwrap_or(Dimension::auto()),
            height: frame.height.unwrap_or(Dimension::auto()),
        },
        ..Default::default()
    }
}

impl<C: Body> Body for Framed<C> {
    type Values = (Frame, C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: &[C::SCHEMA],
    };
    const NODES: usize = 1 + C::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = Some(crate::ir::Local(0));
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<First<S>>(node, Property::Layout, |frame, _, _| {
            Value::Layout(frame_style(frame), frame_algorithm(frame.alignment))
        });
        C::bind::<P, Second<S>>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut super::Resolution<'_>,
    ) -> crate::ir::Result<Self::Values> {
        Ok((
            self.frame,
            resolution.at_layout(|resolution| {
                self.content
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
            })?,
        ))
    }
}

impl<C: Body, B: Body> Body for Background<C, B> {
    type Values = (B::Values, C::Values);
    const SCHEMA: NodeSchema = if C::BACKGROUND_TARGET.is_some() && B::BOUNDS_BACKGROUND {
        NodeSchema {
            kind: Kind::Group,
            children: &[
                NodeSchema {
                    kind: Kind::Group,
                    children: &[],
                },
                C::SCHEMA,
            ],
        }
    } else {
        NodeSchema {
            kind: Kind::Layout,
            children: &[LayoutRoot::<B>::SCHEMA, LayoutRoot::<C>::SCHEMA],
        }
    };
    const NODES: usize = if C::BACKGROUND_TARGET.is_some() && B::BOUNDS_BACKGROUND {
        1 + B::NODES + C::NODES
    } else {
        1 + LayoutRoot::<B>::NODES + LayoutRoot::<C>::NODES
    };
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        if let Some(target) = C::BACKGROUND_TARGET.filter(|_| B::BOUNDS_BACKGROUND) {
            assert_eq!(B::NODES, 1, "bounds-only decoration must be one primitive");
            compiler.node();
            let primary = crate::ir::Local(compiler.current().0 + target.0);
            B::bind_background::<P, First<S>>(compiler, primary);
            C::bind::<P, Second<S>>(compiler);
            return;
        }
        compiler.recipe(
            node,
            Recipe {
                algorithm: Algorithm::Custom(LayoutAlgorithm::new(
                    crate::view::decoration::Decoration {
                        primary: 1,
                        alignment: AnchorPoint::CENTER,
                    },
                )),
                ..Default::default()
            },
        );
        LayoutRoot::<B>::bind::<P, First<S>>(compiler);
        LayoutRoot::<C>::bind::<P, Second<S>>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut super::Resolution<'_>,
    ) -> crate::ir::Result<Self::Values> {
        if C::BACKGROUND_TARGET.is_some() && B::BOUNDS_BACKGROUND {
            Ok((
                self.background
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)?,
                self.content.resolve(
                    context,
                    crate::runtime::ViewId(id.0 + 1 + B::NODES as u64),
                    resolution,
                )?,
            ))
        } else {
            resolution.at_layout(|resolution| {
                Ok((
                    LayoutRoot(self.background).resolve(
                        context,
                        crate::runtime::ViewId(id.0 + 1),
                        resolution,
                    )?,
                    LayoutRoot(self.content).resolve(
                        context,
                        crate::runtime::ViewId(id.0 + 1 + LayoutRoot::<B>::NODES as u64),
                        resolution,
                    )?,
                ))
            })
        }
    }
}

impl<C: Body> Body for Offset<C> {
    type Values = ((f32, f32), C::Values);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Layout,
        children: &[C::SCHEMA],
    };
    const NODES: usize = 1 + C::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.recipe(
            node,
            Recipe {
                algorithm: frame_algorithm(AnchorPoint::TOP_LEADING),
                ..Default::default()
            },
        );
        compiler.bind::<First<S>>(node, Property::Offset, |offset, _, _| {
            Value::Offset(*offset)
        });
        C::bind::<P, Second<S>>(compiler);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut super::Resolution<'_>,
    ) -> crate::ir::Result<Self::Values> {
        Ok((
            self.offset,
            resolution.at_layout(|resolution| {
                self.content
                    .resolve(context, crate::runtime::ViewId(id.0 + 1), resolution)
            })?,
        ))
    }
}

impl<C: Body> Body for HitTesting<C> {
    type Values = (bool, C::Values);
    const SCHEMA: NodeSchema = C::SCHEMA;
    const NODES: usize = C::NODES;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.current();
        C::bind::<P, Second<S>>(compiler);
        compiler.hit_testing::<First<S>>(node, C::SCHEMA);
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: crate::runtime::ViewId,
        resolution: &mut super::Resolution<'_>,
    ) -> crate::ir::Result<Self::Values> {
        Ok((
            self.enabled,
            resolution.with_hit_testing(self.enabled, |resolution| {
                self.content.resolve(context, id, resolution)
            })?,
        ))
    }
}
