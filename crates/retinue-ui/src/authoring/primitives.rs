use super::*;
use crate::{
    drawing::{CanvasContent, TextStyle, Typography},
    ir::{Property, Recipe, Value},
    view::{PrimitiveOutput, primitive::paint},
};
use retinue_render::PaintKey;

fn rectangle_paint(rectangle: Rectangle, key: crate::Key, revision: u64) -> crate::scene::Paint {
    crate::scene::Paint::new(PaintKey::new(key, revision), move |size| {
        vec![retinue_render::Command::Fill {
            bounds: crate::Bounds::new(0.0, 0.0, size.width, size.height),
            radius: rectangle.radius,
            color: rectangle.color,
        }]
    })
}

impl Body for Rectangle {
    type Values = Self;
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Content,
        children: &[],
    };
    const NODES: usize = 1;
    const BOUNDS_BACKGROUND: bool = true;

    fn bind<P: 'static, S: Select<P, Value = Self>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.recipe(
            node,
            Recipe {
                style: taffy::Style {
                    size: taffy::Size {
                        width: taffy::Dimension::percent(1.0),
                        height: taffy::Dimension::percent(1.0),
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        Self::bind_background::<P, S>(compiler, node);
    }

    fn bind_background<P: 'static, S: Select<P, Value = Self>>(
        compiler: &mut Compiler<P>,
        node: crate::ir::Local,
    ) {
        compiler.bind::<S>(node, Property::Paint, |rectangle, key, revision| {
            Value::Paint(Some(rectangle_paint(*rectangle, key, revision)))
        });
    }

    fn resolve(
        self,
        _: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<Self> {
        Ok(self)
    }
}

fn content(output: PrimitiveOutput, key: crate::Key, revision: u64) -> Value {
    Value::Content(Recipe {
        paint: Some(paint(&output, PaintKey::new(key, revision))),
        style: output.style,
        algorithm: output.algorithm,
    })
}

impl Body for Image {
    type Values = PrimitiveOutput;
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Content,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<S>(node, Property::Content, |output, key, revision| {
            content(output.clone(), key, revision)
        });
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok(crate::view::Primitive::resolve(&self, context))
    }
}

impl Body for Text {
    type Values = (Self, TextStyle);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Content,
        children: &[],
    };
    const NODES: usize = 1;
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<S>(node, Property::Content, |(text, style), key, revision| {
            content(text.resolve_with(style.clone()), key, revision)
        });
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((self, context.environment().get::<Typography>()))
    }
}

impl<C: CanvasContent> Body for Canvas<C> {
    type Values = (Self, TextStyle);
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Content,
        children: &[],
    };
    const NODES: usize = 1;
    const BOUNDS_BACKGROUND: bool = true;
    fn bind_background<P: 'static, S: Select<P, Value = Self::Values>>(
        compiler: &mut Compiler<P>,
        target: crate::ir::Local,
    ) {
        compiler.bind::<S>(target, Property::Paint, |(canvas, style), key, revision| {
            let output = canvas.resolve_with(style.clone());
            let mut paint = crate::scene::Paint::new(PaintKey::new(key, revision), move |size| {
                output.drawing.record(
                    crate::Bounds::new(0.0, 0.0, size.width, size.height),
                    output.text_style.clone(),
                )
            });
            paint.cache = output.cache;
            Value::Paint(Some(paint))
        });
    }
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        let node = compiler.node();
        compiler.bind::<S>(node, Property::Content, |(canvas, style), key, revision| {
            content(canvas.resolve_with(style.clone()), key, revision)
        });
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        _: crate::runtime::ViewId,
        _: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        Ok((self, context.environment().get::<Typography>()))
    }
}
