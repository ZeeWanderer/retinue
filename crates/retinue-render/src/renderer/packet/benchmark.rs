use super::*;
use crate::{Bounds, Command, PaintInstance};
use serde_json::json;
use std::hint::black_box;

#[allow(dead_code)]
mod bench {
    use crate as retinue_render;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/benchmarks/support.rs"
    ));
}

#[test]
#[ignore = "packet construction and clip query costs; use make bench BENCH_SUITE=packet-work"]
fn packet_work_benchmark() {
    let mut cases = Vec::new();
    let size = (1024, 1);
    let rect = |x, width| Rect {
        x,
        y: 0,
        width,
        height: 1,
    };
    for count in [32, 128, 512] {
        let name = format!("freeze-{count}");
        if !bench::selected(&name) {
            continue;
        }
        let repair: Region = rect(0, 1).into();
        let program = PaintProgram::new(
            1,
            (1., 1.),
            1.,
            Arc::new(vec![Command::Fill {
                bounds: Bounds::new(0., 0., 1., 1.),
                radius: 0.,
                color: Color::rgb(0x278aca),
            }]),
        )
        .unwrap();
        let mut frame = PaintFrame::default();
        frame
            .replace(
                size,
                1.,
                (0..count)
                    .map(|i| {
                        PaintInstance::new(
                            Key::new("pixel", i as u64),
                            program.clone(),
                            (i, 0),
                            rect(0, size.0),
                        )
                    })
                    .collect(),
                vec![],
            )
            .unwrap();
        let mut state = Renderer::new(ResidencyPolicy::desktop(0));
        // Measure packet assembly, not repeated hits on the same prepared packet.
        state.set_leaf_packet_reuse(false);
        state.prepare_for(&frame, RasterMode::Replay, &repair);
        let frame = frame.freeze();
        let freeze = |r: &mut Renderer| {
            r.freeze_frame(&frame, &repair, |_| Ok(Default::default()))
                .unwrap()
        };
        assert_eq!(freeze(&mut state).draws.len(), 1);
        let mut case = bench::measure(
            &name,
            size,
            1.,
            || state,
            |state, _| {
                black_box(freeze(state));
            },
            |state| json!({"draw_steps":state.draw_plan.len(),"emitted_programs":1,"damaged_pixels":1}),
        );
        case["actual_threads"] = json!(1);
        cases.push(case);
    }
    for count in [1, 8] {
        let region = Region::new((0..count).map(|n| rect(n * 2, 1)));
        for materialize in [false, true] {
            let name = format!(
                "clip-{}-{count}",
                if materialize { "region" } else { "count" }
            );
            if !bench::selected(&name) {
                continue;
            }
            let mut case = bench::measure(
                &name,
                size,
                1.,
                || vec![0; size.0 as usize * 4],
                |pixels, _| {
                    let mut p = Painter::new(pixels, size.0, size.1).unwrap();
                    p.repair(&region);
                    for _ in 0..512 {
                        let p = black_box(&p);
                        black_box(if materialize {
                            p.visible_regions().rects().len() > 1
                        } else {
                            p.has_multiple_clips()
                        });
                    }
                    p.finish().unwrap();
                },
                |_| json!({"clip_queries":512, "clip_count":count}),
            );
            case["actual_threads"] = json!(1);
            cases.push(case);
        }
    }
    bench::report_with_scope(
        "packet-work",
        cases,
        "Freeze prepared non-nested replay plans for a fixed one-pixel repair. Clip cases compare 512 clip-count queries with identical painter setup and no pixel work. Measures owner and query cost, not frame time.",
    );
}
