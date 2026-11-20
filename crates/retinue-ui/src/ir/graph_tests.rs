use super::graph::InstanceGraph;
use super::*;
use crate::scene::{Algorithm, RecordingMode, Scene};
use retinue_render::damage::Damage;
use std::{rc::Rc, sync::Arc, task::Poll};

pub(super) fn outlet() -> Rc<Template<()>> {
    Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[Declaration {
                    parent: None,
                    kind: Kind::DynamicLayout,
                }])
                .unwrap(),
            ),
            vec![Recipe::default()],
            vec![],
            vec![],
        )
        .unwrap(),
    )
}

#[test]
fn idle_and_equivalent_updates_report_no_backend_work() {
    let mut graph = InstanceGraph::default();
    let inputs = fixtures::CycleInputs::default();
    let root = graph.mount(fixtures::cycle(), inputs.clone());
    graph.set_root(Some(root)).unwrap();
    graph.advance((800, 200), 1.0, None).unwrap();
    assert!(graph.scene().stats.layout_computations > 0);
    assert!(graph.scene().stats.recordings > 0);
    let visible = graph.scene().stats.visible_chunks;
    for staged in [false, true] {
        if staged {
            graph.stage(root, inputs.clone()).unwrap();
        }
        let update = graph.advance((800, 200), 1.0, None).unwrap();
        assert!(matches!(update.completion, Ok(Poll::Ready(Damage::None))));
        let stats = graph.scene().stats;
        assert_eq!(stats.nodes, 0);
        assert_eq!(stats.layout_computations, 0);
        assert_eq!(stats.recordings, 0);
        assert_eq!(stats.visible_chunks, visible);
    }
}

#[test]
fn graph_mounts_heterogeneous_children_and_updates_them_atomically() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(outlet(), ());
    let child = graph.mount(fixtures::cycle(), fixtures::CycleInputs::default());
    graph
        .children(root, Local(0), vec![(1.into(), child)])
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let result = graph.advance((800, 200), 1.0, None).unwrap();
    assert!(matches!(result.completion, Ok(Poll::Ready(_))));
    assert_eq!(result.accepted, result.published);
    let key = graph.key(child, Local(0)).unwrap();
    graph
        .stage(
            child,
            fixtures::CycleInputs {
                width: 390.0,
                ..Default::default()
            },
        )
        .unwrap();
    let result = graph.advance((800, 200), 1.0, None).unwrap();
    assert_eq!(result.work.bindings_emitted, 1);
    assert_eq!(graph.scene().bounds(key).unwrap().layout.width, 390.0);
    assert!(graph.is_visible(child));
}

#[test]
fn graph_rejects_topology_and_properties_without_partial_acceptance() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(outlet(), ());
    let program = fixtures::cycle();
    let child = graph.mount(program.clone(), fixtures::CycleInputs::default());
    graph
        .children(root, Local(0), vec![(1.into(), child)])
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let original = graph.advance((800, 200), 1.0, None).unwrap().accepted;
    let replacement = graph.mount(program, fixtures::CycleInputs::default());
    graph
        .children(root, Local(0), vec![(2.into(), replacement)])
        .unwrap();
    graph
        .stage(
            replacement,
            fixtures::CycleInputs {
                offset: (f32::NAN, 0.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(graph.advance((800, 200), 1.0, None).is_err());
    assert!(graph.is_visible(child));
    assert!(graph.contains(child));
    assert!(!graph.is_visible(replacement));
    graph.discard();
    assert!(!graph.contains(replacement));
    assert_eq!(
        graph.advance((800, 200), 1.0, None).unwrap().accepted,
        original
    );
}

#[test]
fn graph_keeps_keyed_identity_and_cancels_unpublished_branches() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(outlet(), ());
    graph.set_root(Some(root)).unwrap();
    let a = fixtures::cycle();
    let b = fixtures::cycle();
    let items = |keys: &[u64], p: &Rc<Template<fixtures::CycleInputs>>| {
        keys.iter()
            .map(|&key| (key.into(), p.clone(), fixtures::CycleInputs::default()))
            .collect()
    };
    let old: Vec<_> = graph
        .keyed(root, Local(0), items(&[1, 2], &a))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    graph.advance((800, 200), 1.0, None).unwrap();
    *graph.state::<String>(old[0]).unwrap() = "focus".into();
    let temporary = graph.keyed(root, Local(0), items(&[1], &b)).unwrap()[0].handle;
    let restored: Vec<_> = graph
        .keyed(root, Local(0), items(&[2, 1], &a))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    assert_eq!(restored, vec![old[1], old[0]]);
    graph.advance((800, 200), 1.0, None).unwrap();
    assert_eq!(graph.state::<String>(old[0]).unwrap(), "focus");
    assert!(!graph.contains(temporary));
}

#[test]
fn graph_rejects_cycles_shared_instances_and_wrong_schema() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(outlet(), ());
    graph.set_root(Some(root)).unwrap();
    graph
        .children(root, Local(0), vec![(1.into(), root)])
        .unwrap();
    assert!(graph.advance((800, 200), 1.0, None).is_err());
    graph.children(root, Local(0), vec![]).unwrap();
    graph.advance((800, 200), 1.0, None).unwrap();
    assert!(graph.stage(root, 1_u32).is_err());
    assert!(graph.children(root, Local(9), vec![]).is_err());
    let child = graph.mount(outlet(), ());
    graph
        .children(root, Local(0), vec![(1.into(), child), (2.into(), child)])
        .unwrap();
    assert!(graph.advance((800, 200), 1.0, None).is_err());
}

#[test]
fn scene_transaction_rejects_changes_to_removed_nodes_before_mutating() {
    use crate::{
        Key,
        scene::{Change, Element, layout::Subtree},
    };
    let mut scene = Scene::default();
    let root = Key::from(1_u64);
    let child = Key::from(2_u64);
    let mut initial = Element::new(root, Default::default());
    initial
        .children
        .push(Element::new(child, Default::default()));
    scene
        .update(initial, (100, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(
        scene
            .transact(
                vec![Subtree::from(Element::new(root, Default::default()))],
                &mut vec![Change::Layout(child, Default::default(), Algorithm::Flex)],
                (100, 100),
                1.0,
                None
            )
            .is_err()
    );
    assert!(scene.contains(child));
}
