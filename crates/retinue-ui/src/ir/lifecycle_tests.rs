use super::{graph::InstanceGraph, *};
use crate::{
    Bounds, Key,
    layout::{Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews},
    scene::{Algorithm, LayoutExecution},
};
use retinue_render::{HostPolicy, RenderHost};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, mpsc},
    task::Poll,
    time::Duration,
};

#[derive(Default)]
struct Tracked(Option<Rc<Cell<usize>>>);
impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(count) = &self.0 {
            count.set(count.get() + 1);
        }
    }
}

pub(super) fn large() -> Rc<Template<bool>> {
    Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(
                    &(0..40)
                        .map(|index| Declaration {
                            parent: (index != 0).then_some(Local(0)),
                            kind: if index == 0 {
                                Kind::Layout
                            } else {
                                Kind::Content
                            },
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            ),
            (0..40).map(|_| Recipe::default()).collect(),
            vec![Source {
                equal: |a: &bool, b| a == b,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(0),
                property: Property::Layout,
                evaluate: Rc::new(|wide: &bool, _, _| {
                    Value::Layout(
                        taffy::Style {
                            size: taffy::Size {
                                width: taffy::Dimension::length(if *wide { 120.0 } else { 100.0 }),
                                height: taffy::Dimension::length(50.0),
                            },
                            ..Default::default()
                        },
                        Algorithm::Flex,
                    )
                }),
            }],
        )
        .unwrap(),
    )
}

pub(super) fn executor() -> (RenderHost, LayoutExecution, mpsc::Receiver<()>) {
    let host = RenderHost::new(HostPolicy {
        threads: 2,
        ..HostPolicy::desktop()
    });
    let (send, receive) = mpsc::channel();
    let execution = LayoutExecution::new(
        &host,
        Arc::new(move || {
            let _ = send.send(());
        }),
    );
    (host, execution, receive)
}

fn named_large(name: Key) -> Rc<Template<bool>> {
    Rc::new(
        Rc::try_unwrap(large())
            .ok()
            .unwrap()
            .with_attributes(vec![(
                Local(0),
                Attributes {
                    name: Some(name),
                    ..Default::default()
                },
            )])
            .unwrap(),
    )
}

#[test]
fn hidden_named_nodes_keep_their_declaration_but_do_not_make_visible_names_ambiguous() {
    let shared = Key::new("control", 1);
    let other = Key::new("control", 2);
    type Input = (bool, Option<Key>);
    let template = Template::compile(
        Arc::new(
            Structure::compile(&[
                Declaration {
                    parent: None,
                    kind: Kind::Layout,
                },
                Declaration {
                    parent: Some(Local(0)),
                    kind: Kind::Content,
                },
            ])
            .unwrap(),
        ),
        vec![Recipe::default(), Recipe::default()],
        vec![
            Source {
                equal: |a: &Input, b| a.0 == b.0,
                bindings: Box::new([0]),
            },
            Source {
                equal: |a: &Input, b| a.1 == b.1,
                bindings: Box::new([1]),
            },
        ],
        vec![
            Binding {
                node: Local(1),
                property: Property::Layout,
                evaluate: Rc::new(|v: &Input, _, _| {
                    Value::Layout(
                        taffy::Style {
                            display: if v.0 {
                                taffy::Display::Flex
                            } else {
                                taffy::Display::None
                            },
                            ..Default::default()
                        },
                        Algorithm::Flex,
                    )
                }),
            },
            Binding {
                node: Local(1),
                property: Property::Name,
                evaluate: Rc::new(|v: &Input, _, _| Value::Name(v.1)),
            },
        ],
    )
    .unwrap()
    .with_attributes(vec![(
        Local(0),
        Attributes {
            name: Some(shared),
            ..Default::default()
        },
    )])
    .unwrap();
    let mut graph = InstanceGraph::default();
    let handle = graph.mount(Rc::new(template), (false, Some(shared)));
    graph.set_root(Some(handle)).unwrap();
    let root = graph.key(handle, Local(0)).unwrap();
    let child = graph.key(handle, Local(1)).unwrap();
    for (visible, child_name) in [
        (false, Some(shared)),
        (true, Some(shared)),
        (true, Some(other)),
        (false, Some(other)),
        (false, Some(shared)),
        (true, Some(shared)),
        (true, None),
    ] {
        graph.stage(handle, (visible, child_name)).unwrap();
        assert!(
            graph
                .advance((200, 100), 1.0, None)
                .unwrap()
                .completion
                .unwrap()
                .is_ready()
        );
        assert_eq!(graph.authored_key(child), child_name.filter(|_| visible));
        assert!(graph.has_name(shared));
        assert_eq!(graph.has_name(other), visible && child_name == Some(other));
        if visible && child_name == Some(shared) {
            assert_eq!(graph.unique(shared), Err("ambiguous authored name"));
        } else {
            assert_eq!(graph.named_bounds(shared).unwrap().unwrap().key, root);
        }
    }
}

