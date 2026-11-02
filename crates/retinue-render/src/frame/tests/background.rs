use super::*;
use crate::{
    AnalysisPolicy, HostPolicy, OptimizationContext, PaintStats, RenderHost, ResidencyPolicy,
};
use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

fn source(scale: f32, revision: u64) -> PaintProgram {
    let image = Rc::new(
        RasterImage::render(10, 10, |p| {
            p.fill_rounded_rect(0, 0, 10, 10, 3, [50, 120, 240, 200])
        })
        .unwrap(),
    );
    let font = Rc::new(
        crate::painter::load_overlay_font(&crate::painter::GlyphCache::new(1024 * 1024)).unwrap(),
    );
    let commands = (0..1024)
        .map(|i| {
            let bounds = Bounds::new((i % 32 * 10) as f32, (i / 32 * 10) as f32, 13.0, 13.0);
            match i % 67 {
                1 => Command::Image {
                    bounds,
                    image: image.clone().into(),
                    fit: ImageFit::Stretch,
                },
                2 => Command::Text {
                    bounds,
                    size: 9.0,
                    text: "Aa".into(),
                    font: font.clone(),
                    color: Color::rgb(0xffffff),
                    align: crate::TextAlignment::Top,
                },
                3 => Command::Clip {
                    bounds,
                    commands: Rc::new(vec![Command::Fill {
                        bounds,
                        radius: 1.0,
                        color: Color::rgba(220, 180, 0, 170),
                    }]),
                },
                4 => Command::InsetShadow {
                    bounds,
                    radius: 4.0,
                    blur: 3.0,
                    spread: 0.0,
                    color: Color::rgba(40, 220, 80, 120),
                },
                _ => Command::Fill {
                    bounds,
                    radius: 2.0,
                    color: if revision == 0 {
                        Color::rgba(120, 40, 160, 160)
                    } else {
                        Color::rgba(20, 140, 60, 140)
                    },
                },
            }
        })
        .collect();
    PaintProgram::new(
        PaintKey::new(1.into(), revision),
        (320.0, 320.0),
        scale,
        Rc::new(commands),
    )
    .unwrap()
}

fn render(frame: &PaintFrame, renderer: &mut Renderer, repair: Rect) -> Vec<u8> {
    let host = renderer.host().clone();
    let pass = host.begin_frame(NonZeroU32::MIN);
    renderer.prepare(frame, RasterMode::Replay);
    let mut pixels = vec![0; 800 * 800 * 4];
    let mut painter = Painter::with_threads(&mut pixels, 800, 800, pass.threads()).unwrap();
    painter.clip_to_rect(repair);
    renderer.paint(frame, &mut painter, repair, Color::rgb(0x121419));
    painter.finish().unwrap();
    pixels
}

fn settle(host: &RenderHost) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.stats().analysis.jobs != 0 {
        host.maintain(Instant::now());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn deferred_index_matches_raw_pixels_through_clips_scales_motion_and_replacement() {
    for scale in [1.0, 1.25, 2.0] {
        let host = RenderHost::new(HostPolicy::desktop());
        let mut cached = Renderer::new(ResidencyPolicy::desktop(0));
        cached.set_host(host.clone());
        cached.set_materializations(false);
        let mut direct = Renderer::new(ResidencyPolicy::desktop(0));
        direct.set_optimizations(false);
        let mut frame = PaintFrame::default();
        let clip = Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 800,
        };
        for revision in 0..2 {
            let program = source(scale, revision);
            frame
                .replace(
                    (800, 800),
                    scale,
                    vec![PaintInstance::new(
                        1.into(),
                        program.clone(),
                        (15, 23),
                        clip,
                    )],
                    vec![],
                )
                .unwrap();
            let repair = Rect {
                x: 20,
                y: 30,
                width: 30,
                height: 35,
            };
            assert_eq!(
                render(&frame, &mut cached, repair),
                render(&frame, &mut direct, repair)
            );
            settle(&host);
            for origin in [(15, 23), (-9, -12), (20, 16)] {
                frame
                    .replace(
                        (800, 800),
                        scale,
                        vec![PaintInstance::new(1.into(), program.clone(), origin, clip)],
                        vec![],
                    )
                    .unwrap();
                assert_eq!(
                    render(&frame, &mut cached, repair),
                    render(&frame, &mut direct, repair),
                    "scale {scale}, origin {origin:?}"
                );
                if host.stats().analysis.submitted > 0 {
                    assert!(cached.stats.indexed_commands_skipped > 800);
                    assert!(cached.stats.indexed_nodes < 100);
                }
            }
        }
    }
}

#[test]
fn indexed_fragments_preserve_external_callback_order_and_raw_escape_path() {
    let host = RenderHost::new(HostPolicy::desktop());
    let pass = host.begin_frame(NonZeroU32::MIN);
    let original = source(1.25, 0);
    let mut commands = original.commands().as_ref().clone();
    for (offset, key) in [(200, 2), (101, 3)] {
        commands.insert(
            offset,
            Command::External {
                bounds: Bounds::new(0.0, 0.0, 400.0, 400.0),
                key: key.into(),
            },
        );
    }
    let program = PaintProgram::new(5, (320.0, 320.0), 1.25, Rc::new(commands)).unwrap();
    assert!(host.analyze(&program));
    drop(pass);
    settle(&host);
    let command = Command::Fragment {
        program,
        context: OptimizationContext::default(),
    };
    let mut outputs = Vec::new();
    for optimized in [false, true] {
        let mut pixels = vec![0; 800 * 800 * 4];
        let mut painter = Painter::new(&mut pixels, 800, 800).unwrap();
        painter.clip_to_rect(Rect {
            x: 25,
            y: 31,
            width: 35,
            height: 40,
        });
        let mut stats = PaintStats::default();
        let mut callbacks = Vec::new();
        command.paint(
            &mut painter,
            1.25,
            (-3, -7),
            &mut |key, p| {
                callbacks.push(key);
                p.fill_rounded_rect(25, 31, 35, 40, 0, [20, 80, 30, 100]);
            },
            optimized,
            &mut stats,
            &mut |_, _| None,
        );
        painter.finish().unwrap();
        assert_eq!(callbacks, vec![3.into(), 2.into()]);
        if optimized && host.stats().analysis.submitted != 0 {
            assert!(stats.indexed_commands_skipped > 800);
        }
        outputs.push(pixels);
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn queue_pressure_retries_current_programs_without_waking_idle_hosts() {
    let host = RenderHost::new(HostPolicy {
        analysis: AnalysisPolicy {
            jobs: 1,
            ..AnalysisPolicy::desktop()
        },
        ..HostPolicy::desktop()
    });
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.set_host(host.clone());
    renderer.set_materializations(false);
    let clip = Rect {
        x: 0,
        y: 0,
        width: 800,
        height: 800,
    };
    let mut frame = PaintFrame::default();
    frame
        .replace(
            (800, 800),
            1.0,
            vec![
                PaintInstance::new(1.into(), source(1.0, 0), (0, 0), clip),
                PaintInstance::new(2.into(), source(1.0, 1), (0, 0), clip),
            ],
            vec![],
        )
        .unwrap();
    render(&frame, &mut renderer, clip);
    let submitted = host.stats().analysis.submitted;
    assert!(!host.has_deferred());
    assert_eq!(host.maintenance_deadline(), None);
    settle(&host);
    render(&frame, &mut renderer, clip);
    settle(&host);
    assert_eq!(host.stats().analysis.published, submitted * 2);
    assert_eq!(host.stats().analysis.reserved_bytes, 0);
}
