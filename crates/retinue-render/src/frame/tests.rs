use super::*;
mod background;
mod coverage;
mod deferred;
mod fragments;
mod properties;
mod repair;
mod residency;
mod resolved;
mod tiles;
use crate::{
    Bounds, Color, Command, ImageFit, LayerPolicy, PaintKey, RasterMode, Renderer,
    painter::{Painter, RasterImage},
    resources::ResourceCache,
};

const SIZE: (u32, u32) = (160, 120);

fn viewport() -> Rect {
    Rect {
        width: SIZE.0,
        height: SIZE.1,
        ..Rect::default()
    }
}

fn item(id: u64, revision: u64, origin: (i32, i32), scale: f32) -> PaintInstance {
    let commands = Rc::new(vec![
        Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 40.0, 30.0),
            radius: 0.0,
            color: Color::rgb(0x263649),
        },
        Command::Fill {
            bounds: Bounds::new(5.0, 5.0, 12.0, 12.0),
            radius: 0.0,
            color: Color::rgb(if revision == 0 { 0x63c792 } else { 0xffae66 }),
        },
    ]);
    let program = PaintProgram::new(
        PaintKey::new(id.into(), revision),
        (40.0, 30.0),
        scale,
        commands,
    )
    .unwrap();
    let mut item = PaintInstance::new(id.into(), program, origin, viewport());
    item.cache = CachePolicy::Prefer;
    item
}

fn paint(frame: &PaintFrame, renderer: &mut Renderer, mode: RasterMode) -> Vec<u8> {
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    renderer.prepare(frame, mode);
    let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1).unwrap();
    renderer.paint(frame, &mut painter, viewport(), Color::rgb(0x121419));
    painter.finish().unwrap();
    pixels
}

#[test]
fn layout_free_frames_replay_after_producer_is_dropped() {
    for scale in [1.0, 1.25, 2.0] {
        let mut frame = PaintFrame::default();
        let mut trace = Vec::new();
        for step in 0..8 {
            let mut items = vec![
                item(1, u64::from(step >= 4), (10, 8 - step * 2), scale),
                item(2, 0, (45, 35 - step * 2), scale),
            ];
            if step >= 6 {
                items.reverse();
            }
            frame.replace(SIZE, scale, items, Vec::new()).unwrap();
            trace.push(frame.snapshot());
        }
        drop(frame);
        let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
        direct.set_optimizations(false);
        let mut cached = Renderer::new(crate::ResidencyPolicy::desktop(65536));
        for snapshot in trace.iter().chain(trace.iter().rev()) {
            assert_eq!(
                paint(snapshot.frame(), &mut cached, RasterMode::Explicit),
                paint(snapshot.frame(), &mut direct, RasterMode::Replay),
                "scale {scale}"
            );
        }
    }
}

#[test]
fn placement_changes_share_programs_and_preserve_rasters() {
    let mut frame = PaintFrame::default();
    let item = item(1, 0, (10, 10), 1.0);
    let commands = item.program.commands().clone();
    frame
        .replace(SIZE, 1.0, vec![item.clone()], Vec::new())
        .unwrap();
    let snapshot = frame.snapshot();
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(65536));
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    let mut moved = item;
    moved.set_pixels((25, -10), moved.clip());
    frame.replace(SIZE, 1.0, vec![moved], Vec::new()).unwrap();
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 0);
    assert!(Rc::ptr_eq(frame.chunks[0].program.commands(), &commands));
    assert!(Rc::ptr_eq(
        snapshot.frame().chunks[0].program.commands(),
        &commands
    ));
    assert_eq!(snapshot.frame().chunks[0].origin, (10, 10));
}

