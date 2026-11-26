use super::*;
use crate::state::State;

#[derive(Clone, Copy, PartialEq)]
struct Inputs {
    offset: f32,
    value: u32,
    enabled: bool,
}

fn initial() -> Inputs {
    Inputs {
        offset: 0.0,
        value: 1,
        enabled: true,
    }
}

fn template(seen: Rc<Cell<u32>>, calls: Rc<Cell<u32>>) -> Rc<Template<Inputs>> {
    Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[
                    Declaration {
                        parent: None,
                        kind: Kind::Group,
                    },
                    Declaration {
                        parent: Some(Local(0)),
                        kind: Kind::DynamicLayout,
                    },
                ])
                .unwrap(),
            ),
            vec![Recipe::default(), Recipe::default()],
            vec![
                Source {
                    equal: |a: &Inputs, b| a.offset == b.offset,
                    bindings: Box::new([0, 1]),
                },
                Source {
                    equal: |a: &Inputs, b| a.value == b.value && a.enabled == b.enabled,
                    bindings: Box::new([2]),
                },
            ],
            vec![
                PropertyBinding {
                    node: Local(1),
                    property: Property::Layout,
                    evaluate: Rc::new(|input: &Inputs, _, _| {
                        Value::Layout(
                            taffy::Style {
                                size: taffy::Size {
                                    width: taffy::Dimension::length(100.0 + input.offset),
                                    height: taffy::Dimension::length(50.0),
                                },
                                ..Default::default()
                            },
                            crate::scene::Algorithm::Flex,
                        )
                    }),
                },
                PropertyBinding {
                    node: Local(1),
                    property: Property::Offset,
                    evaluate: Rc::new(|input: &Inputs, _, _| Value::Offset((input.offset, 0.0))),
                },
                PropertyBinding {
                    node: Local(0),
                    property: Property::Input,
                    evaluate: Rc::new(move |input: &Inputs, _, _| {
                        calls.set(calls.get() + 1);
                        Value::Input {
                            enabled: input.enabled,
                            options: (input.value != 0).then(|| {
                                InputOptions::new(EventHandler::with(
                                    (input.value, seen.clone()),
                                    |(value, seen), event, _| {
                                        if matches!(
                                            event,
                                            Event::Pointer {
                                                phase: PointerPhase::Down(_),
                                                ..
                                            }
                                        ) {
                                            seen.set(*value);
                                        }
                                    },
                                ))
                                .focusable(FocusPolicy::Tab)
                            }),
                        }
                    }),
                },
            ],
        )
        .unwrap()
        .with_input_equality(PartialEq::eq),
    )
}

#[test]
fn host_routing_publishes_geometry_without_installing_a_second_target() {
    let mut graph = InstanceGraph::with_host_routed_input();
    let root = graph.mount(template(Default::default(), Default::default()), initial());
    graph.set_root(Some(root)).unwrap();
    for offset in [0.0, 12.0] {
        graph
            .stage(
                root,
                Inputs {
                    offset,
                    ..initial()
                },
            )
            .unwrap();
        let update = graph.advance((200, 100), 1.0, None).unwrap();
        assert!(update.completion.unwrap().is_ready());
        assert_eq!(graph.geometry(address(root, 1)).unwrap().layout.x, offset);
        assert_eq!(update.work.input_updates, 0);
        assert!(graph.input().targets().is_empty());
    }
    assert!(
        graph
            .bind_input(address(root, 0), true, Some(InputOptions::default()))
            .is_err()
    );
}

