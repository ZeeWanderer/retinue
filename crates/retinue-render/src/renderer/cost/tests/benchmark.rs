use super::*;
use crate::{
    ImageFit, ImageSource, TextAlignment,
    painter::{GlyphCache, PixelSource, RasterImage, load_overlay_font},
};
use std::{io::Write, num::NonZeroU32, time::Instant};

fn fixture(kind: &str, side: u32) -> PaintFrame {
    let size = (2560, 1440);
    let full = Rect {
        width: size.0,
        height: size.1,
        ..Rect::default()
    };
    let font = Rc::new(load_overlay_font(&GlyphCache::new(8 * 1024 * 1024)).unwrap());
    let image: ImageSource = Rc::new(
        RasterImage::render(side, side, |p| {
            p.fill_rounded_rect(0, 0, side, side, side / 8, [100, 130, 150, 170]);
        })
        .unwrap(),
    )
    .into();
    let columns = (size.0 - 32) / (side + 8);
    let rows = (size.1 - 32) / (side + 8);
    let mut items = Vec::new();
    let mut store = ProgramStore::default();
    for n in 0..(columns * rows).min(96) {
        let bounds = Bounds::new(0.0, 0.0, side as f32, side as f32);
        let mut commands = Vec::new();
        if matches!(kind, "fill" | "rounded" | "mixed") {
            commands.push(Command::Fill {
                bounds,
                radius: if kind == "fill" { 0.0 } else { 8.0 },
                color: Color::rgb(0x293549),
            });
        }
        if matches!(kind, "image" | "mixed") {
            commands.push(Command::Image {
                bounds,
                image: image.clone(),
                fit: ImageFit::Contain,
            });
        }
        if matches!(kind, "text" | "mixed") {
            for row in 0..3 {
                commands.push(Command::Text {
                    bounds: Bounds::new(3.0, 3.0 + row as f32 * 18.0, side as f32 - 6.0, 18.0),
                    size: 14.0,
                    text: format!("Item {n} row {row}"),
                    font: font.clone(),
                    color: Color::rgb(0xdee4ed),
                    align: TextAlignment::Top,
                });
            }
        }
        let program = store
            .record(
                u64::from(n),
                (side as f32, side as f32),
                1.0,
                Rc::new(commands),
            )
            .unwrap();
        items.push(PaintInstance::new(
            u64::from(n).into(),
            program,
            (
                (16 + n % columns * (side + 8)) as i32,
                (16 + n / columns * (side + 8)) as i32,
            ),
            full,
        ));
    }
    let mut frame = PaintFrame::default();
    frame.replace(size, 1.0, items, Vec::new()).unwrap();
    frame
}

#[test]
#[ignore = "opt-in native replay/copy calibration; not a timing correctness gate"]
fn native_cost_benchmark() {
    let path = std::env::var("COST_REPORT").expect("COST_REPORT");
    let mut output = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(output, "kind,side,threads,mode,sample,total_ns,submit_ns,finish_ns,estimated,items,area,solid,blended,sampled,commands,edges,glyphs,copy_estimated").unwrap();
    for kind in ["fill", "rounded", "text", "image", "mixed"] {
        for side in [64, 128, 192, 384, 512, 768] {
            let frame = fixture(kind, side);
            let size = frame.size();
            let full = Rect {
                width: size.0,
                height: size.1,
                ..Rect::default()
            };
            let region = Region::new([frame
                .instances()
                .iter()
                .map(PaintInstance::visible_bounds)
                .fold(Rect::default(), Rect::union)]);
            let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
            renderer.set_materializations(false);
            renderer.prepare(&frame, RasterMode::Replay);
            let mut scene = Snapshot::new(size, BACKGROUND);
            scene.record_rendered(&frame, &renderer, RasterMode::Replay, |_, _| false);
            let work = scene.repair_work(&region);
            let mut source = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut painter = Painter::new(&mut source, size.0, size.1).unwrap();
            renderer.paint(&frame, &mut painter, full, BACKGROUND);
            painter.finish().unwrap();
            let mut pixels = source.clone();
            let source = PixelSource::new(&source, size.0, size.1).unwrap();
            for threads in [1, 2, 4, 8] {
                let execution = crate::cost::Execution {
                    threads,
                    copy: crate::cost::CopyExecution::Queued,
                };
                let estimate = execution.replay(work);
                let copy_estimate = execution.transfer(region.area() * 4, region.area() * 4, 1);
                for sample in 0..112 {
                    for mode in if sample % 2 == 0 {
                        ["paint", "copy"]
                    } else {
                        ["copy", "paint"]
                    } {
                        let start = Instant::now();
                        let mut p = Painter::with_threads(
                            &mut pixels,
                            size.0,
                            size.1,
                            NonZeroU32::new(threads).unwrap(),
                        )
                        .unwrap();
                        if mode == "copy" {
                            for rect in region.rects() {
                                p.copy_source(&source, *rect, (rect.x, rect.y));
                            }
                        } else {
                            p.repair(&region);
                            renderer.paint(&frame, &mut p, full, BACKGROUND);
                        }
                        let submitted = Instant::now();
                        p.finish().unwrap();
                        let done = Instant::now();
                        if sample >= 16 {
                            writeln!(output, "{kind},{side},{threads},{mode},{sample},{},{},{},{estimate},{},{},{},{},{},{},{},{},{copy_estimate}",
                                (done-start).as_nanos(), (submitted-start).as_nanos(), (done-submitted).as_nanos(), frame.instances().len(), region.area(), work.solid, work.blended, work.sampled, work.commands, work.edges, work.glyphs).unwrap();
                        }
                    }
                }
            }
        }
    }
}