#[test]
fn preparation_deduplicates_consumers_and_keeps_simultaneous_revisions_live() {
    let mut frame = PaintFrame::default();
    let original = item(1, 0, (0, 0), 1.0);
    let mut second = original.clone();
    second.key = 2.into();
    second.set_pixels((50, 0), second.clip());
    frame
        .replace(SIZE, 1.0, vec![original.clone(), second], Vec::new())
        .unwrap();
    let mut cached = Renderer::new(crate::ResidencyPolicy::desktop(1));
    paint(&frame, &mut cached, RasterMode::Explicit);
    assert_eq!(cached.stats.preparation_tasks, 1);
    assert_eq!(cached.stats.prepared_rasters, 1);
    assert_eq!(
        cached.execution_plan().cloned().collect::<Vec<_>>(),
        vec![
            crate::DrawStep::Item { instance: 0 },
            crate::DrawStep::Item { instance: 1 },
        ]
    );
    let mut changed = item(1, 1, (50, 0), 1.0);
    changed.key = 2.into();
    let mut direct = Renderer::new(crate::ResidencyPolicy::desktop(0));
    for n in 0..5 {
        let mut items = vec![original.clone(), changed.clone()];
        if n % 2 == 1 {
            items.reverse();
        }
        frame.replace(SIZE, 1.0, items, Vec::new()).unwrap();
        let actual = paint(&frame, &mut cached, RasterMode::Explicit);
        assert_eq!(cached.stats.prepared_rasters, usize::from(n == 0));
        assert_eq!(actual, paint(&frame, &mut direct, RasterMode::Replay));
    }
    cached.clear_cache();
    assert_eq!(cached.residency_stats().live_bytes, 0);
}

#[test]
fn snapshot_retains_image_lease_until_its_last_consumer_drops() {
    let mut images = ResourceCache::new(0);
    let image = images.insert(
        1u64,
        RasterImage::render(12, 12, |p| {
            p.fill_rounded_rect(0, 0, 12, 12, 2, Color::rgb(0x63c792).bgra());
        })
        .unwrap(),
    );
    let program = PaintProgram::new(
        1,
        (12.0, 12.0),
        1.0,
        Rc::new(vec![Command::Image {
            bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
            image: image.into(),
            fit: ImageFit::Stretch,
        }]),
    )
    .unwrap();
    let mut frame = PaintFrame::default();
    frame
        .replace(
            SIZE,
            1.0,
            vec![PaintInstance::new(1.into(), program, (10, 10), viewport())],
            Vec::new(),
        )
        .unwrap();
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(0));
    let expected = paint(&frame, &mut renderer, RasterMode::Replay);
    let snapshot = frame.snapshot();
    drop(frame);
    assert_eq!(images.stats().cached_bytes, 0);
    assert!(images.stats().live_bytes > 0);
    assert_eq!(
        paint(snapshot.frame(), &mut renderer, RasterMode::Replay),
        expected
    );
    drop(snapshot);
    assert_eq!(images.stats().live_bytes, 0);
}

#[test]
fn identical_frames_do_not_change_revision() {
    let mut frame = PaintFrame::default();
    let items = vec![item(1, 0, (10, 10), 1.0)];
    frame.replace(SIZE, 1.0, items.clone(), Vec::new()).unwrap();
    let generation = frame.generation;
    assert_eq!(
        frame.replace(SIZE, 1.0, items, Vec::new()).unwrap(),
        Damage::None
    );
    assert_eq!(frame.generation, generation);
}