#[test]
fn observed_handler_edits_skip_layout_and_reuse_transaction_storage() {
    let mut graph = InstanceGraph::default();
    let seen = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let model = State::new(initial());
    let root = graph.mount(template(seen.clone(), calls.clone()), initial());
    graph.set_root(Some(root)).unwrap();
    graph
        .observe(root, {
            let model = model.clone();
            move || model.get()
        })
        .unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(calls.get(), 1);
    let frame = graph.scene().frame().generation();
    let storage = graph.changes.inputs.as_ptr();
    let targets = graph.input().targets().as_ptr();
    for value in 2..6 {
        model.set(Inputs { value, ..initial() });
        let update = graph.advance((200, 100), 1.0, None).unwrap();
        assert_eq!(update.work.owners_evaluated, 1);
        assert_eq!(update.work.bindings_emitted, 1);
        assert_eq!(update.work.input_updates, 1);
        assert_eq!(graph.scene().frame().generation(), frame);
        assert_eq!(graph.changes.inputs.as_ptr(), storage);
        assert!(graph.changes.inputs.is_empty());
        assert_eq!(graph.input().targets().as_ptr(), targets);
        graph.dispatch(&pointer(10.0, 10.0));
        assert_eq!(seen.get(), value);
    }
    let update = graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(update.work, Work::default());
    model.set(Inputs {
        value: 6,
        enabled: false,
        ..initial()
    });
    graph.advance((200, 100), 1.0, None).unwrap();
    assert!(!graph.input().targets()[0].enabled);
    assert!(graph.input().focused().is_none());
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    assert_eq!(graph.scene().frame().generation(), frame);
    model.set(Inputs {
        value: 7,
        ..initial()
    });
    graph.advance((200, 100), 1.0, None).unwrap();
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 7);
    model.set(Inputs {
        value: 0,
        ..initial()
    });
    graph.advance((200, 100), 1.0, None).unwrap();
    assert!(graph.input().targets().is_empty());
    assert_eq!(graph.scene().frame().generation(), frame);
    graph.set_root(None).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(graph.changes.inputs.capacity(), 0);
}

#[test]
fn rejected_input_edits_retry_atomically_and_topology_does_not_restore_old_handlers() {
    let mut graph = InstanceGraph::default();
    let seen = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let root = graph.mount(template(seen.clone(), calls.clone()), initial());
    graph.set_root(Some(root)).unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap();
    graph
        .stage(
            root,
            Inputs {
                value: 2,
                offset: f32::NAN,
                ..initial()
            },
        )
        .unwrap();
    assert!(graph.advance((200, 100), 1.0, None).is_err());
    assert!(graph.changes.inputs.is_empty());
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 1);
    assert_eq!(graph.input().targets().len(), 1);
    graph
        .stage(
            root,
            Inputs {
                value: 2,
                ..initial()
            },
        )
        .unwrap();
    let child = graph.mount(cell(), 0.0);
    graph
        .children(root, Local(1), vec![(1.into(), child)])
        .unwrap();
    let after = graph.advance((200, 100), 1.0, None).unwrap();
    assert!(after.published > before.published);
    assert_eq!(calls.get(), 3);
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 2);
    graph.children(root, Local(1), vec![]).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(calls.get(), 3);
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 2);
}

#[test]
fn typed_handlers_wait_for_queued_geometry_and_explicit_edits_override_one_admission() {
    let mut graph = InstanceGraph::default();
    let seen = Rc::new(Cell::new(0));
    let root = graph.mount(template(seen.clone(), Rc::default()), initial());
    graph.set_root(Some(root)).unwrap();
    let cell = cell();
    let children = (0..40)
        .map(|id| (id.into(), graph.mount(cell.clone(), 0.0)))
        .collect();
    graph.children(root, Local(1), children).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    graph
        .stage(
            root,
            Inputs {
                value: 2,
                offset: 20.0,
                ..initial()
            },
        )
        .unwrap();
    let (_host, execution, wake) = crate::ir::lifecycle_tests::executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(queued.completion.unwrap().is_pending());
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 1);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    graph.dispatch(&pointer(25.0, 10.0));
    assert_eq!(seen.get(), 2);
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    graph
        .stage(
            root,
            Inputs {
                value: 3,
                offset: 20.0,
                ..initial()
            },
        )
        .unwrap();
    graph
        .bind_input(address(root, 0), true, Some(handler(9, seen.clone())))
        .unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    graph.dispatch(&pointer(25.0, 10.0));
    assert_eq!(seen.get(), 9);
    graph
        .stage(
            root,
            Inputs {
                value: 4,
                offset: 20.0,
                ..initial()
            },
        )
        .unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    graph.dispatch(&pointer(25.0, 10.0));
    assert_eq!(seen.get(), 4);
}
