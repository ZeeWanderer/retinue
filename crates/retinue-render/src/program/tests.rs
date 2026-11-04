use super::*;
use crate::{Bounds, Color, ImageFit, ResourceRead, painter::RasterImage};

fn fill() -> Rc<Vec<Command>> {
    Rc::new(vec![Command::Fill {
        bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
        radius: 2.0,
        color: Color::rgb(0x338855),
    }])
}

#[test]
fn identical_programs_share_analysis_without_sharing_instance_identity() {
    let mut store = ProgramStore::default();
    let a = store.record(1, (12.0, 12.0), 1.25, fill()).unwrap();
    let b = store.record(2, (12.0, 12.0), 1.25, fill()).unwrap();
    assert_eq!(a.id(), b.id());
    assert_ne!(a.key(), b.key());
    assert_eq!((store.compiled, store.reused), (1, 1));
    let scaled = store.record(1, (12.0, 12.0), 2.0, fill()).unwrap();
    assert_ne!(a.id(), scaled.id());
}

#[test]
fn content_hash_collision_checks_the_actual_commands() {
    let mut store = ProgramStore::default();
    let a = store.record(1, (12.0, 12.0), 1.0, fill()).unwrap();
    let commands = Rc::new(vec![Command::Fill {
        bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
        radius: 0.0,
        color: Color::rgb(0x118822),
    }]);
    store.entries.insert(
        (
            hash::fingerprint(&commands, (12.0, 12.0), 1.0),
            Compilation::Source,
        ),
        vec![Rc::downgrade(&a.data)],
    );
    let b = store.record(1, (12.0, 12.0), 1.0, commands).unwrap();
    assert_ne!(a.id(), b.id());
}

#[test]
fn new_image_versions_cannot_hit_an_old_program_even_with_the_same_paint_key() {
    let mut store = ProgramStore::default();
    let mut programs = Vec::new();
    for _ in 0..2 {
        let image = Rc::new(RasterImage::render(12, 12, |_| {}).unwrap());
        let id = image.id();
        let commands = Rc::new(vec![Command::Image {
            bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
            image: image.into(),
            fit: ImageFit::Stretch,
        }]);
        let p = store.record(1, (12.0, 12.0), 1.0, commands).unwrap();
        assert_eq!(p.analysis().resources, [ResourceRead::Image(id)]);
        programs.push(p);
    }
    assert_ne!(programs[0].id(), programs[1].id());
    assert_ne!(programs[0].signature(), programs[1].signature());
    assert_eq!(programs[0].signature().slot, programs[1].signature().slot);
}

#[test]
fn weak_interning_does_not_keep_program_resources_alive() {
    let mut store = ProgramStore::default();
    let program = store.record(1, (12.0, 12.0), 1.0, fill()).unwrap();
    let weak = Rc::downgrade(&program.data);
    drop(program);
    assert!(weak.upgrade().is_none());
}

#[test]
fn cheap_and_sparse_drawing_does_not_become_an_automatic_candidate() {
    let mut store = ProgramStore::default();
    for commands in [
        fill(),
        Rc::new(
            (0..20)
                .map(|i| Command::Fill {
                    bounds: Bounds::new((i * 100) as f32, 0.0, 12.0, 12.0),
                    radius: 2.0,
                    color: Color::rgb(0x338855),
                })
                .collect(),
        ),
    ] {
        let program = store.record(1, (2000.0, 20.0), 1.0, commands).unwrap();
        assert!(program.calls().is_empty());
    }
}

#[test]
fn text_keeps_the_glyph_mask_path_instead_of_another_colored_raster() {
    let font =
        Rc::new(crate::painter::load_overlay_font(&crate::painter::GlyphCache::new(1024)).unwrap());
    let program = PaintProgram::new(
        1,
        (200.0, 30.0),
        1.0,
        Rc::new(vec![Command::Text {
            bounds: Bounds::new(0.0, 0.0, 200.0, 30.0),
            size: 14.0,
            text: "A sufficiently long candidate".into(),
            font,
            color: Color::rgba(200, 220, 240, 200),
            align: crate::TextAlignment::Top,
        }]),
    )
    .unwrap();
    assert!(program.calls().is_empty());
}