#[test]
fn invalid_replacement_is_atomic() {
    let mut frame = PaintFrame::default();
    let items = vec![
        item(1, 0, (10, 10), 1.0),
        item(2, 0, (35, 15), 1.0),
        item(3, 0, (60, 30), 1.0),
    ];
    frame.replace(SIZE, 1.0, items.clone(), Vec::new()).unwrap();
    let generation = frame.generation;
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(0));
    let before = paint(&frame, &mut renderer, RasterMode::Replay);
    assert!(
        frame
            .replace(SIZE, 1.0, vec![items[0].clone(); 2], Vec::new())
            .is_err()
    );
    assert!(
        frame
            .replace(SIZE, 1.25, items.clone(), Vec::new())
            .is_err()
    );
    assert!(
        frame
            .replace(SIZE, f32::NAN, items.clone(), Vec::new())
            .is_err()
    );
    let groups = vec![
        PaintGroup {
            key: 8.into(),
            instances: 0..2,
            context: crate::OptimizationContext::default().with(CacheHint::Stable),
        },
        PaintGroup {
            key: 9.into(),
            instances: 1..3,
            context: crate::OptimizationContext::default().with(CacheHint::Stable),
        },
    ];
    assert!(frame.replace(SIZE, 1.0, items.clone(), groups).is_err());
    let invalid = vec![PaintGroup {
        key: 8.into(),
        instances: 0..4,
        context: crate::OptimizationContext::default().with(CacheHint::Stable),
    }];
    assert!(frame.replace(SIZE, 1.0, items, invalid).is_err());
    assert_eq!(frame.generation, generation);
    assert_eq!(paint(&frame, &mut renderer, RasterMode::Replay), before);
}

#[test]
fn replacement_reports_reorder_removal_and_paint_changes() {
    let mut frame = PaintFrame::default();
    let items = vec![item(1, 0, (10, 10), 1.0), item(2, 0, (20, 20), 1.0)];
    frame.replace(SIZE, 1.0, items.clone(), Vec::new()).unwrap();
    let damage = frame
        .replace(
            SIZE,
            1.0,
            vec![items[1].clone(), items[0].clone()],
            Vec::new(),
        )
        .unwrap();
    assert_eq!(
        damage,
        Damage::region(crate::damage::Region::new([
            Rect {
                x: 10,
                y: 10,
                width: 40,
                height: 30
            },
            Rect {
                x: 20,
                y: 20,
                width: 40,
                height: 30
            }
        ]))
    );
    let damage = frame
        .replace(SIZE, 1.0, vec![items[0].clone()], Vec::new())
        .unwrap();
    assert_eq!(
        damage,
        Damage::rect(Rect {
            x: 20,
            y: 20,
            width: 40,
            height: 30
        })
    );
    let damage = frame
        .replace(SIZE, 1.0, vec![item(1, 1, (10, 10), 1.0)], Vec::new())
        .unwrap();
    assert_eq!(
        damage,
        Damage::rect(Rect {
            x: 10,
            y: 10,
            width: 40,
            height: 30
        })
    );
}

#[test]
fn frame_groups_drive_layers_without_a_layout_tree() {
    let mut frame = PaintFrame::default();
    let items = vec![item(1, 0, (10, 10), 1.0), item(2, 0, (60, 10), 1.0)];
    let group = PaintGroup {
        key: 3.into(),
        instances: 0..2,
        context: crate::OptimizationContext::default().with(CacheHint::Stable),
    };
    frame
        .replace(SIZE, 1.0, items.clone(), vec![group])
        .unwrap();
    let mut renderer = Renderer::with_layers(
        crate::ResidencyPolicy::desktop(65536),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 2,
            min_work: 1,
            threads: 1,
        },
    );
    paint(&frame, &mut renderer, RasterMode::Explicit);
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.promoted_layers, 1);
    let generation = frame.generation;
    assert_eq!(
        frame.replace(SIZE, 1.0, items, Vec::new()).unwrap(),
        Damage::None
    );
    assert!(frame.generation > generation);
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.layer_bytes, 0);
}

#[test]
fn snapshots_report_live_callback_dependencies() {
    let program = PaintProgram::new(
        1,
        (10.0, 10.0),
        1.0,
        Rc::new(vec![Command::External {
            bounds: Bounds::new(0.0, 0.0, 10.0, 10.0),
            key: 1.into(),
        }]),
    )
    .unwrap();
    let mut frame = PaintFrame::default();
    frame
        .replace(
            SIZE,
            1.0,
            vec![PaintInstance::new(1.into(), program, (0, 0), viewport())],
            Vec::new(),
        )
        .unwrap();
    assert!(frame.snapshot().frame().has_external_paint());
}
