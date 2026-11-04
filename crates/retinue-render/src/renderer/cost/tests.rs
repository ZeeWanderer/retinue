use super::*;
#[cfg(feature = "benchmark")]
mod benchmark;
use crate::{
    Bounds, CacheHint, CollectionContext, CollectionMotion, CollectionRow, Color, Command,
    LayerPolicy, OptimizationContext, PaintGroup, PaintKey, ProgramStore, ResidencyPolicy,
    damage::{Damage, Region},
    painter::Painter,
    relocate::Plan,
    surface::Snapshot,
};

const SIZE: (u32, u32) = (320, 240);
const BACKGROUND: Color = Color::rgb(0x182026);

fn bounds() -> Rect {
    Rect {
        width: SIZE.0,
        height: SIZE.1,
        ..Rect::default()
    }
}

fn shadow(store: &mut ProgramStore, revision: u64, scale: f32) -> PaintProgram {
    store
        .record(
            PaintKey::new(1.into(), revision),
            (120.0, 80.0),
            scale,
            Rc::new(vec![Command::InsetShadow {
                bounds: Bounds::new(0.0, 0.0, 120.0, 80.0),
                radius: 8.0,
                blur: 4.0,
                spread: 1.0,
                color: Color::rgb(0x40aa20 + revision as u32),
            }]),
        )
        .unwrap()
}

fn item(program: PaintProgram, origin: (i32, i32), cached: bool) -> PaintInstance {
    let mut item = PaintInstance::new(1.into(), program, origin, bounds());
    item.cache = if cached {
        CachePolicy::Prefer
    } else {
        CachePolicy::Never
    };
    item
}

fn replace(frame: &mut PaintFrame, items: Vec<PaintInstance>, groups: Vec<PaintGroup>) {
    let scale = items.first().map_or(1.0, |i| i.program.scale());
    frame.replace(SIZE, scale, items, groups).unwrap();
}

fn snapshot(frame: &PaintFrame, renderer: Option<&Renderer>) -> Snapshot {
    let mut scene = Snapshot::new(SIZE, BACKGROUND);
    if let Some(renderer) = renderer {
        scene.record_rendered(frame, renderer, RasterMode::Explicit, |_, _| false);
    } else {
        scene.record(frame, |_, _| false);
    }
    scene
}

fn paint(frame: &PaintFrame, renderer: &mut Renderer) {
    let mut pixels = vec![0; SIZE.0 as usize * SIZE.1 as usize * 4];
    renderer.prepare(frame, RasterMode::Explicit);
    let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1).unwrap();
    renderer.paint(frame, &mut painter, bounds(), BACKGROUND);
    painter.finish().unwrap();
}

#[test]
fn cached_item_changes_admission_not_pixel_evidence_or_residency() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let program = shadow(&mut store, 0, 1.0);
    replace(
        &mut frame,
        vec![item(program.clone(), (20, 20), true)],
        vec![],
    );
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    let cold = snapshot(&frame, Some(&renderer));
    assert_eq!(renderer.residency_stats().entries, 0);
    paint(&frame, &mut renderer);
    let before = snapshot(&frame, Some(&renderer));
    let stats = format!("{:?}", renderer.residency_stats());
    let id = renderer.prepared[0].unwrap();
    let references = Rc::strong_count(renderer.residency.image(id));
    let raw = snapshot(&frame, None);
    let region = Region::new([bounds()]);
    assert_eq!(cold.repair_cost(&region), raw.repair_cost(&region));
    assert!(before.repair_cost(&region) < raw.repair_cost(&region));
    assert_eq!(before.damage_since(Some(&raw)), Damage::None);
    assert_eq!(raw.damage_since(Some(&before)), Damage::None);
    assert_eq!(format!("{:?}", renderer.residency_stats()), stats);
    assert_eq!(Rc::strong_count(renderer.residency.image(id)), references);

    replace(
        &mut frame,
        vec![item(program.clone(), (28, 20), true)],
        vec![],
    );
    let current = snapshot(&frame, Some(&renderer));
    let raw = snapshot(&frame, None);
    let damage = current.damage_since(Some(&before)).regions(SIZE);
    assert!(current.repair_cost(&damage) < raw.repair_cost(&damage));
    assert!(Plan::new(&raw, Some(&before), &damage).copied_pixels() > 0);
    assert_eq!(
        current.damage_since(Some(&before)),
        raw.damage_since(Some(&before))
    );

    replace(&mut frame, vec![item(program, (28, 20), false)], vec![]);
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_cost(&region),
        raw.repair_cost(&region)
    );
    for program in [shadow(&mut store, 1, 1.0), shadow(&mut store, 0, 1.25)] {
        replace(&mut frame, vec![item(program, (28, 20), true)], vec![]);
        assert_eq!(
            snapshot(&frame, Some(&renderer)).repair_cost(&region),
            snapshot(&frame, None).repair_cost(&region)
        );
    }
}

