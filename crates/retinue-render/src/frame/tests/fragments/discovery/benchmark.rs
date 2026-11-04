use super::*;
use rustix::time::{ClockId, clock_gettime};
use std::{io::Write, num::NonZeroU32, time::Instant};

fn cpu() -> u64 {
    let t = clock_gettime(ClockId::ProcessCPUTime);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

#[test]
#[ignore = "manual timing experiment; requires REUSE_REPORT"]
fn automatic_reuse_benchmark() {
    let path = std::env::var("REUSE_REPORT").expect("REUSE_REPORT");
    let mut output = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for scale in [1.0, 1.25, 2.0] {
        for count in [1, 72] {
            for threads in [2, 4] {
                let mut frame = PaintFrame::default();
                unhinted(&mut frame, &mut ProgramStore::default(), scale, count);
                let mut renderers = [
                    Renderer::new(ResidencyPolicy::desktop(0)),
                    Renderer::new(ResidencyPolicy::desktop(0)),
                ];
                renderers[0].set_materializations(false);
                let size = frame.size();
                let clip = Rect {
                    width: size.0,
                    height: size.1,
                    ..Rect::default()
                };
                let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
                for sample in 0..240 {
                    let mut items = frame.instances().to_vec();
                    for (index, item) in items.iter_mut().enumerate() {
                        item.set_pixels(
                            (
                                ((8.0 + (index % 12) as f32 * 67.0) * scale).round() as i32,
                                ((13.0 + (index / 12) as f32 * 67.0) * scale).round() as i32
                                    + sample % 3,
                            ),
                            clip,
                        );
                    }
                    frame.replace(size, scale, items, Vec::new()).unwrap();
                    for order in 0..2 {
                        let mode = (order + sample as usize) % 2;
                        let renderer = &mut renderers[mode];
                        let host = renderer.host().clone();
                        let cpu_start = cpu();
                        let start = Instant::now();
                        let pass = host.begin_frame(NonZeroU32::new(threads).unwrap());
                        renderer.prepare(&frame, RasterMode::Replay);
                        let prepared = Instant::now();
                        let mut p =
                            Painter::with_threads(&mut pixels, size.0, size.1, pass.threads())
                                .unwrap();
                        renderer.paint(&frame, &mut p, clip, Color::rgb(0x10151b));
                        let submitted = Instant::now();
                        p.finish().unwrap();
                        drop(pass);
                        let end = Instant::now();
                        let cpu_ns = cpu() - cpu_start;
                        if sample >= 16 {
                            assert_eq!(renderer.fragments.selected.len(), mode);
                            writeln!(output, "{{\"scale\":{scale},\"count\":{count},\"threads\":{threads},\"mode\":{mode},\"sample\":{sample},\"prepare_ns\":{},\"submit_ns\":{},\"finish_ns\":{},\"total_ns\":{},\"cpu_ns\":{cpu_ns},\"blits\":{},\"selected_bytes\":{}}}",
                                prepared.duration_since(start).as_nanos(), submitted.duration_since(prepared).as_nanos(), end.duration_since(submitted).as_nanos(), end.duration_since(start).as_nanos(), renderer.stats.fragment_blits, renderer.residency_stats().selected_bytes).unwrap();
                        }
                    }
                }
            }
        }
    }
}
