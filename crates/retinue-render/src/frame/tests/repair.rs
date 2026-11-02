use super::*;
use crate::damage::Region;
#[cfg(feature = "benchmark")]
use crate::{
    ImageSource, TextAlignment,
    painter::{GlyphCache, load_overlay_font},
};
#[cfg(feature = "benchmark")]
use std::num::NonZeroU32;

#[cfg(feature = "benchmark")]
fn fixture(scale: f32) -> PaintFrame {
    let size = ((2560.0 * scale) as u32, (1600.0 * scale) as u32);
    let clip = Rect {
        width: size.0,
        height: size.1,
        ..Rect::default()
    };
    let font = Rc::new(load_overlay_font(&GlyphCache::new(8 * 1024 * 1024)).unwrap());
    let image: ImageSource = Rc::new(
        RasterImage::render(80, 80, |p| {
            p.fill_rounded_rect(0, 0, 80, 80, 12, [160, 100, 60, 180]);
            p.draw_ring(40, 40, 30, 5, [80, 240, 180, 230]);
        })
        .unwrap(),
    )
    .into();
    let items = (0..90u64)
        .map(|i| {
            let mut commands = vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 236.0, 142.0),
                    radius: 7.0,
                    color: Color::rgba(40, 47, 61, 220),
                },
                Command::Image {
                    bounds: Bounds::new(12.0, 12.0, 80.0, 80.0),
                    image: image.clone(),
                    fit: ImageFit::Contain,
                },
            ];
            for row in 0..4 {
                commands.push(Command::Text {
                    bounds: Bounds::new(10.0, 66.0 + row as f32 * 17.0, 216.0, 18.0),
                    size: 14.0,
                    text: format!("Item {i} status row {row}"),
                    font: font.clone(),
                    color: Color::rgba(220, 224, 231, 235),
                    align: TextAlignment::Top,
                });
            }
            let program = PaintProgram::new(
                PaintKey::new((i + 1).into(), 0),
                (236.0, 142.0),
                scale,
                Rc::new(commands),
            )
            .unwrap();
            let mut item = PaintInstance::new(
                (i + 1).into(),
                program,
                (
                    ((20 + i % 10 * 250) as f32 * scale).round() as i32,
                    ((20 + i / 10 * 160) as f32 * scale).round() as i32,
                ),
                clip,
            );
            item.cache = CachePolicy::Prefer;
            item
        })
        .collect();
    let mut frame = PaintFrame::default();
    frame.replace(size, scale, items, Vec::new()).unwrap();
    frame
}

#[test]
fn sparse_scene_repair_culls_gaps_and_calls_external_paint_once() {
    let size = (480, 220);
    let full = Rect {
        width: size.0,
        height: size.1,
        ..Rect::default()
    };
    let sparse = Region::new([
        Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 25,
        },
        Rect {
            x: 420,
            y: 160,
            width: 35,
            height: 30,
        },
    ]);
    let commands = Rc::new(vec![
        Command::Fill {
            bounds: Bounds::new(0.0, 0.0, 480.0, 220.0),
            radius: 12.0,
            color: Color::rgba(70, 100, 90, 220),
        },
        Command::Fill {
            bounds: Bounds::new(100.0, 80.0, 100.0, 80.0),
            radius: 9.0,
            color: Color::rgba(190, 70, 90, 170),
        },
        Command::External {
            bounds: Bounds::new(0.0, 0.0, 480.0, 220.0),
            key: 2.into(),
        },
    ]);
    let program =
        PaintProgram::new(PaintKey::new(1.into(), 0), (480.0, 220.0), 1.0, commands).unwrap();
    let mut frame = PaintFrame::default();
    frame
        .replace(
            size,
            1.0,
            vec![PaintInstance::new(1.into(), program, (0, 0), full)],
            Vec::new(),
        )
        .unwrap();
    let mut renderer = Renderer::new(crate::ResidencyPolicy::desktop(0));
    renderer.prepare(&frame, RasterMode::Replay);
    let mut expected = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut p = Painter::new(&mut expected, size.0, size.1).unwrap();
    let external = |_: Key, p: &mut Painter<'_>| p.draw_ring(36, 34, 25, 3, [100, 250, 170, 170]);
    renderer.paint_with(&frame, &mut p, full, Color::rgb(0x111822), external);
    p.finish().unwrap();
    let mut actual = expected.clone();
    for r in sparse.rects() {
        for y in r.y..r.y + r.height {
            let start = ((y * size.0 + r.x) * 4) as usize;
            actual[start..start + r.width as usize * 4].fill(0);
        }
    }
    renderer.prepare(&frame, RasterMode::Replay);
    let mut p = Painter::new(&mut actual, size.0, size.1).unwrap();
    p.repair(&sparse);
    let mut calls = 0;
    renderer.paint_with(&frame, &mut p, full, Color::rgb(0x111822), |k, p| {
        calls += 1;
        external(k, p);
    });
    p.finish().unwrap();
    assert_eq!(calls, 1);
    assert!(renderer.stats.culled_commands + renderer.stats.indexed_commands_skipped > 0);
    let mut rect_clipped = expected.clone();
    let mut p = Painter::new(&mut rect_clipped, size.0, size.1).unwrap();
    for &r in sparse.rects() {
        renderer.paint_with(&frame, &mut p, r, Color::rgb(0x111822), external);
    }
    p.finish().unwrap();
    assert!(
        actual == rect_clipped,
        "region submission differs from native rectangle clipping"
    );
    for (i, (a, b)) in actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .enumerate()
    {
        let (x, y) = (i as u32 % size.0, i as u32 / size.0);
        let inside = sparse.intersects(Rect {
            x,
            y,
            width: 1,
            height: 1,
        });
        // Native curve clipping can change an antialiased channel by one unit.
        assert!(
            a.iter()
                .zip(b)
                .all(|(a, b)| a.abs_diff(*b) <= u8::from(inside)),
            "{x},{y}: {a:?} != {b:?}"
        );
    }
}