#[test]
fn fragment_cost_uses_its_own_clip_and_does_not_double_charge_parent() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let fragment = shadow(&mut store, 0, 1.0);
    let parent = store
        .record(
            2,
            (300.0, 200.0),
            1.0,
            Rc::new(vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 300.0, 200.0),
                    radius: 0.0,
                    color: BACKGROUND,
                },
                Command::Translate {
                    offset: (10.0, 20.0),
                    commands: Rc::new(vec![Command::Clip {
                        bounds: Bounds::new(0.0, 0.0, 50.0, 30.0),
                        commands: Rc::new(vec![
                            Command::Fragment {
                                program: fragment.clone(),
                                context: OptimizationContext::default().with(CacheHint::Stable),
                            },
                            Command::Fragment {
                                program: fragment,
                                context: OptimizationContext::default().with(CacheHint::Stable),
                            },
                        ]),
                    }]),
                },
            ]),
        )
        .unwrap();
    replace(&mut frame, vec![item(parent, (0, 0), false)], vec![]);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    paint(&frame, &mut renderer);
    assert_eq!(renderer.fragments.selected.len(), 1);
    let full = Region::new([bounds()]);
    let scene = snapshot(&frame, Some(&renderer));
    assert_eq!(scene.repair_work(&full).sampled, (2 * 50 * 30) as f64);
    let outside = Region::new([Rect {
        x: 200,
        y: 100,
        width: 20,
        height: 20,
    }]);
    assert_eq!(
        scene.repair_work(&outside).solid,
        (2 * outside.area()) as f64
    );
    assert_eq!(scene.repair_work(&outside).sampled, 0.0);
}

#[test]
fn indexed_cache_evidence_survives_reordering_but_not_a_different_frame() {
    let mut store = ProgramStore::default();
    let mut frame = PaintFrame::default();
    let first = item(shadow(&mut store, 0, 1.0), (10, 20), true);
    let mut second = item(shadow(&mut store, 1, 1.0), (150, 20), false);
    second.key = 2.into();
    replace(&mut frame, vec![first.clone(), second.clone()], vec![]);
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    paint(&frame, &mut renderer);
    let full = Region::new([bounds()]);
    let expected = (120 * 80) as f64;
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_work(&full).sampled,
        expected
    );
    replace(&mut frame, vec![second.clone(), first.clone()], vec![]);
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_work(&full).sampled,
        expected
    );
    let mut other = PaintFrame::default();
    replace(&mut other, vec![second, first], vec![]);
    assert_eq!(
        snapshot(&other, Some(&renderer)).repair_cost(&full),
        snapshot(&other, None).repair_cost(&full)
    );
}

fn layer_renderer() -> Renderer {
    Renderer::with_layers(
        ResidencyPolicy::desktop(0),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 2,
            min_work: 1,
            threads: 1,
        },
    )
}

