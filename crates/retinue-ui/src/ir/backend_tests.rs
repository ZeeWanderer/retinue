use super::*;
use crate::scene::{RecordingMode, Scene};
use retinue_render::{
    Color, Command, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use std::{rc::Rc, sync::Arc};

pub(super) fn render(scene: &Scene, renderer: &mut Renderer, pixels: &mut [u8], damage: Damage) {
    let size = scene.frame().size();
    renderer.prepare(scene.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(pixels, size.0, size.1, 4.try_into().unwrap()).unwrap();
    painter.repair(&damage.regions(size));
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(scene.frame(), &mut painter, bounds, Color::rgb(0x202020));
    }
    painter.finish().unwrap();
}

#[test]
fn direct_fixture_matches_retained_authored_views() {
    use crate::runtime::ViewTree;
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (100.0 * scale) as u32);
        let mut input = fixtures::CycleInputs::default();
        let mut views = ViewTree::new(
            fixtures::CycleView {
                input: input.clone(),
            },
            || {},
        );
        let mut direct = Scene::default();
        let mut bank = Bank::default();
        let handle = bank.mount(fixtures::cycle(), input.clone());
        direct
            .update(
                bank.element(handle).unwrap(),
                size,
                scale,
                RecordingMode::Retained,
            )
            .unwrap();
        for step in 0..8 {
            match step {
                1 => input.timer = "00:09:59".into(),
                2 => input.color = Color::rgb(0x7cb895),
                3 => input.width = 390.0,
                4 => input.offset = (7.25, 2.5),
                5 => input.enabled = false,
                6 => {
                    input.name = "CAMBION DRIFT".into();
                    input.font_size = 13.0;
                }
                _ => {}
            }
            views.set_root(fixtures::CycleView {
                input: input.clone(),
            });
            views.update(size, scale).unwrap();
            bank.stage(handle, input.clone()).unwrap();
            bank.submit(|changes| direct.patch(changes).map(|_| ()))
                .unwrap();
            let mut expected = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut actual = expected.clone();
            render(
                views.scene(),
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            render(
                &direct,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut actual,
                Damage::Full,
            );
            assert!(
                actual == expected,
                "authored fixture differs at step {step}, scale {scale}; first byte {:?}",
                actual.iter().zip(&expected).position(|(a, b)| a != b)
            );
        }
    }
}

