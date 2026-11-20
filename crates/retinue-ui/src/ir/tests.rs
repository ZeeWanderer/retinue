use super::*;
use crate::{Key, scene};
use std::{rc::Rc, sync::Arc};

struct Inputs {
    offset: f32,
    enabled: bool,
}

fn inputs(offset: f32) -> Inputs {
    Inputs {
        offset,
        enabled: true,
    }
}

fn program(depth: usize) -> Rc<Template<Inputs>> {
    let mut declarations = vec![Declaration {
        parent: None,
        kind: Kind::Layout,
    }];
    for index in 1..=depth {
        declarations.push(Declaration {
            parent: Some(Local(index as u32 - 1)),
            kind: if index == depth {
                Kind::Content
            } else {
                Kind::Layout
            },
        });
    }
    let sources = vec![
        Source {
            equal: |a: &Inputs, b: &Inputs| a.offset == b.offset,
            bindings: Box::new([0]),
        },
        Source {
            equal: |a: &Inputs, b: &Inputs| a.enabled == b.enabled,
            bindings: Box::new([1]),
        },
    ];
    let bindings = vec![
        Binding {
            node: Local(depth as u32),
            property: Property::Offset,
            evaluate: Rc::new(|input: &Inputs, _, _| Value::Offset((input.offset, 0.0))),
        },
        Binding {
            node: Local(0),
            property: Property::HitTesting,
            evaluate: Rc::new(|input: &Inputs, _, _| Value::HitTesting(input.enabled)),
        },
    ];
    Rc::new(
        Template::compile(
            Arc::new(Structure::compile(&declarations).unwrap()),
            (0..=depth).map(|_| Recipe::default()).collect(),
            sources,
            bindings,
        )
        .unwrap(),
    )
}

#[test]
fn revision_only_comparison_stops_after_the_first_changed_output_source() {
    let t = program(2);
    let before = inputs(1.0);
    for (offset, enabled, compared, changed) in
        [(2.0, false, 1, 1), (1.0, false, 2, 1), (1.0, true, 2, 0)]
    {
        let w = t.compare(&before, &Inputs { offset, enabled });
        assert_eq!(
            (w.sources_compared, w.instances_changed, w.bindings_emitted),
            (compared, changed, 0)
        );
    }
}