#[test]
fn host_names_stay_with_published_layout_until_async_replacement_finishes() {
    let mut graph = InstanceGraph::default();
    let old_name = Key::new("old control", 0);
    let new_name = Key::new("new control", 0);
    let old = graph.mount(named_large(old_name), false);
    graph.set_root(Some(old)).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    let old_address = graph.unique(old_name).unwrap().unwrap();
    let replacement = graph.mount(named_large(new_name), true);
    graph.set_root(Some(replacement)).unwrap();
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(queued.completion.unwrap().is_pending());
    assert_eq!(graph.unique(old_name).unwrap(), Some(old_address));
    assert_eq!(graph.unique(new_name).unwrap(), None);
    assert_eq!(graph.geometry(old_address).unwrap().layout.width, 100.0);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(ready.completion.unwrap().is_ready());
    assert_eq!(graph.unique(old_name).unwrap(), None);
    let new_address = graph.unique(new_name).unwrap().unwrap();
    assert_eq!(new_address.instance, replacement);
    assert_eq!(graph.geometry(new_address).unwrap().layout.width, 120.0);
}

#[test]
fn outgoing_state_lives_until_the_replacement_is_published() {
    let mut graph = InstanceGraph::default();
    let p = large();
    let old = graph.mount(p.clone(), false);
    let drops = Rc::new(Cell::new(0));
    graph.state::<Tracked>(old).unwrap().0 = Some(drops.clone());
    graph.set_root(Some(old)).unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap().published;
    let replacement = graph.mount(p, true);
    graph.set_root(Some(replacement)).unwrap();
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(queued.completion, Ok(Poll::Pending)));
    assert!(queued.accepted > before);
    assert_eq!(queued.published, before);
    let old_key = graph.key(old, Local(0)).unwrap();
    let old_address = graph.address(old_key).unwrap();
    let new_key = graph.key(replacement, Local(0)).unwrap();
    assert!(graph.geometry(old_address).is_some());
    assert!(graph.address(new_key).is_none());
    assert!(graph.is_visible(old));
    assert!(graph.contains(old));
    assert_eq!(drops.get(), 0);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(ready.completion, Ok(Poll::Ready(_))));
    assert_eq!(ready.published, queued.accepted);
    assert!(!graph.contains(old));
    assert!(graph.is_visible(replacement));
    assert!(graph.geometry(old_address).is_none());
    assert!(graph.address(new_key).is_some());
    assert_eq!(drops.get(), 1);
}

#[test]
fn transparent_owner_survives_until_its_layout_roots_are_replaced() {
    let mut graph = InstanceGraph::default();
    let template = Rc::new(
        Rc::try_unwrap(large())
            .ok()
            .unwrap()
            .with_input_equality(PartialEq::eq),
    );
    let old = graph.mount(super::fragment_tests::group(), ());
    let child = graph.mount(template.clone(), false);
    graph
        .children(old, Local(1), vec![(0.into(), child)])
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    graph.state::<Tracked>(old).unwrap().0 = Some(drops.clone());
    graph.set_root(Some(old)).unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap().published;
    let replacement = graph.mount(super::fragment_tests::group(), ());
    let new_child = graph.mount(template, true);
    graph
        .children(replacement, Local(3), vec![(0.into(), new_child)])
        .unwrap();
    graph.set_root(Some(replacement)).unwrap();
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(queued.completion, Ok(Poll::Pending)));
    assert_eq!(queued.published, before);
    assert!(graph.is_visible(old));
    assert!(graph.is_visible(child));
    assert_eq!(drops.get(), 0);
    graph.stage(new_child, true).unwrap();
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(ready.completion, Ok(Poll::Ready(_))));
    assert_eq!(ready.published, queued.accepted);
    assert_eq!(ready.work, Work::default());
    assert!(!graph.contains(old));
    assert!(!graph.contains(child));
    assert!(graph.is_visible(replacement));
    assert!(graph.is_visible(new_child));
    assert_eq!(drops.get(), 1);
}