#[test]
fn resident_layers_follow_translation_but_not_changed_content_or_context() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let program = shadow(&mut store, 0, 1.0);
    let group = PaintGroup {
        key: 10.into(),
        instances: 0..1,
        context: OptimizationContext::default().with(CacheHint::Stable),
    };
    replace(
        &mut frame,
        vec![item(program.clone(), (20, 20), false)],
        vec![group.clone()],
    );
    let mut renderer = layer_renderer();
    paint(&frame, &mut renderer);
    paint(&frame, &mut renderer);
    assert_eq!(renderer.layers.prepared.len(), 1);
    let full = Region::new([bounds()]);
    let expected = (120 * 80) as f64;
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_work(&full).sampled,
        expected
    );
    replace(
        &mut frame,
        vec![item(program.clone(), (28, 20), false)],
        vec![group.clone()],
    );
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_work(&full).sampled,
        expected
    );
    for groups in [
        vec![],
        vec![PaintGroup {
            context: OptimizationContext::default(),
            ..group.clone()
        }],
        vec![PaintGroup {
            context: group.context.clone().with(CollectionContext {
                motion: CollectionMotion::Reordering,
                ..Default::default()
            }),
            ..group.clone()
        }],
    ] {
        replace(
            &mut frame,
            vec![item(program.clone(), (28, 20), false)],
            groups,
        );
        assert_eq!(
            snapshot(&frame, Some(&renderer)).repair_cost(&full),
            snapshot(&frame, None).repair_cost(&full)
        );
    }
    replace(
        &mut frame,
        vec![item(shadow(&mut store, 1, 1.0), (28, 20), false)],
        vec![group],
    );
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_cost(&full),
        snapshot(&frame, None).repair_cost(&full)
    );
}

#[test]
fn cached_row_does_not_hide_uncached_rows_or_newly_exposed_pixels() {
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let program = shadow(&mut store, 0, 1.0);
    let mut second = item(shadow(&mut store, 1, 1.0), (20, 110), false);
    second.key = 2.into();
    let items = vec![item(program.clone(), (20, 20), false), second];
    let group = PaintGroup {
        key: 10.into(),
        instances: 0..2,
        context: OptimizationContext::default()
            .with(CacheHint::Stable)
            .with(CollectionContext {
                origin: (20, 20),
                motion: CollectionMotion::Translating,
                rows: vec![
                    CollectionRow {
                        key: 1.into(),
                        instances: 0..1,
                    },
                    CollectionRow {
                        key: 2.into(),
                        instances: 1..2,
                    },
                ],
            }),
    };
    replace(&mut frame, items.clone(), vec![group.clone()]);
    let mut renderer = layer_renderer();
    paint(&frame, &mut renderer);
    paint(&frame, &mut renderer);
    assert_eq!(renderer.layers.prepared.len(), 1);
    let full = Region::new([bounds()]);
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_work(&full).sampled,
        (120 * 80) as f64
    );

    let mut clipped = items.clone();
    clipped[0].set_pixels((20, 20), Rect { x: 60, ..bounds() });
    replace(&mut frame, clipped, vec![group.clone()]);
    paint(&frame, &mut renderer);
    replace(&mut frame, items, vec![group]);
    let newly_exposed = Region::new([Rect {
        x: 20,
        y: 20,
        width: 40,
        height: 80,
    }]);
    assert_eq!(
        snapshot(&frame, Some(&renderer)).repair_cost(&newly_exposed),
        snapshot(&frame, None).repair_cost(&newly_exposed)
    );
}

#[test]
fn nested_external_frames_preserve_cost_space_and_logical_damage() {
    let mut store = ProgramStore::default();
    let mut child = PaintFrame::default();
    replace(
        &mut child,
        vec![item(shadow(&mut store, 0, 1.0), (0, 0), true)],
        vec![],
    );
    let mut child_renderer = Renderer::new(ResidencyPolicy::desktop(0));
    paint(&child, &mut child_renderer);
    let mut parent = PaintFrame::default();
    let external = store
        .record(
            3,
            (300.0, 200.0),
            1.0,
            Rc::new(vec![Command::External {
                bounds: Bounds::new(30.0, 40.0, 100.0, 60.0),
                key: 9.into(),
            }]),
        )
        .unwrap();
    replace(&mut parent, vec![item(external, (10, 10), false)], vec![]);
    let renderer = Renderer::new(ResidencyPolicy::desktop(0));
    let mut scene = Snapshot::new(SIZE, BACKGROUND);
    scene.record_rendered(&parent, &renderer, RasterMode::Explicit, |_, scene| {
        scene.record_rendered(&child, &child_renderer, RasterMode::Explicit, |_, _| false);
        true
    });
    let full = Region::new([bounds()]);
    assert_eq!(scene.repair_work(&full).sampled, (100 * 60) as f64);
    let outside = Region::new([Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    }]);
    assert_eq!(scene.repair_work(&outside).solid, outside.area() as f64);
    assert_eq!(scene.repair_work(&outside).sampled, 0.0);
}
