use super::{backend_tests::render, fixtures::*, graph::InstanceGraph, *};
use crate::runtime::ViewTree;
use retinue_render::{Color, Renderer, ResidencyPolicy, damage::Damage};
use std::task::Poll;

#[test]
fn combined_collection_and_property_edits_match_authored_layout_and_paint() {
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (600.0 * scale) as u32);
        let program = cycle();
        let mut graph = InstanceGraph::default();
        let root = graph.mount(list(), ());
        graph.set_root(Some(root)).unwrap();
        let mut items = list_items(8);
        let mut views = ViewTree::new(
            CycleList {
                items: items.clone(),
            },
            || {},
        );
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        for step in 0..7 {
            match step {
                1 => {
                    items.rotate_left(3);
                    items[1].1.color = Color::rgb(0x7cb895);
                }
                2 => {
                    items.remove(2);
                    items[0].1.width = 390.0;
                }
                3 => {
                    items.insert(
                        2,
                        (
                            42,
                            CycleInputs {
                                timer: "00:09:59".into(),
                                ..Default::default()
                            },
                        ),
                    );
                }
                4 => items.reverse(),
                5 => items.clear(),
                6 => items = list_items(5),
                _ => {}
            }
            graph
                .keyed(
                    root,
                    Local(0),
                    items
                        .iter()
                        .map(|(id, input)| ((*id).into(), program.clone(), input.clone()))
                        .collect(),
                )
                .unwrap();
            let update = graph.advance(size, scale, None).unwrap();
            let Poll::Ready(damage) = update.completion.unwrap() else {
                panic!("inline layout queued")
            };
            render(graph.scene(), &mut renderer, &mut pixels, damage);
            views.set_root(CycleList {
                items: items.clone(),
            });
            views.update(size, scale).unwrap();
            let mut expected = vec![0; pixels.len()];
            render(
                views.scene(),
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            let mut actual = vec![0; pixels.len()];
            render(
                graph.scene(),
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut actual,
                Damage::Full,
            );
            assert!(
                actual == expected,
                "collection pixels differ at step {step}, scale {scale}; first byte {:?}",
                actual.iter().zip(&expected).position(|(a, b)| a != b)
            );
            assert!(
                pixels
                    .iter()
                    .zip(&actual)
                    .enumerate()
                    .all(|(i, (a, b))| a.abs_diff(*b) <= u8::from(i % 4 != 3)),
                "incremental repair differs at step {step}, scale {scale}"
            );
            for geometry in graph.scene().geometry().iter().skip(1) {
                let address = graph.address(geometry.key).unwrap();
                assert_eq!(graph.geometry(address).unwrap().layout, geometry.layout);
            }
            if step == 4 {
                assert_eq!(
                    update.work.nodes_materialized, 1,
                    "reorder rematerialized unchanged cells"
                );
                assert_eq!(update.work.bindings_emitted, 0);
            }
        }
    }
}