#[test]
fn native_text_and_custom_canvas_updates_match_fresh_layout_and_pixels() {
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (100.0 * scale) as u32);
        let p = fixtures::cycle();
        let mut bank = Bank::default();
        let mut input = fixtures::CycleInputs::default();
        let id = bank.mount(p.clone(), input.clone());
        let mut scene = Scene::default();
        scene
            .update(
                bank.element(id).unwrap(),
                size,
                scale,
                RecordingMode::Retained,
            )
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        render(&scene, &mut renderer, &mut pixels, Damage::Full);
        let initial_programs: Vec<_> = scene
            .frame()
            .instances()
            .iter()
            .take(2)
            .map(|i| i.program.id())
            .collect();
        assert_eq!(
            scene
                .frame()
                .instances()
                .iter()
                .flat_map(|i| i.program.commands().iter())
                .filter(|c| matches!(c, Command::GlyphRun { .. }))
                .count(),
            3
        );
        for step in 0..6 {
            match step {
                0 => input.timer = "00:09:59".into(),
                1 => {
                    input.name = "CAMBION DRIFT".into();
                    input.font_size = 13.0;
                }
                2 => input.color = Color::rgb(0x7cb895),
                3 => input.width = 390.0,
                4 => input.offset = (7.25, 2.5),
                _ => input.enabled = false,
            }
            bank.stage(id, input.clone()).unwrap();
            let mut damage = Damage::None;
            let work = bank
                .submit(|changes| {
                    damage = scene.patch(changes)?;
                    Ok(())
                })
                .unwrap();
            assert_eq!(work.sources_compared, 8);
            assert_eq!(
                work.bindings_emitted,
                if step == 1 || step == 2 { 3 } else { 1 }
            );
            if step == 2 {
                assert_eq!(scene.stats.layout_computations, 0);
            }
            if step == 5 {
                assert!(matches!(damage, Damage::None));
            }
            let repair = damage.regions(size);
            let before = pixels.clone();
            render(&scene, &mut renderer, &mut pixels, damage);
            let mut fresh = Scene::default();
            fresh
                .update(
                    p.mount(&input, 1_000_000, 0).unwrap(),
                    size,
                    scale,
                    RecordingMode::Retained,
                )
                .unwrap();
            let geometry = |scene: &Scene| {
                scene
                    .geometry()
                    .iter()
                    .map(|g| (g.layout, g.clip, g.interactive))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                geometry(&scene),
                geometry(&fresh),
                "step {step} scale {scale}"
            );
            let mut expected = vec![0; pixels.len()];
            render(
                &fresh,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            let mut full = vec![0; pixels.len()];
            render(
                &scene,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut full,
                Damage::Full,
            );
            assert!(
                full == expected,
                "retained full frame differs at step {step} scale {scale}"
            );
            for (index, (actual, expected)) in pixels
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                let (x, y) = (
                    (index % size.0 as usize) as f64,
                    (index / size.0 as usize) as f64,
                );
                if !repair.rects().iter().any(|rect| rect.contains(x, y)) {
                    assert_eq!(
                        actual,
                        &before[index * 4..index * 4 + 4],
                        "wrote outside repair"
                    );
                }
                // Matches the renderer's native curve-clipping oracle; full frames stay exact.
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .enumerate()
                        .all(|(channel, (a, b))| a.abs_diff(*b) <= u8::from(channel != 3)),
                    "incremental pixels differ at step {step} scale {scale}, ({x},{y}): {actual:?} != {expected:?}"
                );
            }
            if step < 3 {
                assert_eq!(
                    scene
                        .frame()
                        .instances()
                        .iter()
                        .take(2)
                        .map(|i| i.program.id())
                        .collect::<Vec<_>>(),
                    initial_programs
                );
            }
        }
    }
}

