use super::*;
use crate::{
    input::{
        Activation, Button, EventHandler, FocusPolicy, Interaction, Phase, PointerId, PointerPhase,
    },
    ir::{Binding as PropertyBinding, Declaration, Property, Recipe, Source, Structure, Value},
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
    time::Duration,
};

mod bindings;

fn address(instance: Handle, local: u32) -> Address {
    Address {
        instance,
        local: Local(local),
    }
}

fn pointer(x: f32, y: f32) -> Event {
    Event::Pointer {
        id: PointerId::MOUSE,
        phase: PointerPhase::Down(Button::Primary),
        position: (x, y),
        modifiers: Default::default(),
    }
}

fn cell() -> Rc<Template<f32>> {
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
                style: taffy::Style {
                    size: taffy::Size {
                        width: taffy::Dimension::length(40.0),
                        height: taffy::Dimension::length(30.0),
                    },
                    ..Default::default()
                },
                ..Default::default()
            }],
            vec![Source {
                equal: PartialEq::eq,
                bindings: Box::new([0]),
            }],
            vec![PropertyBinding {
                node: Local(0),
                property: Property::Offset,
                evaluate: Rc::new(|x: &f32, _, _| Value::Offset((*x, 0.0))),
            }],
        )
        .unwrap()
        .with_input_equality(PartialEq::eq),
    )
}

fn handler(value: u32, seen: Rc<Cell<u32>>) -> InputOptions {
    InputOptions::new(EventHandler::new(move |event, _| {
        if matches!(
            event,
            Event::Pointer {
                phase: PointerPhase::Down(_),
                ..
            }
        ) {
            seen.set(value);
        }
    }))
}

#[test]
fn shared_routes_follow_reparenting_through_transparent_owners() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::fixtures::list(), ());
    let group = graph.mount(crate::ir::fragment_tests::group(), ());
    let a = graph.mount(cell(), 0.0);
    let b = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(0), vec![(0.into(), group)])
        .unwrap();
    graph
        .children(group, Local(1), vec![(0.into(), a), (1.into(), b)])
        .unwrap();
    let options = InputOptions::default().focusable(FocusPolicy::Tab);
    for at in [
        address(root, 0),
        address(group, 0),
        address(group, 2),
        address(a, 0),
        address(b, 0),
    ] {
        graph.bind_input(at, true, Some(options.clone())).unwrap();
    }
    let check = |g: &InstanceGraph| {
        assert_eq!(g.inputs.routes.len(), g.input().targets().len());
        for route in &g.inputs.routes {
            let ancestors = std::iter::successors(g.parent(route.address), |at| g.parent(*at));
            let parent = ancestors
                .clone()
                .find(|at| g.inputs.bindings.contains_key(at))
                .map(|at| ViewId(g.key(at.instance, at.local).unwrap().value));
            let id = ViewId(
                g.key(route.address.instance, route.address.local)
                    .unwrap()
                    .value,
            );
            assert_eq!((route.parent, route.depth), (parent, ancestors.count()));
            assert_eq!(g.input().target(id).unwrap().parent, parent);
        }
    };
    graph.advance((160, 150), 1.0, None).unwrap();
    check(&graph);
    let id = |g: &InstanceGraph, h, local| ViewId(g.key(h, Local(local)).unwrap().value);
    let a_id = id(&graph, a, 0);
    assert!(graph.focus(Some(address(a, 0))).unwrap());
    graph
        .children(group, Local(1), vec![(1.into(), b)])
        .unwrap();
    graph
        .children(group, Local(3), vec![(0.into(), a)])
        .unwrap();
    graph.advance((160, 150), 1.0, None).unwrap();
    check(&graph);
    assert_eq!(graph.input().focused(), Some(a_id));
    assert_eq!(
        graph.input().target(a_id).unwrap().parent,
        Some(id(&graph, group, 2))
    );
    assert_eq!(graph.input().hit_test((5.0, 40.0)), Some(a_id));
    graph.bind_input(address(group, 2), true, None).unwrap();
    graph.advance((160, 150), 1.0, None).unwrap();
    check(&graph);
    assert_eq!(
        graph.input().target(a_id).unwrap().parent,
        Some(id(&graph, group, 0))
    );
}

