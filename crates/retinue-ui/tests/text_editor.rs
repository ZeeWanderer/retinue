use retinue_render::{
    Rect,
    painter::{CachedFont, Painter},
};
use retinue_ui::line_edit::{Colors, LineEdit};
use serde_json::json;
use std::{hint::black_box, path::PathBuf};

#[allow(dead_code)]
#[path = "../../../tools/benchmarks/support.rs"]
mod bench;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf");
const LATO: &[u8] = include_bytes!("../../../apps/poe2companion/assets/Lato-Regular.ttf");
const TEXT: &str = "Mesa Prime Neuroptics Blueprint";
const COLORS: Colors = Colors {
    text: [240, 240, 240, 255],
    selection: [110, 70, 35, 255],
};

fn font(bytes: &[u8]) -> CachedFont {
    CachedFont::from_bytes(bytes).unwrap()
}

#[test]
#[ignore = "editor measurement; use make bench BENCH_SUITE=text-editor"]
fn text_editor_benchmark() {
    let font = font(ROBOTO);
    let mut cases = Vec::new();
    for scale in [1.0, 1.2, 1.75] {
        let width = (220.0 * scale) as u32;
        let height = (36.0 * scale) as u32;
        for name in [
            "editor-repaint",
            "editor-selection",
            "editor-viewport",
            "editor-insert",
        ] {
            if !bench::selected(name) {
                continue;
            }
            let mut case = bench::measure(
                name,
                (width, height),
                scale,
                || {
                    let mut editor = LineEdit::new(&font).unwrap();
                    editor.set_text(TEXT, false);
                    (editor, vec![0; (width * height * 4) as usize])
                },
                |(editor, pixels), index| {
                    let mut rect = Rect {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    };
                    match name {
                        "editor-selection" => {
                            editor.key(if index % 2 == 0 { 0xff51 } else { 0xff53 }, false, true);
                        }
                        "editor-viewport" => {
                            if index % 2 == 0 {
                                rect.width /= 2;
                            }
                        }
                        "editor-insert" => {
                            if index % 2 == 0 {
                                editor.insert("!", |_| true);
                            } else {
                                editor.key(0xff08, false, false);
                            }
                        }
                        _ => {}
                    }
                    let mut painter =
                        Painter::with_threads(pixels, width, height, bench::render_threads())
                            .unwrap();
                    painter.fill_rounded_rect(0, 0, width, height, 0, [24, 24, 24, 255]);
                    editor.paint(&mut painter, rect, 14.0 * scale, COLORS);
                    painter.finish().unwrap();
                    black_box(pixels);
                },
                |_| json!({ "source_bytes": TEXT.len(), "fields": 1 }),
            );
            case["actual_threads"] = json!(bench::render_threads().get());
            case["items"] = json!(1);
            cases.push(case);
        }
    }
    bench::report_with_scope(
        "native-text-editor",
        cases,
        "One single-line editor update and complete native paint per sample, including context creation and completion but no platform presentation. Font registration is warm. Repaint, selection and viewport cases retain shaped glyphs; insert reshapes changed text.",
    );
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "host-font discovery timing; run separately in a fresh release process"]
fn host_font_discovery_benchmark() {
    assert!(!cfg!(debug_assertions), "use a release build");
    let rect = Rect {
        x: 0,
        y: 0,
        width: 240,
        height: 40,
    };
    let paint = |edit: &mut LineEdit, pixels: &mut [u8]| {
        let mut p = Painter::with_threads(pixels, rect.width, rect.height, bench::render_threads())
            .unwrap();
        edit.paint(&mut p, rect, 16.8, COLORS);
        p.finish().unwrap();
    };
    const MIXED: [&str; 2] = [
        "A \u{6f22} \u{633}\u{644}\u{627}\u{645} \u{1f600}",
        "B \u{6f22} \u{633}\u{644}\u{627}\u{645} \u{1f600}",
    ];
    let run = |name: &str| {
        bench::measure(
            name,
            (rect.width, rect.height),
            1.2,
            || {
                let mut edit = LineEdit::new(&font(ROBOTO)).unwrap();
                let mut pixels = vec![0; (rect.width * rect.height * 4) as usize];
                edit.set_text("A", false);
                paint(&mut edit, &mut pixels);
                (edit, pixels)
            },
            |(edit, pixels), i| {
                edit.set_text(MIXED[i % 2], false);
                paint(edit, pixels);
            },
            |_| json!({"fields":1}),
        )
    };
    let preparation = if std::env::var_os("RETINUE_BENCH_WARM_FONT_CATALOG").is_some() {
        let narrow = retinue_render::RenderHost::new(retinue_render::HostPolicy {
            threads: 1,
            ..retinue_render::HostPolicy::desktop()
        });
        assert!(retinue_ui::drawing::text::prepare_host_fonts(&narrow).is_none());
        let host = retinue_render::RenderHost::new(retinue_render::HostPolicy::desktop());
        let started = std::time::Instant::now();
        let job = retinue_ui::drawing::text::prepare_host_fonts(&host)
            .expect("needs a spare participant");
        let submitted = started.elapsed();
        assert_eq!(job.wait().unwrap().completed, 1);
        let ready = started.elapsed();
        assert!(retinue_ui::drawing::text::prepare_host_fonts(&host).is_none());
        Some((submitted, ready))
    } else {
        None
    };
    let mut cold = run(if preparation.is_some() {
        "prepared-catalog"
    } else {
        "fresh-process"
    });
    if let Some((submitted, ready)) = preparation {
        cold["catalog_dispatch_ns"] = json!(submitted.as_nanos() as u64);
        cold["catalog_ready_ns"] = json!(ready.as_nanos() as u64);
    }
    let mut live = LineEdit::new(&font(ROBOTO)).unwrap();
    live.set_text(MIXED[0], false);
    let peer =
        std::thread::scope(|scope| scope.spawn(|| run("new-owner-live-faces")).join().unwrap());
    drop(live);
    bench::report_with_scope(
        "host-font-discovery",
        vec![cold, peer],
        "First mixed-script edit in a fresh process or on a fresh owner while another editor retains the selected faces. Includes font selection, shaping and completed paint; bundled font and painter warm. Host font inventory and OS page cache affect results.",
    );
}