#[test]
fn logical_ancestry_includes_transparent_nodes_and_publishes_with_geometry() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(super::fragment_tests::group(), ());
    let child = graph.mount(large(), false);
    let at = |instance, local| Address {
        instance,
        local: Local(local),
    };
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(1), vec![(0.into(), child)])
        .unwrap();
    assert_eq!(graph.parent(at(child, 0)), None);
    graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(graph.parent(at(root, 0)), None);
    assert_eq!(graph.parent(at(root, 3)), Some(at(root, 2)));
    assert_eq!(graph.parent(at(root, 2)), Some(at(root, 0)));
    assert_eq!(graph.parent(at(child, 1)), Some(at(child, 0)));
    assert_eq!(graph.parent(at(child, 0)), Some(at(root, 1)));
    assert_eq!(graph.parent(at(child, 40)), None);

    graph.children(root, Local(1), vec![]).unwrap();
    graph
        .children(root, Local(3), vec![(0.into(), child)])
        .unwrap();
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(queued.completion, Ok(Poll::Pending)));
    assert_eq!(graph.parent(at(child, 0)), Some(at(root, 1)));
    graph.discard();
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.published, queued.accepted);
    assert_eq!(graph.parent(at(child, 0)), Some(at(root, 3)));

    graph.set_root(None).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    assert_eq!(graph.parent(at(child, 0)), None);
    assert_eq!(graph.parent(at(child, 1)), None);
    assert!(!graph.contains(child));
}

#[test]
fn stale_reparent_cannot_replace_the_published_ancestry() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(super::fragment_tests::group(), ());
    let name = Key::new("moving child", 0);
    let child = graph.mount(named_large(name), false);
    let address = Address {
        instance: child,
        local: Local(0),
    };
    let original = Some(Address {
        instance: root,
        local: Local(1),
    });
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(1), vec![(0.into(), child)])
        .unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap().published;
    let (_host, execution, wake) = executor();
    graph.children(root, Local(1), vec![]).unwrap();
    graph
        .children(root, Local(3), vec![(0.into(), child)])
        .unwrap();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(queued.completion, Ok(Poll::Pending)));
    graph.children(root, Local(3), vec![]).unwrap();
    graph
        .children(root, Local(1), vec![(0.into(), child)])
        .unwrap();
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let superseding = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(superseding.completion, Ok(Poll::Pending)));
    assert_eq!(superseding.published, before);
    assert_eq!(graph.parent(address), original);
    assert_eq!(graph.unique(name).unwrap(), Some(address));
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.published, superseding.accepted);
    assert_eq!(graph.parent(address), original);
    assert_eq!(graph.unique(name).unwrap(), Some(address));

    graph
        .children(root, Local(3), vec![(0.into(), child)])
        .unwrap();
    assert!(graph.advance((200, 100), 1.0, None).is_err());
    assert_eq!(graph.parent(address), original);
    graph.discard();
    assert_eq!(graph.parent(address), original);
}