#[test]
fn geometry_updates_reuse_routes_but_ancestor_binding_changes_rebuild_them() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::fixtures::list(), ());
    let child = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(0), vec![(0.into(), child)])
        .unwrap();
    let options = InputOptions::default().focusable(FocusPolicy::Tab);
    graph
        .bind_input(address(root, 0), true, Some(options.clone()))
        .unwrap();
    graph
        .bind_input(address(child, 0), true, Some(options.clone()))
        .unwrap();
    graph.advance((160, 80), 1.0, None).unwrap();
    let child_id = ViewId(graph.key(child, Local(0)).unwrap().value);
    let root_id = ViewId(graph.key(root, Local(0)).unwrap().value);
    let builds = graph.inputs.route_builds;
    assert_eq!(
        graph.input().target(child_id).unwrap().parent,
        Some(root_id)
    );
    assert!(graph.focus(Some(address(child, 0))).unwrap());
    for x in [5.0, 12.0, 3.0] {
        let patches = graph.inputs.motion_patches;
        graph.stage(child, x).unwrap();
        graph.advance((160, 80), 1.0, None).unwrap();
        assert_eq!(graph.inputs.motion_patches, patches + 1);
        assert_eq!(graph.inputs.route_builds, builds);
        assert_eq!(graph.input().target(child_id).unwrap().bounds.x, x);
        assert_eq!(graph.input().hit_test((x + 1.0, 1.0)), Some(child_id));
        assert_eq!(graph.input().focused(), Some(child_id));
    }
    graph.bind_input(address(root, 0), true, None).unwrap();
    graph.advance((160, 80), 1.0, None).unwrap();
    assert_eq!(graph.inputs.route_builds, builds + 1);
    assert_eq!(graph.input().target(child_id).unwrap().parent, None);
    graph
        .bind_input(address(root, 0), true, Some(options))
        .unwrap();
    graph.advance((160, 80), 1.0, None).unwrap();
    assert_eq!(graph.inputs.route_builds, builds + 2);
    assert_eq!(
        graph.input().target(child_id).unwrap().parent,
        Some(root_id)
    );
}

#[test]
fn motion_outside_input_targets_skips_publication_and_old_deltas_expire() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::fixtures::list(), ());
    let a = graph.mount(cell(), 0.0);
    let b = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(0), vec![(0.into(), a), (1.into(), b)])
        .unwrap();
    graph
        .bind_input(address(a, 0), true, Some(InputOptions::default()))
        .unwrap();
    graph.advance((160, 80), 1.0, None).unwrap();
    let first = graph.scene().input_revision();
    graph.stage(b, 15.0_f32).unwrap();
    assert_eq!(
        graph
            .advance((160, 80), 1.0, None)
            .unwrap()
            .work
            .input_updates,
        0
    );
    assert!(graph.scene().input_motion_since(first).is_some());
    graph.stage(a, 10.0_f32).unwrap();
    assert_eq!(
        graph
            .advance((160, 80), 1.0, None)
            .unwrap()
            .work
            .input_updates,
        1
    );
    assert!(graph.scene().input_motion_since(first).is_none());
    assert_eq!(graph.input().hit_test((2.0, 5.0)), None);
}

