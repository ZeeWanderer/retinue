use super::{backend_tests::render, fixtures::*, *};
use crate::{runtime::ViewTree, scene::Paint};
use retinue_render::{Color, Command, PaintKey, Renderer, ResidencyPolicy, damage::Damage};
use std::{rc::Rc, sync::Arc};

pub(super) fn group() -> Rc<Template<()>> {
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
                        kind: Kind::Slot,
                    },
                    Declaration {
                        parent: Some(Local(0)),
                        kind: Kind::Group,
                    },
                    Declaration {
                        parent: Some(Local(2)),
                        kind: Kind::Slot,
                    },
                    Declaration {
                        parent: Some(Local(2)),
                        kind: Kind::Group,
                    },
                ])
                .unwrap(),
            ),
            (0..5).map(|_| Recipe::default()).collect(),
            vec![],
            vec![],
        )
        .unwrap(),
    )
}

#[test]
fn transparent_groups_splice_keyed_roots_without_changing_layout_or_pixels() {
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (500.0 * scale) as u32);
        let mut graph = InstanceGraph::default();
        let root = graph.mount(list(), ());
        let owner = graph.mount(group(), ());
        *graph.state::<String>(owner).unwrap() = "retained editing state".into();
        let cell = cycle();
        let prefix = graph.mount(cell.clone(), CycleInputs::default());
        graph.set_root(Some(root)).unwrap();
        let mut left = list_items(2);
        let mut right = list_items(3);
        let mut view = ViewTree::new(CycleList { items: vec![] }, || {});
        for step in 0..6 {
            match step {
                1 => {
                    left.reverse();
                    right[1].1.timer = "00:09:59".into();
                }
                2 => {
                    left.clear();
                    right.clear();
                }
                3 => {
                    left = list_items(1);
                    right = list_items(2);
                }
                _ => {}
            }
            let children = if step == 5 {
                vec![(90.into(), prefix)]
            } else if step == 4 {
                vec![(91.into(), owner), (90.into(), prefix)]
            } else {
                vec![(90.into(), prefix), (91.into(), owner)]
            };
            graph.children(root, Local(0), children).unwrap();
            if step != 5 {
                for (slot, items) in [(Local(1), &left), (Local(3), &right)] {
                    graph
                        .keyed(
                            owner,
                            slot,
                            items
                                .iter()
                                .map(|(id, input)| ((*id).into(), cell.clone(), input.clone()))
                                .collect(),
                        )
                        .unwrap();
                }
            }
            let update = graph.advance(size, scale, None).unwrap();
            assert!(update.completion.unwrap().is_ready());
            if step == 4 {
                assert_eq!(update.work.nodes_materialized, 1);
                assert_eq!(update.work.bindings_emitted, 0);
            }
            let mut items = vec![(90, CycleInputs::default())];
            if step != 5 {
                let nested = left
                    .iter()
                    .map(|(id, input)| (*id, input.clone()))
                    .chain(right.iter().map(|(id, input)| (*id + 10, input.clone())));
                if step == 4 {
                    items.splice(0..0, nested);
                } else {
                    items.extend(nested);
                }
                assert!(
                    graph
                        .scene()
                        .bounds(graph.key(owner, Local(0)).unwrap())
                        .is_none()
                );
                assert!(
                    graph
                        .scene()
                        .bounds(graph.key(owner, Local(1)).unwrap())
                        .is_none()
                );
                assert_eq!(
                    graph.state::<String>(owner).unwrap(),
                    "retained editing state"
                );
            } else {
                assert!(!graph.contains(owner));
            }
            view.set_root(CycleList { items });
            view.update(size, scale).unwrap();
            let mut actual = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut expected = actual.clone();
            render(
                graph.scene(),
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut actual,
                Damage::Full,
            );
            render(
                view.scene(),
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            assert!(
                actual == expected,
                "fragment pixels differ at {scale}, step {step}"
            );
        }
    }
}

