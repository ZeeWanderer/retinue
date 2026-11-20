use std::num::NonZeroU32;

pub fn threads() -> NonZeroU32 {
    std::env::var("GUI_BENCH_THREADS")
        .ok()
        .map(|value| {
            let count: u32 = value.parse().expect("GUI_BENCH_THREADS must be an integer");
            assert!((1..=8).contains(&count));
            NonZeroU32::new(count).unwrap()
        })
        .unwrap_or_else(retinue_ui::desktop::render_threads)
}

pub fn scales(default: &[f64]) -> Vec<f64> {
    match std::env::var("GUI_BENCH_SCALES") {
        Ok(value) => parse_scales(&value).expect("invalid GUI_BENCH_SCALES"),
        Err(std::env::VarError::NotPresent) => default.to_vec(),
        Err(error) => panic!("invalid GUI_BENCH_SCALES: {error}"),
    }
}

fn parse_scales(value: &str) -> Result<Vec<f64>, String> {
    let mut scales = Vec::new();
    for token in value.split(',').map(str::trim) {
        let scale = token
            .parse::<f64>()
            .map_err(|_| format!("expected a scale ratio, got {token:?}"))?;
        if !(0.75..=2.0).contains(&scale) {
            return Err(format!("scale must be between 0.75 and 2.0: {token}"));
        }
        let scale = (scale * 100.0).round() / 100.0;
        if scales.contains(&scale) {
            return Err(format!("duplicate rendered scale: {token}"));
        }
        scales.push(scale);
    }
    Ok(scales)
}

#[test]
fn scale_list_preserves_order_and_report_precision() {
    assert_eq!(
        parse_scales("1.2, 0.9, 1, 1.5").unwrap(),
        [1.2, 0.9, 1.0, 1.5]
    );
    assert_eq!(parse_scales("1.234,1.236").unwrap(), [1.23, 1.24]);
    assert_eq!(parse_scales("0.75,2").unwrap(), [0.75, 2.0]);
}

#[test]
fn scale_list_rejects_invalid_or_duplicate_scales() {
    for value in [
        "",
        "1,",
        "90%",
        "0",
        "-1",
        "NaN",
        "inf",
        "1e100",
        "1e-100",
        "90",
        "0.74",
        "2.01",
        "1,1.0",
        "1,1.00000001",
        "1,1.004",
    ] {
        assert!(parse_scales(value).is_err(), "{value}");
    }
}

#[test]
#[ignore = "owner-side input/resource scheduling experiment; no presentation timing"]
fn resource_dispatch_benchmark() {
    use super::resources;
    use crate::{
        data::storage::Snapshot,
        presentation::shell::{Page, Shell},
    };
    use serde_json::json;
    use std::{hint::black_box, time::Instant};

    let output = std::env::var("GUI_BENCH_OUTPUT").expect("set GUI_BENCH_OUTPUT");
    let mut reports = Vec::new();
    for scale in scales(&[1.0, 1.2]) {
        for events_per_frame in [1, 8, 32] {
            for per_event in [true, false] {
                let mut shell = Shell::new(Some(Snapshot::fixture()), scale as f32).unwrap();
                shell.resize((2560, 1440), 1.0).unwrap();
                shell.set_page(Page::Foundry);
                shell.foundry.update(crate::data::foundry::fixture());
                shell.prepare_paint();
                let viewport = shell.foundry.capture_bounds("foundry.grid").unwrap();
                let mut samples = Vec::new();
                for frame in 0..160 {
                    let start = Instant::now();
                    let mut request_ns = 0;
                    for event in 0..events_per_frame {
                        let x = 30 + ((frame * events_per_frame + event) % 400);
                        let y = viewport.y + 30;
                        black_box(shell.foundry.cursor());
                        black_box(shell.foundry.pointer_move((x as f64, y as f64)));
                        black_box(shell.foundry.cursor());
                        if per_event {
                            let before = Instant::now();
                            black_box(resources::requests(&shell));
                            request_ns += before.elapsed().as_nanos();
                        }
                    }
                    shell.prepare_paint();
                    if !per_event {
                        let before = Instant::now();
                        black_box(resources::requests(&shell));
                        request_ns += before.elapsed().as_nanos();
                    }
                    let total_ns = start.elapsed().as_nanos();
                    if frame >= 20 {
                        samples.push(json!({"total_ns":total_ns, "request_ns":request_ns}));
                    }
                }
                reports.push(json!({"scale":scale, "events_per_frame":events_per_frame,
                    "policy":if per_event { "per-event" } else { "per-frame" },
                    "samples":samples}));
            }
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
}