#[test]
fn transparent_input_keeps_one_identity_and_routes_through_logical_ancestors() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::fixtures::list(), ());
    let group = graph.mount(crate::ir::fragment_tests::group(), ());
    let a = graph.mount(cell(), 0.0);
    let b = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(0), vec![(0.into(), group)])
        .unwrap();
    graph
        .children(group, Local(1), vec![(0.into(), a), (1.into(), b)])
        .unwrap();
    let status = Interaction::default();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    graph
        .bind_input(
            address(group, 0),
            true,
            Some(
                InputOptions::new(EventHandler::new(move |event, context| {
                    if matches!(
                        event,
                        Event::Pointer {
                            phase: PointerPhase::Down(_),
                            ..
                        }
                    ) {
                        record.borrow_mut().push((0, context.phase));
                    }
                }))
                .interaction(status.clone())
                .focusable(FocusPolicy::Tab),
            ),
        )
        .unwrap();
    let update = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(update.work.input_updates, 1);
    assert_eq!(graph.input().targets().len(), 1);
    let id = graph.input().targets()[0].id;
    assert_eq!(graph.input_address(id), Some(address(group, 0)));
    assert_eq!(graph.input().hit_test((10.0, 10.0)), Some(id));
    assert_eq!(graph.input().hit_test((10.0, 32.0)), None);
    assert_eq!(graph.input().hit_test((10.0, 40.0)), Some(id));
    assert!(graph.focus(Some(address(group, 0))).unwrap());
    assert_eq!(graph.input().focused(), Some(id));
    assert_eq!(
        graph
            .advance((100, 100), 1.0, None)
            .unwrap()
            .work
            .input_updates,
        0
    );

    let record = seen.clone();
    graph
        .bind_input(
            address(a, 0),
            true,
            Some(InputOptions::new(EventHandler::new(
                move |event, context| {
                    if matches!(
                        event,
                        Event::Pointer {
                            phase: PointerPhase::Down(_),
                            ..
                        }
                    ) {
                        record.borrow_mut().push((1, context.phase));
                    }
                },
            ))),
        )
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(
        *seen.borrow(),
        [(0, Phase::Capture), (1, Phase::Target), (0, Phase::Bubble)]
    );
    let patches = graph.inputs.motion_patches;
    graph.stage(a, 20.0_f32).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(graph.inputs.motion_patches, patches + 1);
    let published = |g: &InstanceGraph| {
        g.input()
            .targets()
            .iter()
            .map(|t| {
                (
                    t.id,
                    t.parent,
                    t.bounds,
                    t.clip,
                    t.regions.clone(),
                    t.enabled,
                )
            })
            .collect::<Vec<_>>()
    };
    let incremental = published(&graph);
    assert_eq!(graph.input().hit_test((10.0, 10.0)), None);
    assert_eq!(graph.input().hit_test((10.0, 40.0)), Some(id));
    assert_eq!(graph.input().hit_test((25.0, 32.0)), None);
    graph.inputs.topology_changed();
    assert!(graph.sync_input());
    assert_eq!(published(&graph), incremental);
    let old_revision = graph.scene().input_revision();
    for (size, scale) in [((38, 25), 1.25), ((100, 100), 1.0)] {
        let patches = graph.inputs.motion_patches;
        graph.advance(size, scale, None).unwrap();
        assert_eq!(graph.inputs.motion_patches, patches + 1);
        let placements = published(&graph);
        let hits = [(10.0, 10.0), (10.0, 32.0), (10.0, 40.0), (25.0, 5.0)];
        let before = hits.map(|p| graph.input().hit_test(p));
        assert_eq!(before[2], (scale == 1.0).then_some(id));
        graph.inputs.topology_changed();
        graph.sync_input();
        assert_eq!(published(&graph), placements);
        assert_eq!(hits.map(|p| graph.input().hit_test(p)), before);
    }
    assert!(graph.scene().input_motion_since(old_revision).is_none());
    let placements = published(&graph);
    let patches = graph.inputs.motion_patches;
    graph.inputs.geometry_revision = old_revision;
    assert!(graph.sync_input());
    assert_eq!(graph.inputs.motion_patches, patches);
    assert_eq!(published(&graph), placements);
    graph.stage(a, 0.0_f32).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    graph
        .children(group, Local(1), vec![(1.into(), b), (0.into(), a)])
        .unwrap();
    let patches = graph.inputs.motion_patches;
    graph.advance((110, 110), 1.0, None).unwrap();
    assert_eq!(graph.inputs.motion_patches, patches);
    assert_eq!(graph.input().focused(), Some(id));
    assert_eq!(
        graph.input_address(graph.input().hit_test((10.0, 40.0)).unwrap()),
        Some(address(a, 0))
    );
}

#[test]
fn input_only_commits_do_not_relayout_and_rejected_layout_does_not_publish_handlers() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let seen = Rc::new(Cell::new(0));
    graph
        .bind_input(address(root, 0), true, Some(handler(1, seen.clone())))
        .unwrap();
    let initial = graph.advance((100, 100), 1.0, None).unwrap();
    let paint = graph.scene().frame().generation();
    graph
        .bind_input(address(root, 0), true, Some(handler(2, seen.clone())))
        .unwrap();
    let changed = graph.advance((100, 100), 1.0, None).unwrap();
    assert!(changed.published > initial.published);
    assert_eq!(graph.scene().frame().generation(), paint);
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 2);
    graph.stage(root, f32::NAN).unwrap();
    graph
        .bind_input(address(root, 0), true, Some(handler(3, seen.clone())))
        .unwrap();
    assert!(graph.advance((100, 100), 1.0, None).is_err());
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 2);
    graph.discard();
    assert_eq!(
        graph.advance((100, 100), 1.0, None).unwrap().published,
        changed.published
    );
    assert!(graph.bind_input(address(root, 1), true, None).is_err());
}