#[test]
#[ignore = "editor captures; set RETINUE_TEXT_CAPTURE_DIR"]
fn text_editor_capture() {
    let output = PathBuf::from(std::env::var_os("RETINUE_TEXT_CAPTURE_DIR").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for (name, bytes) in [("roboto", ROBOTO), ("lato", LATO)] {
        let font = font(bytes);
        for scale in [1.0, 1.2, 1.75] {
            let width = (360.0 * scale) as u32;
            let height = (256.0 * scale) as u32;
            let mut pixels = vec![0; (width * height * 4) as usize];
            let mut painter =
                Painter::with_threads(&mut pixels, width, height, 4.try_into().unwrap()).unwrap();
            painter.fill_rounded_rect(0, 0, width, height, 0, [24, 24, 24, 255]);
            for (index, text) in [
                "Mesa Prime cafe\u{301} 123",
                "-12.5",
                TEXT,
                "secret e\u{301}",
                "Astra  \u{6f22}  \u{633}\u{644}\u{627}\u{645}  \u{915}\u{93f}  \u{1f600}",
            ]
            .into_iter()
            .enumerate()
            {
                let rect = Rect {
                    x: (16.0 * scale) as u32,
                    y: ((12.0 + index as f32 * 48.0) * scale) as u32,
                    width: (328.0 * scale) as u32,
                    height: (36.0 * scale) as u32,
                };
                painter.fill_rounded_rect(
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    (4.0 * scale) as u32,
                    [48, 48, 48, 255],
                );
                let mut editor = if index == 3 {
                    LineEdit::password(&font)
                } else {
                    LineEdit::new(&font)
                }
                .unwrap();
                editor.set_text(text, index == 1);
                let rect = Rect {
                    x: rect.x + (6.0 * scale) as u32,
                    width: if index == 2 {
                        (110.0 * scale) as u32
                    } else {
                        rect.width - (12.0 * scale) as u32
                    },
                    ..rect
                };
                editor.paint(&mut painter, rect, 14.0 * scale, COLORS);
            }
            painter.finish().unwrap();
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            image::RgbaImage::from_raw(width, height, pixels)
                .unwrap()
                .save(output.join(format!("editor-{name}-{}.png", (scale * 100.0) as u32)))
                .unwrap();
        }
    }
}