#[test]
fn stale_completion_cannot_publish_or_dispose_the_visible_generation() {
    let mut graph = InstanceGraph::default();
    let name = Key::new("replacement", 0);
    let p = named_large(name);
    let first = graph.mount(p.clone(), false);
    graph.set_root(Some(first)).unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap().published;
    let second = graph.mount(p.clone(), true);
    let drops = Rc::new(Cell::new(0));
    graph.state::<Tracked>(second).unwrap().0 = Some(drops.clone());
    graph.set_root(Some(second)).unwrap();
    let (_host, execution, wake) = executor();
    assert!(matches!(
        graph
            .advance((200, 100), 1.0, Some(&execution))
            .unwrap()
            .completion,
        Ok(Poll::Pending)
    ));
    let third = graph.mount(p, false);
    graph.set_root(Some(third)).unwrap();
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let next = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(next.completion, Ok(Poll::Pending)));
    assert_eq!(next.published, before);
    assert_eq!(graph.unique(name).unwrap().unwrap().instance, first);
    assert!(graph.is_visible(first));
    assert!(!graph.contains(second));
    assert_eq!(drops.get(), 1);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        graph
            .advance((200, 100), 1.0, Some(&execution))
            .unwrap()
            .completion
            .unwrap()
            .is_ready()
    );
    assert!(graph.is_visible(third));
    assert_eq!(graph.unique(name).unwrap().unwrap().instance, third);
    assert!(!graph.contains(first));
}

#[test]
fn coalesced_input_noop_keeps_a_queued_generation_publishable() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(large(), false);
    graph.set_root(Some(root)).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    graph.stage(root, true).unwrap();
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(matches!(queued.completion, Ok(Poll::Pending)));
    graph.stage(root, false).unwrap();
    graph.stage(root, true).unwrap();
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.accepted, queued.accepted);
    assert_eq!(ready.published, queued.accepted);
    assert_eq!(ready.work.bindings_emitted, 0);
    assert_eq!(
        graph
            .scene()
            .bounds(graph.key(root, Local(0)).unwrap())
            .unwrap()
            .layout
            .width,
        120.0
    );
}

#[test]
fn observed_noop_keeps_queued_layout_and_reader_lives_until_final_publication() {
    let mut graph = InstanceGraph::default();
    let template = Rc::new(
        Rc::try_unwrap(large())
            .ok()
            .unwrap()
            .with_input_equality(PartialEq::eq),
    );
    let root = graph.mount(template.clone(), false);
    graph.set_root(Some(root)).unwrap();
    let source = crate::state::State::new(false);
    let retained = Rc::new(());
    let keep = retained.clone();
    let read = source.clone();
    graph
        .observe(root, move || {
            std::hint::black_box(&keep);
            read.get()
        })
        .unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    source.set(true);
    let (_host, execution, wake) = executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(queued.completion.unwrap().is_pending());
    source.set(false);
    source.set(true);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.published, queued.accepted);
    assert_eq!(ready.work.owners_evaluated, 1);
    assert_eq!(ready.work.sources_compared, 0);
    assert!(ready.completion.unwrap().is_ready());

    let replacement = graph.mount(template, false);
    graph.set_root(Some(replacement)).unwrap();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(queued.completion.unwrap().is_pending());
    assert_eq!(Rc::strong_count(&retained), 2);
    source.set(false);
    assert!(graph.has_pending_inputs());
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.published, queued.accepted);
    assert_eq!(ready.work.owners_evaluated, 0);
    assert_eq!(Rc::strong_count(&retained), 1);
    source.set(true);
    assert!(!graph.has_pending_inputs());
}