#[cfg(feature = "benchmark")]
#[test]
#[ignore = "writes renderer repair timings; run through make bench-repair"]
fn repair_benchmark() {
    use rustix::time::{ClockId, clock_gettime};
    use std::{io::Write, time::Instant};
    let cpu = |id| {
        let t = clock_gettime(id);
        t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
    };
    let path = std::env::var("RETINUE_REPAIR_REPORT").expect("RETINUE_REPAIR_REPORT");
    let mut output = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for scale in [1.0_f32, 1.25, 2.0] {
        let frame = fixture(scale);
        let size = frame.size;
        let full = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let r = |x, y, width, height| Rect {
            x: (x as f32 * scale).round() as u32,
            y: (y as f32 * scale).round() as u32,
            width: (width as f32 * scale).round() as u32,
            height: (height as f32 * scale).round() as u32,
        };
        let cases = [
            Region::new([r(30, 35, 55, 60), r(2300, 1320, 60, 60)]),
            Region::new([r(30, 35, 2200, 12), r(30, 95, 2200, 12)]),
            Region::new([r(30, 35, 12, 1330), r(2300, 35, 12, 1330)]),
            Region::from(r(25, 25, 2300, 1350)),
            Region::new((0..192).map(|i| r(20 + i % 16 * 150, 20 + i / 16 * 120, 12, 12))),
        ];
        for threads in [1, 2, 4] {
            for cache in [false, true] {
                let mut renderer =
                    Renderer::new(crate::ResidencyPolicy::desktop(128 * 1024 * 1024));
                let mode = if cache {
                    RasterMode::Explicit
                } else {
                    RasterMode::Replay
                };
                let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
                for (case, precise) in cases.iter().enumerate() {
                    let coarse = Region::new(precise.rects().iter().map(|r| {
                        let x = r.x / 128 * 128;
                        let y = r.y / 32 * 32;
                        Rect {
                            x,
                            y,
                            width: (r.x + r.width).div_ceil(128) * 128 - x,
                            height: (r.y + r.height).div_ceil(32) * 32 - y,
                        }
                        .intersection(full)
                    }));
                    let candidates = [
                        precise.clone(),
                        precise.clone(),
                        coarse,
                        precise.bounds().into(),
                        full.into(),
                    ];
                    for sample in 0..44 {
                        for index in 0..5 {
                            let position = if sample % 2 == 0 { index } else { 4 - index };
                            let strategy = ([0, 1, 4, 2, 3][position] + sample / 2) % 5;
                            let repair = &candidates[strategy];
                            let process = cpu(ClockId::ProcessCPUTime);
                            let caller = cpu(ClockId::ThreadCPUTime);
                            let start = Instant::now();
                            let host = renderer.host().clone();
                            let native_frame = host.begin_frame(NonZeroU32::new(threads).unwrap());
                            renderer.prepare(&frame, mode);
                            let prepared = Instant::now();
                            let mut p = Painter::with_threads(
                                &mut pixels,
                                size.0,
                                size.1,
                                native_frame.threads(),
                            )
                            .unwrap();
                            let actual_threads = p.thread_count();
                            let created = Instant::now();
                            if strategy == 1 {
                                for &region in repair.rects() {
                                    renderer.paint(&frame, &mut p, region, Color::rgb(0x111822));
                                }
                            } else {
                                p.repair(repair);
                                renderer.paint(
                                    &frame,
                                    &mut p,
                                    repair.bounds(),
                                    Color::rgb(0x111822),
                                );
                            }
                            let submitted = Instant::now();
                            p.finish().unwrap();
                            drop(native_frame);
                            let finished = Instant::now();
                            let process = cpu(ClockId::ProcessCPUTime) - process;
                            let caller = cpu(ClockId::ThreadCPUTime) - caller;
                            if sample >= 4 {
                                writeln!(output, "{{\"scale\":{scale},\"threads\":{threads},\"actual_threads\":{actual_threads},\"cached\":{cache},\"case\":{case},\"strategy\":{strategy},\"sample\":{},\"prepare_ns\":{},\"create_ns\":{},\"submit_ns\":{},\"finish_ns\":{},\"caller_ns\":{caller},\"process_ns\":{process},\"commands\":{},\"culled\":{},\"blits\":{},\"pixels\":{},\"regions\":{}}}",
                                    sample - 4, prepared.duration_since(start).as_nanos(), created.duration_since(prepared).as_nanos(), submitted.duration_since(created).as_nanos(), finished.duration_since(submitted).as_nanos(), renderer.stats.replayed_commands, renderer.stats.culled_commands + renderer.stats.indexed_commands_skipped, renderer.stats.blits, repair.area(), repair.rects().len()).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}