#[test]
fn queued_layout_retains_published_handler_and_bounds_until_completion() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::lifecycle_tests::large(), false);
    graph.set_root(Some(root)).unwrap();
    let seen = Rc::new(Cell::new(0));
    graph
        .bind_input(address(root, 0), true, Some(handler(1, seen.clone())))
        .unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    graph.stage(root, true).unwrap();
    graph
        .bind_input(address(root, 0), true, Some(handler(2, seen.clone())))
        .unwrap();
    let (_host, execution, wake) = crate::ir::lifecycle_tests::executor();
    let queued = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert!(queued.completion.unwrap().is_pending());
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 1);
    assert_eq!(graph.input().hit_test((110.0, 10.0)), None);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = graph.advance((200, 100), 1.0, Some(&execution)).unwrap();
    assert_eq!(ready.published, queued.accepted);
    assert!(ready.completion.unwrap().is_ready());
    graph.dispatch(&pointer(110.0, 10.0));
    assert_eq!(seen.get(), 2);
}

#[test]
fn input_cleanup_precedes_owner_disposal_and_disabled_targets_can_block() {
    #[derive(Default)]
    struct Owned(Option<Rc<Cell<u32>>>);
    impl Drop for Owned {
        fn drop(&mut self) {
            if let Some(drops) = &self.0 {
                drops.set(drops.get() + 1);
            }
        }
    }
    let mut graph = InstanceGraph::default();
    let root = graph.mount(cell(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let dropped = Rc::new(Cell::new(0));
    graph.state::<Owned>(root).unwrap().0 = Some(dropped.clone());
    let lost = Rc::new(Cell::new(0));
    let record = lost.clone();
    let alive = dropped.clone();
    let options = InputOptions::new(EventHandler::new(move |event, context| match event {
        Event::Pointer {
            phase: PointerPhase::Down(_),
            ..
        } => context.capture_pointer(),
        Event::CaptureLost(_) => {
            assert_eq!(alive.get(), 0);
            record.set(record.get() + 1);
        }
        _ => {}
    }))
    .activation(Activation::Release);
    graph
        .bind_input(address(root, 0), true, Some(options.clone()))
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    graph.dispatch(&pointer(10.0, 10.0));
    assert!(graph.input().captured(PointerId::MOUSE).is_some());
    graph
        .bind_input(
            address(root, 0),
            false,
            Some(InputOptions {
                blocks_when_disabled: true,
                ..options
            }),
        )
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(lost.get(), 1);
    assert!(graph.input().hit_test((10.0, 10.0)).is_some());
    graph.set_root(None).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(dropped.get(), 1);
    assert!(graph.input().targets().is_empty());
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
}

#[test]
fn inherited_hit_eligibility_releases_focus_and_capture_of_a_transparent_owner() {
    let template = Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[
                    Declaration {
                        parent: None,
                        kind: Kind::Layout,
                    },
                    Declaration {
                        parent: Some(Local(0)),
                        kind: Kind::Group,
                    },
                    Declaration {
                        parent: Some(Local(1)),
                        kind: Kind::Content,
                    },
                ])
                .unwrap(),
            ),
            (0..3)
                .map(|index| Recipe {
                    style: if index == 1 {
                        Default::default()
                    } else {
                        taffy::Style {
                            size: taffy::Size {
                                width: taffy::Dimension::length(40.0),
                                height: taffy::Dimension::length(30.0),
                            },
                            ..Default::default()
                        }
                    },
                    ..Default::default()
                })
                .collect(),
            vec![Source {
                equal: PartialEq::eq,
                bindings: Box::new([0]),
            }],
            vec![PropertyBinding {
                node: Local(0),
                property: Property::HitTesting,
                evaluate: Rc::new(|enabled: &bool, _, _| Value::HitTesting(*enabled)),
            }],
        )
        .unwrap(),
    );
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template, true);
    graph.set_root(Some(root)).unwrap();
    graph
        .bind_input(
            address(root, 1),
            true,
            Some(
                InputOptions::default()
                    .focusable(FocusPolicy::Tab)
                    .activation(Activation::Release),
            ),
        )
        .unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    let target = graph.input().targets()[0].id;
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(graph.input().focused(), Some(target));
    assert_eq!(graph.input().captured(PointerId::MOUSE), Some(target));
    let patches = graph.inputs.motion_patches;
    graph.advance((8, 8), 1.25, None).unwrap();
    assert_eq!(graph.inputs.motion_patches, patches + 1);
    assert_eq!(graph.input().focused(), Some(target));
    assert_eq!(graph.input().captured(PointerId::MOUSE), Some(target));
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    graph.stage(root, false).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(graph.inputs.motion_patches, patches + 1);
    assert!(graph.input().focused().is_none());
    assert!(graph.input().captured(PointerId::MOUSE).is_none());
    assert!(!graph.input().targets()[0].enabled);
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    graph.stage(root, true).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(graph.input().hit_test((10.0, 10.0)), Some(target));
}
