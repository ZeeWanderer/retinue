use super::*;
use crate::{
    drawing::{CanvasContent, DrawingContext, Typography},
    ir::{Address, Local},
    state::State,
};
use retinue_render::Color;
use std::{cell::Cell, rc::Rc};

mod composition;
mod dynamic;
mod invalidation;
mod layout;
mod modifiers;
mod named;
mod scopes;

#[derive(PartialEq)]
struct Fill;
impl CanvasContent for Fill {
    fn draw(&self, context: &mut DrawingContext) {
        context.fill(context.bounds, 0.0, Color::rgb(0x123456));
    }
}

struct Label {
    text: State<String>,
    width: State<f32>,
    reads: Rc<Cell<usize>>,
}
#[view]
impl View for Label {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        self.reads.set(self.reads.get() + 1);
        Text::new(self.text.get())
            .frame(Frame::new().width(self.width.get()).height(40.0))
            .background(Canvas::new(Fill))
    }
}

#[test]
fn token_body_compiles_to_the_same_template_as_builders() {
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            |_| {
                ui! {
                    VStack(3.0) {
                        Text("Top")
                        HStack(4.0) { Text("Left") Text("Right") }
                    }.frame(Frame::new().width(200.0)).background { Canvas(Fill) }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.len(), 1);
    assert_eq!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(2)
            })
            .unwrap()
            .layout
            .width,
        200.0
    );
}

fn advance(graph: &mut InstanceGraph) -> crate::ir::Update {
    let update = graph.advance((400, 300), 1.0, None).unwrap();
    assert!(update.completion.as_ref().unwrap().is_ready());
    update
}

#[test]
fn typed_body_updates_only_its_changed_binding_and_reuses_its_template() {
    let mut graph = InstanceGraph::default();
    let text = State::new("Hello".to_owned());
    let width = State::new(200.0);
    let reads = Rc::new(Cell::new(0));
    let handle = graph
        .mount_view(
            Label {
                text: text.clone(),
                width: width.clone(),
                reads: reads.clone(),
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(handle)).unwrap();
    advance(&mut graph);
    assert_eq!(reads.get(), 1);
    assert_eq!(graph.templates.len(), 1);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 0);
    width.set(240.0);
    let update = advance(&mut graph);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(
        graph
            .geometry(Address {
                instance: handle,
                local: Local(2)
            })
            .unwrap()
            .layout
            .width,
        240.0
    );
    text.set("Updated".into());
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    let next = graph
        .mount_view(Label { text, width, reads }, Environment::default())
        .unwrap();
    graph.set_root(Some(next)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.templates.len(), 1);
}

#[test]
fn inherited_typography_invalidates_without_rebuilding_structure() {
    let mut graph = InstanceGraph::default();
    let environment = Environment::default();
    let handle = graph
        .mount_body(|_| Text::new("Inherited"), environment.clone())
        .unwrap();
    graph.set_root(Some(handle)).unwrap();
    advance(&mut graph);
    let address = Address {
        instance: handle,
        local: Local(0),
    };
    let before = graph.geometry(address).unwrap().layout;
    let mut typography = environment.get::<Typography>();
    typography.size *= 2.0;
    environment.set::<Typography>(typography);
    let update = advance(&mut graph);
    assert_eq!(update.work.owners_evaluated, 1);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(update.work.topology_visited, 0);
    assert!(graph.geometry(address).unwrap().layout.height > before.height);
}

#[test]
fn fixed_child_structure_has_a_compile_time_shape_without_elements() {
    type Labels = VStack<(Text, HStack<(Text, Text)>)>;
    const COUNT: usize = Labels::NODES;
    const ROOT: NodeSchema = Labels::SCHEMA;
    assert_eq!(COUNT, 5);
    assert_eq!(ROOT.children.len(), 2);
    assert_eq!(ROOT.children[1].children.len(), 2);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            |_| {
                VStack::new(
                    3.0,
                    (
                        Text::new("Top"),
                        HStack::new(4.0, (Text::new("Left"), Text::new("Right"))),
                    ),
                )
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.len(), 1);
    for local in 0..COUNT as u32 {
        assert!(
            graph
                .geometry(Address {
                    instance: root,
                    local: Local(local)
                })
                .is_some()
        );
    }
}