#[test]
fn updates_compare_dynamic_sources_not_static_descendants() {
    for depth in [1, 8, 64] {
        let mut bank = Bank::default();
        let handle = bank.mount(program(depth), inputs(0.0));
        bank.stage(handle, inputs(3.0)).unwrap();
        let key = bank.key(handle, Local(depth as u32)).unwrap();
        let work = bank
            .submit(|changes| {
                assert_eq!(changes.len(), 1);
                assert!(matches!(changes[0], scene::Change::Offset(k, (3.0, 0.0)) if k == key));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            work,
            Work {
                sources_compared: 2,
                bindings_emitted: 1,
                instances_changed: 1,
                ..Default::default()
            }
        );
        assert_eq!(bank.revision(handle), Ok(1));
    }
}

#[test]
fn coalesced_noop_never_reaches_backend_or_advances_revision() {
    let mut bank = Bank::default();
    let handle = bank.mount(program(8), inputs(0.0));
    bank.stage(handle, inputs(10.0)).unwrap();
    bank.stage(handle, inputs(0.0)).unwrap();
    let work = bank
        .submit(|_| panic!("no-op submitted to backend"))
        .unwrap();
    assert_eq!(work.bindings_emitted, 0);
    assert_eq!(bank.revision(handle), Ok(0));
    assert_eq!(bank.submit(|_| panic!()).unwrap(), Work::default());
}

#[test]
fn rejected_submission_keeps_pending_values_for_retry() {
    let mut bank = Bank::default();
    let handle = bank.mount(program(1), inputs(0.0));
    bank.stage(handle, inputs(10.0)).unwrap();
    assert_eq!(bank.submit(|_| Err("busy")), Err("busy"));
    assert_eq!(bank.revision(handle), Ok(0));
    let work = bank
        .submit(|changes| {
            assert_eq!(changes.len(), 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(work.instances_changed, 1);
    assert_eq!(bank.revision(handle), Ok(1));
    assert_eq!(
        bank.element(handle).unwrap().children[0].offset,
        (10.0, 0.0)
    );
}

#[test]
fn reordered_keys_keep_state_but_new_template_remounts() {
    let p = program(2);
    let mut graph = graph::InstanceGraph::default();
    let root = graph.mount(graph_tests::outlet(), ());
    graph.set_root(Some(root)).unwrap();
    let items = |keys: &[u64]| {
        keys.iter()
            .map(|&key| (Key::from(key), p.clone(), inputs(key as f32)))
            .collect()
    };
    let original: Vec<_> = graph
        .keyed(root, Local(0), items(&[1, 2, 3]))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    graph.advance((200, 200), 1.0, None).unwrap();
    *graph.state::<String>(original[1]).unwrap() = "editing".into();
    assert!(
        graph
            .keyed(root, Local(0), items(&[3, 2, 1]))
            .unwrap()
            .iter()
            .map(|child| child.handle)
            .eq([original[2], original[1], original[0]])
    );
    assert_eq!(graph.state::<String>(original[1]).unwrap(), "editing");
    assert_eq!(
        graph
            .advance((200, 200), 1.0, None)
            .unwrap()
            .work
            .bindings_emitted,
        0
    );
    assert!(matches!(
        graph.keyed(root, Local(0), items(&[1, 1])),
        Err("duplicate collection key")
    ));
    let changed = graph
        .keyed(
            root,
            Local(0),
            vec![(Key::from(2_u64), program(2), inputs(2.0))],
        )
        .unwrap()[0]
        .handle;
    assert_ne!(changed, original[1]);
    assert!(graph.contains(original[1]));
    graph.advance((200, 200), 1.0, None).unwrap();
    assert!(!graph.contains(original[1]));
}

#[test]
fn stale_and_foreign_handles_cannot_target_new_instances() {
    let mut bank = Bank::default();
    let p = program(1);
    let old = bank.mount(p.clone(), inputs(0.0));
    bank.stage(old, inputs(1.0)).unwrap();
    bank.remove(old).unwrap();
    let new = bank.mount(p, inputs(0.0));
    assert_ne!(old, new);
    assert!(bank.stage(old, inputs(2.0)).is_err());
    assert_eq!(
        bank.submit(|_| panic!("disposed input emitted")).unwrap(),
        Work::default()
    );
    let mut other = Bank::default();
    assert!(other.stage(new, inputs(2.0)).is_err());
}

#[test]
fn topology_is_shareable_without_making_owner_payloads_send() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Structure>();
    let structure = Structure::compile(&[
        Declaration {
            parent: None,
            kind: Kind::Layout,
        },
        Declaration {
            parent: Some(Local(0)),
            kind: Kind::DynamicLayout,
        },
    ])
    .unwrap();
    assert_eq!(structure.layout_children(Local(0)), [Local(1)]);
    assert_eq!(structure.nodes[1].kind, Kind::DynamicLayout);
    assert!(structure.bytes() > 0);
    assert!(Bank::<Inputs>::instance_bytes() > 0);
    assert!(
        Structure::compile(&[
            Declaration {
                parent: None,
                kind: Kind::DynamicLayout
            },
            Declaration {
                parent: Some(Local(0)),
                kind: Kind::Content
            },
        ])
        .is_err()
    );
    assert!(
        Structure::compile(&[Declaration {
            parent: Some(Local(0)),
            kind: Kind::Layout
        }])
        .is_err()
    );
}

#[test]
fn one_source_can_target_multiple_properties() {
    let topology = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Content,
        }])
        .unwrap(),
    );
    let p = Template::compile(
        topology,
        vec![Recipe::default()],
        vec![Source {
            equal: |a: &Inputs, b: &Inputs| a.offset == b.offset,
            bindings: Box::new([0, 1]),
        }],
        vec![
            Binding {
                node: Local(0),
                property: Property::Layout,
                evaluate: Rc::new(|_, _, _| {
                    Value::Layout(taffy::Style::default(), scene::Algorithm::Flex)
                }),
            },
            Binding {
                node: Local(0),
                property: Property::Paint,
                evaluate: Rc::new(|_, _, _| Value::Paint(None)),
            },
        ],
    )
    .unwrap();
    let mut changes = super::edits::Edits::default();
    let work = p
        .diff(
            &inputs(0.0),
            &inputs(1.0),
            1,
            1,
            &mut changes,
            &mut Vec::new(),
        )
        .unwrap();
    assert_eq!(work.sources_compared, 1);
    assert_eq!(changes.scene.len(), 2);
}

