use super::*;
use crate::{
    PaintProgram, PaintStats,
    painter::{Painter, RasterImage},
    text::{Blob, Font, FontData, Glyph, GlyphRun, Hinting, preparation::PreparedFonts},
};
use serde_json::json;
use std::{hint::black_box, num::NonZeroU32, sync::Arc};

#[allow(dead_code)]
mod bench {
    use crate as retinue_render;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/benchmarks/support.rs"
    ));
}

const FONT: &[u8] = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
const TARGET: (u32, u32) = (2048, 1024);

fn fixture(kind: &str, amount: u32, scale: f32) -> PaintProgram {
    let side = amount as f32;
    let bounds = Bounds::new(0.0, 0.0, side, side);
    if kind == "mixed" {
        let commands = [
            fixture("rounded", amount, scale),
            fixture("glow", amount, scale),
            fixture("text", amount / 4, scale),
        ]
        .into_iter()
        .flat_map(|program| program.commands().as_ref().clone())
        .collect();
        return PaintProgram::new(1, (1024.0, 256.0), scale, Arc::new(commands)).unwrap();
    }
    let command = match kind {
        "text" => {
            let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(FONT);
            let metrics = read_fonts::model::Font::new(bytes.clone(), 0).unwrap();
            let font = Font::new(FontData::new(Blob::new(bytes), 0)).unwrap();
            let glyphs = "Galvanized Aptitude 0123456789"
                .chars()
                .cycle()
                .take(amount as usize)
                .enumerate()
                .map(|(i, ch)| Glyph {
                    id: metrics.charmap().map_unicode(ch).unwrap().to_u32(),
                    x: i as f64 * 9.25,
                    y: 17.75,
                })
                .collect::<Vec<_>>();
            Command::GlyphRun {
                origin: (0.0, 0.0),
                run: GlyphRun::new(font, 16.0, glyphs, Hinting::None).unwrap(),
                color: Color::rgb(0xeeeeee),
            }
        }
        "glow" => Command::InsetShadow {
            bounds,
            radius: side / 2.0,
            blur: side / 10.0,
            spread: 1.0,
            color: Color::rgba(70, 210, 52, 220),
        },
        "solid" | "rounded" => Command::Fill {
            bounds,
            radius: if kind == "solid" { 0.0 } else { side / 4.0 },
            color: Color::rgba(70, 110, 180, 220),
        },
        "rounded-outline" => Command::Outline {
            bounds,
            radius: side / 4.0,
            thickness: 2.0,
            color: Color::rgba(70, 110, 180, 220),
        },
        "ring" => Command::Ring {
            center: (side / 2.0, side / 2.0),
            radius: side / 2.0 - 2.0,
            thickness: 2.0,
            color: Color::rgba(70, 110, 180, 220),
        },
        _ => unreachable!(),
    };
    PaintProgram::new(1, (1024.0, 256.0), scale, Arc::new(vec![command])).unwrap()
}

fn replay(program: &PaintProgram, fonts: &Arc<PreparedFonts>, p: &mut Painter<'_>, at: (i32, i32)) {
    let bounds = program.bounds();
    p.with_text_fonts(fonts, |p| {
        program.replay(
            p,
            (at.0 - bounds.x, at.1 - bounds.y),
            &mut |_, _| unreachable!(),
            true,
            true,
            &mut PaintStats::default(),
            &mut |_, _| None,
        );
    });
}

#[test]
#[ignore = "opt-in cache construction/replay/blit calibration"]
fn promotion_cost_benchmark() {
    let threads = bench::render_threads();
    let mut cases = Vec::new();
    for kind in [
        "solid",
        "rounded",
        "rounded-outline",
        "ring",
        "glow",
        "text",
        "mixed",
    ] {
        for amount in if kind == "text" {
            [8, 24, 48]
        } else {
            [32, 64, 128]
        } {
            for scale in [1.0, 1.2] {
                let program = fixture(kind, amount, scale);
                let bounds = program.bounds();
                let size = (bounds.width, bounds.height);
                let (fonts, _) = PreparedFonts::default().update([&program]);
                let fonts = Arc::new(fonts);
                let cached =
                    RasterImage::render(size.0, size.1, |p| replay(&program, &fonts, p, (0, 0)))
                        .unwrap();
                let columns = TARGET.0 / (size.0 + 4);
                let count = (columns * (TARGET.1 / (size.1 + 4))).min(48);
                assert!(count > 0);
                let work = program.cost(false).work(area(bounds));
                let payback = crate::cost::cache::CacheCost::new(work, area(bounds), threads.get())
                    .required_uses();
                for mode in ["build", "replay", "blit"] {
                    let name = format!("{kind}-{mode}");
                    if !bench::selected(&name) {
                        continue;
                    }
                    cases.push(bench::measure(
                        &name, size, scale,
                        || (vec![0; (TARGET.0 * TARGET.1 * 4) as usize], 1_u32),
                        |(pixels, actual_threads), _| {
                            if mode == "build" {
                                let images: Vec<_> = (0..8).map(|_| {
                                    RasterImage::render_with_threads(size.0, size.1, NonZeroU32::MIN,
                                        |p| replay(&program, &fonts, p, (0, 0))).unwrap()
                                }).collect();
                                black_box(images);
                            } else {
                                let mut p = Painter::with_threads(pixels, TARGET.0, TARGET.1, threads).unwrap();
                                *actual_threads = p.thread_count();
                                p.clear();
                                for i in 0..count {
                                    let x = i % columns * (size.0 + 4);
                                    let y = i / columns * (size.1 + 4);
                                    if mode == "blit" {
                                        p.draw_image(&cached, x, y, size.0, size.1);
                                    } else {
                                        replay(&program, &fonts, &mut p, (x as i32, y as i32));
                                    }
                                }
                                p.finish().unwrap();
                            }
                        },
                        |(_, actual_threads)| json!({"amount":amount, "operations":if mode == "build" { 8 } else { count },
                            "render_threads":actual_threads,
                            "bytes":size.0 * size.1 * 4,
                            "work":{"solid":work.solid,"blended":work.blended,"sampled":work.sampled,
                            "commands":work.commands,"edges":work.edges,"glyphs":work.glyphs,"native_glyphs":work.native_glyphs},
                            "required_uses":payback}),
                    ));
                }
            }
        }
    }
    bench::report_with_scope(
        "promotion",
        cases,
        "Cache construction and destruction of eight independent targets per sample, caller-only queued producer; fonts/programs warm. Replay and blit draw up to 48 instances into one 2048x1024 target using requested participants. Equal target clear/context work cancels only between replay and blit. Fixed-position native glyph fixture, not shaping or whole-UI performance. Reported size is one cache target, not the frame.",
    );
}

#[test]
fn curved_probe_fixtures_draw_hollow_geometry() {
    for kind in ["rounded-outline", "ring"] {
        for scale in [1.0, 1.2] {
            let program = fixture(kind, 64, scale);
            let bounds = program.bounds();
            let image = RasterImage::render(bounds.width, bounds.height, |p| {
                replay(&program, &Arc::default(), p, (0, 0));
            })
            .unwrap()
            .to_rgba()
            .unwrap();
            assert!(image.pixels().any(|p| p.0[3] > 0));
            assert_eq!(
                image.get_pixel(image.width() / 2, image.height() / 2).0[3],
                0
            );
        }
    }
}
