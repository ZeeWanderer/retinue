use super::*;
use crate::{
    Bounds, Key,
    drawing::{CanvasContent, DrawingContext},
    environment::Environment,
    layout::{Available, Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews},
    scene::Algorithm,
    timeline::{Deadlines, FrameTime},
    view::{
        Canvas, HStack, IntoElement, LayoutView, Owned, Primitive, PrimitiveOutput, Text, VStack,
        View, ViewContext, ViewExt, primitive::paint,
    },
};
use retinue_render::{Color, PaintKey};
use std::{rc::Rc, sync::Arc, time::Duration};
use taffy::{AlignItems, Dimension, FlexDirection, LengthPercentage, Size, Style};

#[derive(Clone, PartialEq)]
pub(super) struct CycleInputs {
    pub name: Arc<str>,
    pub state: Arc<str>,
    pub timer: Arc<str>,
    pub color: Color,
    pub font_size: f32,
    pub width: f32,
    pub offset: (f32, f32),
    pub enabled: bool,
}

impl Default for CycleInputs {
    fn default() -> Self {
        Self {
            name: "CETUS".into(),
            state: "NIGHT".into(),
            timer: "00:10:00".into(),
            color: Color::rgb(0xd8dce6),
            font_size: 14.0,
            width: 300.0,
            offset: (0.0, 0.0),
            enabled: true,
        }
    }
}

#[derive(PartialEq)]
struct CycleLayout;

impl Layout for CycleLayout {
    type Cache = ();
    fn measure(
        &self,
        proposal: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> std::result::Result<LayoutSize, LayoutError> {
        Ok(LayoutSize::new(
            match proposal.width {
                Available::Limit(width) => width,
                _ => 300.0,
            },
            56.0,
        ))
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> std::result::Result<(), LayoutError> {
        children.place(0, Bounds::new(7.0, 7.0, 35.0, 35.0))?;
        children.place(
            1,
            Bounds::new(51.0, 2.0, (bounds.width - 58.0).max(0.0), 49.0),
        )
    }
}

#[derive(PartialEq)]
struct Background;

impl CanvasContent for Background {
    fn draw(&self, context: &mut DrawingContext) {
        context.fill(context.bounds, 24.5, Color::rgb(0x101623));
        context.fill(
            Bounds::new(0.0, 0.0, 49.0, 49.0),
            24.5,
            Color::rgb(0x263044),
        );
    }
}

#[derive(PartialEq)]
struct Icon;

impl CanvasContent for Icon {
    fn draw(&self, context: &mut DrawingContext) {
        context.fill(
            context.bounds,
            context.bounds.width / 2.0,
            Color::rgb(0x648bad),
        );
    }
}

#[derive(PartialEq)]
pub(super) struct CycleView {
    pub input: CycleInputs,
}

pub(super) struct CompiledCycle {
    pub model: crate::state::State<CycleInputs>,
}

#[crate::authoring::view]
impl crate::authoring::View for CompiledCycle {
    fn body(&self, _: &mut ViewContext<'_>) -> impl crate::authoring::Body {
        use crate::authoring::{self, Canvas, Frame, HStack, Text, VStack, ViewExt};
        let input = self.model.get();
        let label = |text: &Arc<str>| {
            Text(text.clone())
                .font_size(input.font_size)
                .color(input.color)
                .line_limit(1)
        };
        authoring::LayoutView::new(
            CycleLayout,
            (
                Canvas(Icon),
                authoring::ui! {
                    VStack(3.0) {
                        HStack(4.0) { label(&input.name) label(&input.state) }
                        label(&input.timer)
                    }
                },
            ),
        )
        .frame(Frame::new().width(input.width).height(56.0))
        .background(Canvas(Background))
        .offset(input.offset.0, input.offset.1)
        .hit_testing(input.enabled)
    }
}

#[derive(PartialEq)]
pub(super) struct CycleList {
    pub items: Vec<(u64, CycleInputs)>,
}

impl View for CycleList {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        VStack::new(
            4.0,
            self.items
                .iter()
                .map(|(id, input)| {
                    CycleView {
                        input: input.clone(),
                    }
                    .id(*id)
                })
                .collect::<Vec<_>>(),
        )
    }
}

pub(super) fn list() -> Rc<Template<()>> {
    Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[Declaration {
                    parent: None,
                    kind: Kind::DynamicLayout,
                }])
                .unwrap(),
            ),
            vec![Recipe {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: Some(AlignItems::CENTER),
                    flex_shrink: 0.0,
                    gap: Size {
                        width: LengthPercentage::length(4.0),
                        height: LengthPercentage::length(4.0),
                    },
                    ..Default::default()
                },
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .unwrap(),
    )
}

pub(super) fn list_items(count: u64) -> Vec<(u64, CycleInputs)> {
    (0..count)
        .map(|id| {
            (
                id,
                CycleInputs {
                    name: format!("CYCLE {id}").into(),
                    ..Default::default()
                },
            )
        })
        .collect()
}

impl View for CycleView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let input = &self.input;
        let label = |text: &Arc<str>| {
            Text::new(text.clone())
                .font_size(input.font_size)
                .color(input.color)
                .line_limit(1)
        };
        LayoutView::new(
            CycleLayout,
            (
                Canvas::new(Icon),
                VStack::new(
                    3.0,
                    (
                        HStack::new(4.0, (label(&input.name), label(&input.state))),
                        label(&input.timer),
                    ),
                ),
            ),
        )
        .frame(input.width, 56.0)
        .background(Canvas::new(Background))
        .offset(input.offset.0, input.offset.1)
        .hit_testing(input.enabled)
    }
}