#[test]
fn rejected_backend_values_do_not_replace_accepted_input() {
    let p = fixtures::cycle();
    let mut bank = Bank::default();
    let input = fixtures::CycleInputs::default();
    let id = bank.mount(p, input.clone());
    let mut scene = Scene::default();
    scene
        .update(
            bank.element(id).unwrap(),
            (430, 100),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    let generation = scene.frame().generation();
    bank.stage(
        id,
        fixtures::CycleInputs {
            offset: (f32::NAN, 0.0),
            ..input.clone()
        },
    )
    .unwrap();
    assert_eq!(
        bank.submit(|changes| scene.patch(changes).map(|_| ())),
        Err("composition offset must be finite")
    );
    assert_eq!(bank.revision(id), Ok(0));
    assert_eq!(scene.frame().generation(), generation);
    bank.stage(
        id,
        fixtures::CycleInputs {
            offset: (3.0, 0.0),
            ..input
        },
    )
    .unwrap();
    bank.submit(|changes| scene.patch(changes).map(|_| ()))
        .unwrap();
    assert_eq!(bank.revision(id), Ok(1));
    assert_eq!(
        scene
            .bounds(bank.key(id, Local(0)).unwrap())
            .unwrap()
            .layout
            .x,
        3.0
    );
}

#[test]
fn no_op_does_not_invalidate_queued_layout_or_old_geometry() {
    use crate::scene::LayoutExecution;
    use retinue_render::{HostPolicy, RenderHost};
    use std::{sync::mpsc, time::Duration};
    use taffy::{Dimension, Size, Style};

    let declarations: Vec<_> = (0..40)
        .map(|index| Declaration {
            parent: (index != 0).then_some(Local(0)),
            kind: if index == 0 {
                Kind::Layout
            } else {
                Kind::Content
            },
        })
        .collect();
    let p = Rc::new(
        Template::compile(
            Arc::new(Structure::compile(&declarations).unwrap()),
            (0..40).map(|_| Recipe::default()).collect(),
            vec![Source {
                equal: |a: &f32, b| a == b,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(1),
                property: Property::Layout,
                evaluate: Rc::new(|width: &f32, _, _| {
                    Value::Layout(
                        Style {
                            size: Size {
                                width: Dimension::length(*width),
                                height: Dimension::length(10.0),
                            },
                            ..Style::default()
                        },
                        crate::scene::Algorithm::Flex,
                    )
                }),
            }],
        )
        .unwrap(),
    );
    let mut bank = Bank::default();
    let id = bank.mount(p, 40.0);
    let key = bank.key(id, Local(1)).unwrap();
    let mut scene = Scene::default();
    scene
        .update(
            bank.element(id).unwrap(),
            (200, 200),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    let generation = scene.frame().generation();
    let old_geometry = scene.bounds(key).unwrap().layout;
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
    assert!(
        scene
            .resize_with((300, 200), 1.0, Some(&execution))
            .unwrap()
            .is_pending()
    );
    bank.stage(id, 40.0).unwrap();
    assert_eq!(
        bank.submit(|_| panic!("no-op reached busy backend"))
            .unwrap()
            .bindings_emitted,
        0
    );
    assert_eq!(bank.revision(id), Ok(0));
    bank.stage(id, 50.0).unwrap();
    assert_eq!(
        bank.submit(|changes| scene.patch(changes).map(|_| ())),
        Err("layout update pending")
    );
    assert_eq!(scene.frame().generation(), generation);
    assert_eq!(scene.bounds(key).unwrap().layout, old_geometry);
    receive.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(scene.poll_layout(true).unwrap().is_ready());
    assert_eq!(scene.frame().size(), (300, 200));
    bank.submit(|changes| scene.patch(changes).map(|_| ()))
        .unwrap();
    assert_eq!(bank.revision(id), Ok(1));
    assert_eq!(scene.bounds(key).unwrap().layout.width, 50.0);
}

#[test]
fn dynamic_outlet_composes_typed_banks_and_retains_reordered_children() {
    use crate::Key;
    let p = Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[
                    Declaration {
                        parent: None,
                        kind: Kind::Layout,
                    },
                    Declaration {
                        parent: Some(Local(0)),
                        kind: Kind::DynamicLayout,
                    },
                ])
                .unwrap(),
            ),
            vec![Recipe::default(), Recipe::default()],
            Vec::<Source<()>>::new(),
            vec![],
        )
        .unwrap(),
    );
    let mut graph = graph::InstanceGraph::default();
    let parent = graph.mount(p, ());
    graph.set_root(Some(parent)).unwrap();
    let p = fixtures::cycle();
    let items = |keys: &[u64]| {
        keys.iter()
            .map(|&key| {
                (
                    Key::from(key),
                    p.clone(),
                    fixtures::CycleInputs {
                        width: 200.0,
                        ..Default::default()
                    },
                )
            })
            .collect()
    };
    let original: Vec<_> = graph
        .keyed(parent, Local(1), items(&[1, 2]))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    *graph.state::<String>(original[0]).unwrap() = "selected".into();
    graph.advance((800, 200), 1.0, None).unwrap();
    let mut initial_program = None;
    for ids in [&[1_u64, 2][..], &[2, 1][..], &[3, 1, 2][..], &[1][..]] {
        let order: Vec<_> = graph
            .keyed(parent, Local(1), items(ids))
            .unwrap()
            .iter()
            .map(|child| child.handle)
            .collect();
        assert_eq!(
            graph
                .advance((800, 200), 1.0, None)
                .unwrap()
                .work
                .bindings_emitted,
            0
        );
        let identity = graph.key(original[0], Local(4)).unwrap();
        let program = graph
            .scene()
            .frame()
            .instances()
            .iter()
            .find(|i| i.key == identity)
            .unwrap()
            .program
            .id();
        assert_eq!(*initial_program.get_or_insert(program), program);
        assert_eq!(graph.state::<String>(original[0]).unwrap(), "selected");
        let actual = graph
            .scene()
            .geometry()
            .iter()
            .filter(|g| g.parent == Some(graph.key(parent, Local(1)).unwrap()))
            .map(|g| g.key)
            .collect::<Vec<_>>();
        let expected = order
            .iter()
            .map(|&h| graph.key(h, Local(0)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
    assert!(!graph.contains(original[1]));
    assert!(graph.children(parent, Local(0), vec![]).is_err());
}
