use super::*;
use crate::{
    environment::{Environment, EnvironmentKey},
    ir::{Binding, Declaration, Property, Recipe, Source, Structure, Value},
    state::State,
};
use std::{cell::Cell, panic::AssertUnwindSafe, sync::Arc};

mod context;

fn template() -> Rc<Template<f32>> {
    Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[Declaration {
                    parent: None,
                    kind: Kind::Content,
                }])
                .unwrap(),
            ),
            vec![Recipe::default()],
            vec![Source {
                equal: PartialEq::eq,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(0),
                property: Property::Offset,
                evaluate: Rc::new(|x: &f32, _, _| Value::Offset((*x, 0.0))),
            }],
        )
        .unwrap()
        .with_input_equality(PartialEq::eq),
    )
}

#[test]
fn only_invalidated_owners_run_and_multiple_mutations_coalesce() {
    let wakes = Rc::new(Cell::new(0));
    let wake = wakes.clone();
    let mut graph = InstanceGraph::new(move || wake.set(wake.get() + 1));
    let root = graph.mount(crate::ir::graph_tests::outlet(), ());
    let a = graph.mount(template(), 0.0);
    let b = graph.mount(template(), 0.0);
    graph
        .children(root, Local(0), vec![(0.into(), a), (1.into(), b)])
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let first = State::new(0.0_f32);
    let second = State::new(0.0_f32);
    let read = first.clone();
    graph.observe(a, move || read.get()).unwrap();
    let read = second.clone();
    graph.observe(b, move || read.get()).unwrap();
    assert_eq!(wakes.get(), 1);
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        2
    );
    assert!(!graph.has_pending_inputs());
    first.set(1.0);
    first.set(2.0);
    first.set(3.0);
    assert_eq!(wakes.get(), 2);
    let result = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(result.work.owners_evaluated, 1);
    assert_eq!(result.work.instances_changed, 1);
    assert_eq!(result.work.bindings_emitted, 1);
    assert_eq!(
        graph
            .geometry(Address {
                instance: a,
                local: Local(0)
            })
            .unwrap()
            .layout
            .x,
        3.0
    );
    assert_eq!(
        graph.advance((100, 100), 1.0, None).unwrap().work,
        Work::default()
    );
    first.set(1.0);
    first.set(3.0);
    let result = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(result.work.owners_evaluated, 1);
    assert_eq!(result.work.sources_compared, 0);
    assert_eq!(result.work.bindings_emitted, 0);
    assert_eq!(result.completion, Ok(Poll::Ready(Damage::None)));
}

struct Position;
impl EnvironmentKey for Position {
    type Value = f32;
    fn default_value() -> f32 {
        0.0
    }
}

#[test]
fn conditional_and_inherited_environment_dependencies_are_replaced() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let ancestor = Environment::default();
    let local = ancestor.child();
    let enabled = State::new(true);
    let env = local.clone();
    let use_environment = enabled.clone();
    graph
        .observe(root, move || {
            if use_environment.get() {
                env.get::<Position>()
            } else {
                20.0
            }
        })
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    ancestor.set::<Position>(2.0);
    assert!(graph.has_pending_inputs());
    graph.advance((100, 100), 1.0, None).unwrap();
    local.set::<Position>(4.0);
    graph.advance((100, 100), 1.0, None).unwrap();
    ancestor.set::<Position>(3.0);
    assert!(!graph.has_pending_inputs());
    local.remove::<Position>();
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        1
    );
    assert_eq!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(0)
            })
            .unwrap()
            .layout
            .x,
        3.0
    );
    enabled.set(false);
    graph.advance((100, 100), 1.0, None).unwrap();
    ancestor.set::<Position>(10.0);
    local.set::<Position>(12.0);
    assert!(!graph.has_pending_inputs());
}

#[test]
fn mutations_during_reads_and_panics_do_not_lose_pending_work() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let state = State::new(0.0_f32);
    let read = state.clone();
    graph
        .observe(root, move || {
            let value = read.get();
            if value == 0.0 {
                read.set(1.0);
            }
            value
        })
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert!(graph.has_pending_inputs());
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        1
    );
    assert!(!graph.has_pending_inputs());
    let fail = Rc::new(Cell::new(true));
    let attempt = fail.clone();
    let read = state.clone();
    graph
        .observe(root, move || {
            let value = read.get();
            assert!(!attempt.replace(false), "test reader panic");
            value
        })
        .unwrap();
    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(|| graph.advance((100, 100), 1.0, None)))
            .is_err()
    );
    assert!(graph.has_pending_inputs());
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        1
    );
    assert!(!graph.has_pending_inputs());
}

#[test]
fn disposed_and_replaced_readers_unsubscribe_and_stale_queued_handles_are_harmless() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let first = State::new(0.0_f32);
    let read = first.clone();
    graph.observe(root, move || read.get()).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    first.set(2.0);
    let second = State::new(0.0_f32);
    let read = second.clone();
    graph.observe(root, move || read.get()).unwrap();
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        1
    );
    first.set(3.0);
    assert!(!graph.has_pending_inputs());
    graph.set_root(None).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    second.set(2.0);
    assert!(!graph.has_pending_inputs());
    let detached = graph.mount(template(), 0.0);
    graph.observe(detached, || 1.0_f32).unwrap();
    graph.discard();
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .owners_evaluated,
        0
    );
    assert!(!graph.has_pending_inputs());
    assert!(graph.observe(detached, || 0.0_f32).is_err());
}