fn resolve(content: impl Primitive) -> PrimitiveOutput {
    let mut owned = Owned::default();
    let mut deadlines = Deadlines::default();
    let environment = Environment::default();
    content.resolve(&mut ViewContext {
        id: crate::runtime::ViewId(1),
        environment: &environment,
        owned: &mut owned,
        mutation: None,
        time: FrameTime::now(),
        deadlines: &mut deadlines,
        frame_interval: Duration::from_millis(16),
        motion_enabled: false,
    })
}

fn text(input: &CycleInputs, text: Arc<str>, key: Key, revision: u64) -> Recipe {
    let output = resolve(
        Text::new(text)
            .font_size(input.font_size)
            .color(input.color)
            .line_limit(1),
    );
    Recipe {
        paint: Some(paint(&output, PaintKey::new(key, revision))),
        style: output.style,
        algorithm: output.algorithm,
    }
}

fn root_style(width: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(width),
            height: Dimension::length(56.0),
        },
        ..Style::default()
    }
}

pub(super) fn cycle() -> Rc<Template<CycleInputs>> {
    let structure = Arc::new(
        Structure::compile(&[
            Declaration {
                parent: None,
                kind: Kind::Layout,
            },
            Declaration {
                parent: Some(Local(0)),
                kind: Kind::Content,
            },
            Declaration {
                parent: Some(Local(0)),
                kind: Kind::Layout,
            },
            Declaration {
                parent: Some(Local(2)),
                kind: Kind::Layout,
            },
            Declaration {
                parent: Some(Local(3)),
                kind: Kind::Content,
            },
            Declaration {
                parent: Some(Local(3)),
                kind: Kind::Content,
            },
            Declaration {
                parent: Some(Local(2)),
                kind: Kind::Content,
            },
        ])
        .unwrap(),
    );
    let output = resolve(Canvas::new(Background));
    let mut recipes: Vec<_> = (0..7).map(|_| Recipe::default()).collect();
    recipes[0].paint = Some(paint(
        &output,
        PaintKey::new("ir.cycle.background".into(), 0),
    ));
    recipes[1].paint = Some(scene_icon());
    recipes[2].style.flex_direction = FlexDirection::Column;
    recipes[2].style.align_items = Some(AlignItems::CENTER);
    recipes[2].style.gap.height = LengthPercentage::length(3.0);
    recipes[3].style.align_items = Some(AlignItems::BASELINE);
    recipes[3].style.gap.width = LengthPercentage::length(4.0);
    let sources = vec![
        Source {
            equal: |a: &CycleInputs, b| a.name == b.name,
            bindings: Box::new([0]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.state == b.state,
            bindings: Box::new([1]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.timer == b.timer,
            bindings: Box::new([2]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.color == b.color,
            bindings: Box::new([0, 1, 2]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.font_size == b.font_size,
            bindings: Box::new([0, 1, 2]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.width == b.width,
            bindings: Box::new([3]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.offset == b.offset,
            bindings: Box::new([4]),
        },
        Source {
            equal: |a: &CycleInputs, b| a.enabled == b.enabled,
            bindings: Box::new([5]),
        },
    ];
    let bindings = vec![
        Binding {
            node: Local(4),
            property: Property::Content,
            evaluate: Rc::new(|p: &CycleInputs, k, r| {
                Value::Content(text(p, p.name.clone(), k, r))
            }),
        },
        Binding {
            node: Local(5),
            property: Property::Content,
            evaluate: Rc::new(|p: &CycleInputs, k, r| {
                Value::Content(text(p, p.state.clone(), k, r))
            }),
        },
        Binding {
            node: Local(6),
            property: Property::Content,
            evaluate: Rc::new(|p: &CycleInputs, k, r| {
                Value::Content(text(p, p.timer.clone(), k, r))
            }),
        },
        Binding {
            node: Local(0),
            property: Property::Layout,
            evaluate: Rc::new(|p: &CycleInputs, _, _| {
                Value::Layout(
                    root_style(p.width),
                    Algorithm::Custom(LayoutAlgorithm::new(CycleLayout)),
                )
            }),
        },
        Binding {
            node: Local(0),
            property: Property::Offset,
            evaluate: Rc::new(|p: &CycleInputs, _, _| Value::Offset(p.offset)),
        },
        Binding {
            node: Local(0),
            property: Property::HitTesting,
            evaluate: Rc::new(|p: &CycleInputs, _, _| Value::HitTesting(p.enabled)),
        },
    ];
    Rc::new(
        Template::compile(structure, recipes, sources, bindings)
            .unwrap()
            .with_input_equality(PartialEq::eq),
    )
}

fn scene_icon() -> crate::scene::Paint {
    crate::scene::Paint::new(PaintKey::new("ir.cycle.symbol".into(), 0), |size| {
        vec![retinue_render::Command::Fill {
            bounds: Bounds::new(0.0, 0.0, size.width, size.height),
            radius: size.width / 2.0,
            color: Color::rgb(0x648bad),
        }]
    })
}
