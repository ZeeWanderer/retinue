use super::*;
use serde_json::json;

#[allow(dead_code)]
mod bench {
    use crate as retinue_render;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/benchmarks/support.rs"
    ));
}

fn content(side: u32, decorated: bool, dot: u32) -> Recipe {
    let fill = |bounds, radius, color| Command::Fill {
        bounds,
        radius,
        color: Color::rgb(color),
    };
    let mut commands = vec![fill(
        Bounds::new(0., 0., side as f32, side as f32),
        0.,
        0x19283d,
    )];
    if decorated {
        for y in (0..side).step_by(32) {
            for x in (0..side).step_by(32) {
                commands.push(fill(
                    Bounds::new(x as f32, y as f32, 24., 24.),
                    5.,
                    0x385977,
                ));
            }
        }
    }
    commands.push(fill(Bounds::new(7., 7., 3., 3.), 0., dot));
    Recipe::item(&PaintProgram::new(1, (side as f32, side as f32), 1., Arc::new(commands)).unwrap())
}

#[test]
#[ignore = "partial layer measurement; use make bench BENCH_SUITE=layer-repair"]
fn layer_repair_benchmark() {
    let threads = bench::render_threads();
    let mut cases = Vec::new();
    for side in [128, 512, 1024] {
        for (scene, decorated) in [("flat", false), ("decorated", true)] {
            let mut init = Preparation::default();
            let id = init.add(
                content(side, decorated, 0xff0000),
                None,
                vec![],
                threads.get(),
            );
            let old = init
                .execute(&mut PaintStats::default())
                .result(id)
                .unwrap()
                .clone();
            let recipe = content(side, decorated, 0x00ff00);
            let mut reference = None;
            for (method, source) in [("replay", None), ("seed", Some(old.clone()))] {
                let name = format!("{scene}-{method}");
                let mut plan = Preparation::default();
                let id = plan.add(recipe.clone(), source, vec![], threads.get());
                let task = &plan.tasks[id.0];
                let fonts = Arc::default();
                let render = || {
                    task.render_with_threads(&[] as &[Outcome], threads, &fonts)
                        .unwrap()
                        .image
                };
                let pixels = render().to_rgba().unwrap();
                if let Some(expected) = &reference {
                    assert!(expected == &pixels);
                } else {
                    reference = Some(pixels);
                }
                if !bench::selected(&name) {
                    continue;
                }
                let mut case = bench::measure(
                    &name,
                    (side, side),
                    1.0,
                    || (),
                    |_, _| {
                        std::hint::black_box(render());
                    },
                    |_| json!({"seed_pixels": if task.repair.copy {side*side} else {0}, "repair_pixels":task.repair.regions.iter().copied().map(area).sum::<usize>(), "previous_lease_bytes": old.image.dimensions().0 as u64 * side as u64 * 4}),
                );
                case["actual_threads"] = json!(threads.get());
                cases.push(case);
            }
        }
    }
    bench::report_with_scope(
        "layer-repair",
        cases,
        "Stable layer with a single 3x3 colour edit; old image remains leased for both policies. Measures native allocation, seed/replay and completion, not adoption cost or compositor time.",
    );
}