#[test]
fn automatic_programs_share_across_view_sizes_and_do_not_keep_resources_alive() {
    let mut store = ProgramStore::default();
    let commands = Rc::new(vec![Command::InsetShadow {
        bounds: Bounds::new(3.0, 4.0, 12.0, 12.0),
        radius: 3.0,
        blur: 4.0,
        spread: 1.0,
        color: Color::rgb(0x338855),
    }]);
    let a = store
        .record(1, (20.0, 20.0), 1.0, commands.clone())
        .unwrap();
    let b = store.record(2, (40.0, 40.0), 1.0, commands).unwrap();
    assert_ne!(a.id(), b.id());
    assert_eq!(a.calls()[0].program.id(), b.calls()[0].program.id());
    let weak = Rc::downgrade(&a.calls()[0].program.data);
    drop((a, b));
    assert!(weak.upgrade().is_none());
}

#[test]
fn authored_dynamic_fragment_does_not_silently_discover_children() {
    let child = PaintProgram::new(
        1,
        (20.0, 20.0),
        1.0,
        Rc::new(vec![Command::InsetShadow {
            bounds: Bounds::new(0.0, 0.0, 20.0, 20.0),
            radius: 3.0,
            blur: 4.0,
            spread: 1.0,
            color: Color::rgb(0x338855),
        }]),
    )
    .unwrap();
    let parent = PaintProgram::new(
        2,
        (20.0, 20.0),
        1.0,
        Rc::new(vec![Command::Fragment {
            program: child,
            context: OptimizationContext::default().with(crate::CacheHint::Dynamic),
        }]),
    )
    .unwrap();
    assert!(parent.calls()[0].program.calls().is_empty());
}

#[test]
fn discovered_compound_records_resource_versions() {
    let mut store = ProgramStore::default();
    let mut programs = Vec::new();
    for _ in 0..2 {
        let image = Rc::new(RasterImage::render(12, 12, |_| {}).unwrap());
        let id = image.id();
        let mut commands: Vec<_> = (0..8)
            .map(|_| Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
                radius: 2.0,
                color: Color::rgba(40, 90, 180, 20),
            })
            .collect();
        commands.push(Command::Image {
            bounds: Bounds::new(0.0, 0.0, 12.0, 12.0),
            image: image.into(),
            fit: ImageFit::Stretch,
        });
        let p = store
            .record(1, (12.0, 12.0), 1.0, Rc::new(commands))
            .unwrap();
        assert_eq!(p.calls().len(), 1);
        assert_eq!(
            p.calls()[0].program.analysis().resources,
            [ResourceRead::Image(id)]
        );
        programs.push(p);
    }
    assert_ne!(
        programs[0].calls()[0].program.id(),
        programs[1].calls()[0].program.id()
    );
}

#[test]
fn bounds_follow_level_by_level_pixel_snapping_and_effect_barriers() {
    let command = Command::Translate {
        offset: (0.4, 0.4),
        commands: Rc::new(vec![Command::Translate {
            offset: (0.4, 0.4),
            commands: fill(),
        }]),
    };
    let p = PaintProgram::new(1, (12.0, 12.0), 1.0, Rc::new(vec![command])).unwrap();
    assert_eq!(
        p.bounds(),
        Pixels {
            x: 0,
            y: 0,
            width: 12,
            height: 12
        }
    );
    let a = p.analysis().effects;
    let mut b = a;
    b.writes.x += 40;
    b.reads.x += 40;
    assert!(a.independent(b));
    b.reads = a.writes;
    assert!(!a.independent(b));
    b.reads.x += 40;
    b.barrier = true;
    assert!(!a.independent(b));
}