#[derive(PartialEq)]
struct Failable(bool);
impl Layout for Failable {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> std::result::Result<LayoutSize, LayoutError> {
        if self.0 {
            Err(LayoutError::InvalidGeometry)
        } else {
            Ok(LayoutSize::new(100.0, 50.0))
        }
    }
    fn place(
        &self,
        _: Bounds,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> std::result::Result<(), LayoutError> {
        Ok(())
    }
}

#[test]
fn returning_visible_owner_keeps_state_and_refreshes_inputs_after_failed_replacement() {
    use crate::{
        environment::Environment,
        state::State,
        timeline::{FrameTime, Schedule},
    };
    let time = FrameTime::now();
    let mut graph = InstanceGraph::default();
    let original = graph.mount(large(), false);
    graph.set_root(Some(original)).unwrap();
    let source = State::new(false);
    let read = source.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let initialized = Rc::new(Cell::new(0));
    let count = initialized.clone();
    graph
        .observe_in(original, Environment::default(), move |context| {
            let keep = context.owned("lifetime", || {
                count.set(count.get() + 1);
                Rc::new(Tracked(Some(disposed.clone())))
            });
            std::hint::black_box(keep);
            context.time(Schedule::Periodic {
                start: time.instant,
                interval: Duration::from_secs(1),
            });
            read.get()
        })
        .unwrap();
    graph.advance_at((200, 100), 1.0, time, None).unwrap();
    assert_eq!(initialized.get(), 1);
    assert!(graph.next_deadline().is_some());
    let invalid = graph.mount(
        Rc::new(
            Template::compile(
                Arc::new(
                    Structure::compile(&[Declaration {
                        parent: None,
                        kind: Kind::Content,
                    }])
                    .unwrap(),
                ),
                vec![Recipe {
                    algorithm: Algorithm::Custom(LayoutAlgorithm::new(Failable(true))),
                    ..Default::default()
                }],
                vec![],
                vec![],
            )
            .unwrap(),
        ),
        (),
    );
    graph.set_root(Some(invalid)).unwrap();
    assert!(
        graph
            .advance_at((200, 100), 1.0, time, None)
            .unwrap()
            .completion
            .is_err()
    );
    assert!(graph.is_visible(original));
    assert_eq!(graph.next_deadline(), None);
    source.set(true);
    let failed = graph.advance_at((200, 100), 1.0, time, None).unwrap();
    assert!(failed.completion.is_err());
    assert_eq!(failed.work.owners_evaluated, 0);
    assert!(!graph.has_pending_inputs());
    assert_eq!(drops.get(), 0);

    graph.set_root(Some(original)).unwrap();
    let restored = graph.advance_at((200, 100), 1.0, time, None).unwrap();
    assert!(restored.completion.unwrap().is_ready());
    assert_eq!(restored.work.owners_evaluated, 1);
    assert_eq!(
        graph
            .geometry(Address {
                instance: original,
                local: Local(0)
            })
            .unwrap()
            .layout
            .width,
        120.0
    );
    assert_eq!(initialized.get(), 1);
    assert_eq!(drops.get(), 0);
    assert!(graph.next_deadline().is_some());
    graph.set_root(None).unwrap();
    graph.advance_at((200, 100), 1.0, time, None).unwrap();
    assert_eq!(drops.get(), 1);
    assert_eq!(graph.next_deadline(), None);
}

#[test]
fn layout_failure_is_accepted_and_correction_is_not_mistaken_for_a_noop() {
    let name = Key::new("failable", 0);
    let p = Rc::new(
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
                equal: |a: &bool, b| a == b,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(0),
                property: Property::Layout,
                evaluate: Rc::new(|bad: &bool, _, _| {
                    Value::Layout(
                        Default::default(),
                        Algorithm::Custom(LayoutAlgorithm::new(Failable(*bad))),
                    )
                }),
            }],
        )
        .unwrap()
        .with_attributes(vec![(
            Local(0),
            Attributes {
                name: Some(name),
                ..Default::default()
            },
        )])
        .unwrap(),
    );
    let mut graph = InstanceGraph::default();
    let first = graph.mount(p.clone(), false);
    graph.set_root(Some(first)).unwrap();
    let before = graph.advance((200, 100), 1.0, None).unwrap().published;
    let second = graph.mount(p, true);
    graph.set_root(Some(second)).unwrap();
    let failed = graph.advance((200, 100), 1.0, None).unwrap();
    assert!(failed.completion.is_err());
    assert!(failed.accepted > before);
    assert_eq!(failed.published, before);
    assert!(graph.contains(first));
    assert!(graph.is_visible(first));
    assert_eq!(graph.unique(name).unwrap().unwrap().instance, first);
    graph.stage(second, false).unwrap();
    let fixed = graph.advance((200, 100), 1.0, None).unwrap();
    assert!(matches!(fixed.completion, Ok(Poll::Ready(_))));
    assert_eq!(fixed.work.bindings_emitted, 1);
    assert_eq!(fixed.published, fixed.accepted);
    assert!(graph.is_visible(second));
    assert_eq!(graph.unique(name).unwrap().unwrap().instance, second);
    assert!(!graph.contains(first));
}
