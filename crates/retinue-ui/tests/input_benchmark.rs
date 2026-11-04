use retinue_ui::{
    Bounds,
    input::{
        Activation, Button, Event, InputOptions, Interaction, PointerId, PointerPhase, Router,
        Target,
    },
    runtime::ViewId,
};
use std::{hint::black_box, time::Instant};

const EVENTS: usize = 4096;
const BATCHES: usize = 16;

fn targets(count: usize, observed: bool) -> Vec<Target> {
    (0..count)
        .map(|i| {
            let bounds = Bounds::new((i % 8) as f32 * 100.0, (i / 8) as f32 * 40.0, 96.0, 36.0);
            Target {
                id: ViewId(i as u64 + 1),
                parent: None,
                bounds,
                clip: bounds,
                enabled: true,
                input: InputOptions {
                    interaction: observed.then(Interaction::default),
                    activation: Activation::Release,
                    ..Default::default()
                },
            }
        })
        .collect()
}

fn pointer(phase: PointerPhase, position: (f32, f32)) -> Event {
    Event::Pointer {
        id: PointerId::MOUSE,
        phase,
        position,
        modifiers: Default::default(),
    }
}

fn position(index: usize) -> (f32, f32) {
    (
        (index % 8) as f32 * 100.0 + 20.0,
        (index / 8) as f32 * 40.0 + 20.0,
    )
}

#[test]
#[ignore = "headless input routing experiment; needs INPUT_BENCH_OUTPUT"]
fn input_dispatch_benchmark() {
    let output = std::env::var_os("INPUT_BENCH_OUTPUT").expect("set INPUT_BENCH_OUTPUT");
    let mut rows = Vec::new();
    for count in [64, 512, 2048] {
        for observed in [false, true] {
            for case in [
                "stationary",
                "within-target",
                "cross-target",
                "capture",
                "reflow",
            ] {
                let source = targets(count, observed);
                let mut router = Router::default();
                router.update(source.clone());
                if case == "capture" {
                    router.dispatch(&pointer(PointerPhase::Down(Button::Primary), position(0)));
                }
                let mut samples = Vec::new();
                for batch in 0..=BATCHES {
                    let start = Instant::now();
                    for event in 0..EVENTS {
                        let index = match case {
                            "stationary" | "within-target" => count / 2,
                            _ => event % count,
                        };
                        let mut p = position(index);
                        if case == "within-target" {
                            p.0 += (event % 2) as f32;
                        }
                        if case == "reflow" && event % 64 == 0 {
                            let mut next = source.clone();
                            for target in &mut next {
                                target.bounds.x += (event % 128 == 0) as u8 as f32;
                            }
                            router.update(next);
                        }
                        black_box(router.dispatch(&pointer(PointerPhase::Move, p)));
                    }
                    let ns = start.elapsed().as_nanos() as f64 / EVENTS as f64;
                    if batch != 0 {
                        samples.push(ns);
                    }
                }
                let mut sorted = samples.clone();
                sorted.sort_by(f64::total_cmp);
                rows.push(serde_json::json!({
                    "case": case,
                    "targets": count,
                    "observed": observed,
                    "ns_per_event": samples,
                    "median_ns": (sorted[BATCHES / 2 - 1] + sorted[BATCHES / 2]) / 2.0,
                }));
            }
        }
    }
    let report = serde_json::json!({"workload": 1, "events_per_batch": EVENTS, "rows": rows});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