#[test]
fn static_multi_root_template_keeps_property_and_retained_node_identity() {
    let structure = Arc::new(
        Structure::compile(&[
            Declaration {
                parent: None,
                kind: Kind::Group,
            },
            Declaration {
                parent: Some(Local(0)),
                kind: Kind::Content,
            },
            Declaration {
                parent: Some(Local(0)),
                kind: Kind::Group,
            },
            Declaration {
                parent: Some(Local(2)),
                kind: Kind::Content,
            },
        ])
        .unwrap(),
    );
    assert_eq!(structure.roots(), [Local(1), Local(3)]);
    let mut recipes: Vec<_> = (0..4).map(|_| Recipe::default()).collect();
    for index in [1, 3] {
        recipes[index].style.size = taffy::Size {
            width: taffy::Dimension::length(20.0),
            height: taffy::Dimension::length(10.0),
        };
    }
    let template = Rc::new(
        Template::compile(
            structure,
            recipes,
            vec![Source {
                equal: PartialEq::eq,
                bindings: Box::new([0, 1]),
            }],
            [Local(1), Local(3)]
                .into_iter()
                .map(|node| Binding {
                    node,
                    property: Property::Paint,
                    evaluate: Rc::new(|input: &bool, key, revision| {
                        let color = Color::rgb(if *input { 0x00ff00 } else { 0xff0000 });
                        Value::Paint(Some(Paint::new(
                            PaintKey::new(key, revision),
                            move |bounds| {
                                vec![Command::Fill {
                                    bounds: crate::Bounds::new(
                                        0.0,
                                        0.0,
                                        bounds.width,
                                        bounds.height,
                                    ),
                                    radius: 0.0,
                                    color,
                                }]
                            },
                        )))
                    }),
                })
                .collect(),
        )
        .unwrap(),
    );
    let mut graph = InstanceGraph::default();
    let root = graph.mount(list(), ());
    let pair = graph.mount(template.clone(), false);
    let other = graph.mount(template, false);
    graph
        .children(root, Local(0), vec![(0.into(), pair), (1.into(), other)])
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let mounted = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(mounted.work.nodes_materialized, 5);
    #[cfg(feature = "profiling")]
    assert_eq!(mounted.work.existing_nodes_materialized, 0);
    assert_eq!(graph.scene().geometry().len(), 6);
    for local in [Local(1), Local(3)] {
        let bounds = graph
            .scene()
            .bounds(graph.key(pair, local).unwrap())
            .unwrap();
        assert_eq!(bounds.parent, Some(graph.key(root, Local(0)).unwrap()));
    }
    let programs: Vec<_> = graph
        .scene()
        .frame()
        .instances()
        .iter()
        .map(|i| i.program.id())
        .collect();
    graph
        .children(root, Local(0), vec![(1.into(), other), (0.into(), pair)])
        .unwrap();
    let reordered = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(reordered.work.nodes_materialized, 1);
    #[cfg(feature = "profiling")]
    assert_eq!(reordered.work.existing_nodes_materialized, 1);
    let after: Vec<_> = graph
        .scene()
        .frame()
        .instances()
        .iter()
        .map(|i| i.program.id())
        .collect();
    assert_eq!(after, [&programs[2..], &programs[..2]].concat());
    graph.stage(pair, true).unwrap();
    let changed = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(changed.work.bindings_emitted, 2);
    assert_eq!(changed.work.nodes_materialized, 0);
    graph.set_root(Some(pair)).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(graph.scene().geometry().len(), 3);
    assert!(!graph.contains(other));
}

#[test]
fn transparent_nodes_reject_properties_that_need_a_box() {
    let structure = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Slot,
        }])
        .unwrap(),
    );
    assert_eq!(
        Template::<()>::compile(
            structure.clone(),
            vec![Recipe {
                style: taffy::Style {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .err(),
        Some("transparent nodes cannot own layout or paint")
    );
    assert_eq!(
        Template::<bool>::compile(
            structure.clone(),
            vec![Recipe::default()],
            vec![Source {
                equal: PartialEq::eq,
                bindings: Box::new([0])
            }],
            vec![Binding {
                node: Local(0),
                property: Property::HitTesting,
                evaluate: Rc::new(|p, _, _| Value::HitTesting(*p))
            }],
        )
        .err(),
        Some("property binding targets a transparent node")
    );
    assert_eq!(
        Template::<()>::compile(structure, vec![Recipe::default()], vec![], vec![])
            .unwrap()
            .with_attributes(vec![(Local(0), Attributes::default())])
            .err(),
        Some("attributes target a transparent node")
    );
}