#[test]
fn shared_binding_runs_once_when_multiple_sources_change() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let p = Template::compile(
        Arc::new(
            Structure::compile(&[Declaration {
                parent: None,
                kind: Kind::Content,
            }])
            .unwrap(),
        ),
        vec![Recipe::default()],
        vec![
            Source {
                equal: |a: &Inputs, b| a.offset == b.offset,
                bindings: Box::new([0]),
            },
            Source {
                equal: |a: &Inputs, b| a.enabled == b.enabled,
                bindings: Box::new([0]),
            },
        ],
        vec![Binding {
            node: Local(0),
            property: Property::Offset,
            evaluate: Rc::new({
                let calls = calls.clone();
                move |p: &Inputs, _, _| {
                    calls.set(calls.get() + 1);
                    Value::Offset((p.offset, if p.enabled { 1.0 } else { 0.0 }))
                }
            }),
        }],
    )
    .unwrap();
    let mut bank = Bank::default();
    let id = bank.mount(Rc::new(p), inputs(0.0));
    bank.stage(
        id,
        Inputs {
            offset: 10.0,
            enabled: false,
        },
    )
    .unwrap();
    let work = bank
        .submit(|changes| {
            assert_eq!(changes.len(), 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(work.sources_compared, 2);
    assert_eq!(work.bindings_emitted, 1);
    assert_eq!(calls.get(), 1);
}

#[test]
fn conflicting_properties_and_invalid_dependencies_are_rejected() {
    let topology = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Content,
        }])
        .unwrap(),
    );
    let binding = |property, node| Binding::<Inputs> {
        node,
        property,
        evaluate: Rc::new(|_, _, _| Value::Content(Recipe::default())),
    };
    let source = |bindings: Vec<usize>| Source::<Inputs> {
        equal: |a, b| a.offset == b.offset,
        bindings: bindings.into_boxed_slice(),
    };
    for (bindings, sources, error) in [
        (
            vec![
                binding(Property::Content, Local(0)),
                binding(Property::Paint, Local(0)),
            ],
            vec![source(vec![0, 1])],
            "bindings write overlapping properties",
        ),
        (
            vec![binding(Property::Content, Local(1))],
            vec![source(vec![0])],
            "binding target is outside template",
        ),
        (
            vec![binding(Property::Content, Local(0))],
            vec![source(vec![1])],
            "source refers to unknown binding",
        ),
        (
            vec![binding(Property::Content, Local(0))],
            vec![],
            "dynamic binding has no source",
        ),
    ] {
        assert_eq!(
            Template::compile(topology.clone(), vec![Recipe::default()], sources, bindings).err(),
            Some(error)
        );
    }
    let wrong = Template::compile(
        topology,
        vec![Recipe::default()],
        vec![source(vec![0])],
        vec![binding(Property::Offset, Local(0))],
    )
    .unwrap();
    let mut bank = Bank::default();
    let handle = bank.mount(Rc::new(wrong), inputs(0.0));
    bank.stage(handle, inputs(1.0)).unwrap();
    assert_eq!(
        bank.submit(|_| panic!("invalid binding accepted")),
        Err("binding emitted a different property")
    );
    assert_eq!(bank.revision(handle), Ok(0));
}

#[test]
fn disposal_drops_owner_state_once_and_not_on_reorder() {
    #[derive(Default)]
    struct Owner(Option<Rc<std::cell::Cell<usize>>>);
    impl Drop for Owner {
        fn drop(&mut self) {
            if let Some(counter) = &self.0 {
                counter.set(counter.get() + 1);
            }
        }
    }
    let drops = Rc::new(std::cell::Cell::new(0));
    let mut graph = graph::InstanceGraph::default();
    let root = graph.mount(graph_tests::outlet(), ());
    graph.set_root(Some(root)).unwrap();
    let p = program(1);
    let items = |ids: &[u64]| {
        ids.iter()
            .map(|&id| (Key::from(id), p.clone(), inputs(0.0)))
            .collect()
    };
    let original: Vec<_> = graph
        .keyed(root, Local(0), items(&[1, 2]))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    graph.advance((200, 200), 1.0, None).unwrap();
    for id in original {
        graph.state::<Owner>(id).unwrap().0 = Some(drops.clone());
    }
    graph.keyed(root, Local(0), items(&[2, 1])).unwrap();
    graph.advance((200, 200), 1.0, None).unwrap();
    assert_eq!(drops.get(), 0);
    graph.keyed(root, Local(0), items(&[1])).unwrap();
    assert_eq!(drops.get(), 0);
    graph.advance((200, 200), 1.0, None).unwrap();
    assert_eq!(drops.get(), 1);
    drop(graph);
    assert_eq!(drops.get(), 2);
}

#[test]
fn removing_a_queued_instance_preserves_other_pending_edits() {
    let mut bank = Bank::default();
    let p = program(1);
    let first = bank.mount(p.clone(), inputs(0.0));
    let second = bank.mount(p, inputs(0.0));
    bank.stage(first, inputs(1.0)).unwrap();
    bank.stage(second, inputs(2.0)).unwrap();
    bank.remove(first).unwrap();
    bank.stage(second, inputs(3.0)).unwrap();
    let work = bank
        .submit(|changes| {
            assert_eq!(changes.len(), 1);
            assert!(matches!(changes[0], scene::Change::Offset(_, (3.0, 0.0))));
            Ok(())
        })
        .unwrap();
    assert_eq!(work.instances_changed, 1);
    assert_eq!(bank.revision(second), Ok(1));
}

#[test]
fn idle_instances_do_not_reserve_a_second_input_payload() {
    type Payload = [u64; 128];
    assert!(Bank::<Payload>::instance_bytes() < std::mem::size_of::<Payload>() + 64);
}
