use retinue_ui::{
    ir::{
        Binding, Declaration, InstanceGraph, Kind, Local, Property, Recipe, Source, Structure,
        Template, Value,
    },
    scene::Algorithm,
    taffy::{Dimension, Size, Style},
};
use std::{rc::Rc, sync::Arc, task::Poll};

#[test]
fn typed_templates_feed_published_geometry_without_an_authored_tree() {
    let structure = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Content,
        }])
        .unwrap(),
    );
    let template = Rc::new(
        Template::compile(
            structure,
            vec![Recipe::default()],
            vec![Source {
                equal: |a: &f32, b| a == b,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(0),
                property: Property::Layout,
                evaluate: Rc::new(|width, _, _| {
                    Value::Layout(
                        Style {
                            size: Size {
                                width: Dimension::length(*width),
                                height: Dimension::length(20.0),
                            },
                            ..Default::default()
                        },
                        Algorithm::Flex,
                    )
                }),
            }],
        )
        .unwrap(),
    );
    let mut graph = InstanceGraph::default();
    let item = graph.mount(template, 40.0);
    graph.set_root(Some(item)).unwrap();
    let initial = graph.advance((300, 100), 1.0, None).unwrap();
    assert!(matches!(initial.completion, Ok(Poll::Ready(_))));
    let key = graph.key(item, Local(0)).unwrap();
    let address = graph.address(key).unwrap();
    graph.stage(item, 80.0_f32).unwrap();
    let changed = graph.advance((300, 100), 1.0, None).unwrap();
    assert_eq!(changed.work.bindings_emitted, 1);
    assert_eq!(changed.work.nodes_materialized, 0);
    assert_eq!(changed.work.topology_visited, 0);
    assert_eq!(graph.geometry(address).unwrap().layout.width, 80.0);
    graph.stage(item, 80.0_f32).unwrap();
    let noop = graph.advance((300, 100), 1.0, None).unwrap();
    assert_eq!(noop.published, changed.published);
    assert_eq!(noop.work.bindings_emitted, 0);
    graph.stage(item, 400.0_f32).unwrap();
    graph.advance((300, 100), 1.0, None).unwrap();
    assert_eq!(graph.geometry(address).unwrap().layout.width, 400.0);
}
