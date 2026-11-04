use super::{Policy, runner, scenarios};
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "opt-in whole-trace optimizer quality; no timing correctness gate"]
fn optimizer_quality_benchmark() {
    assert!(!cfg!(debug_assertions), "benchmark requires release build");
    let output = PathBuf::from(
        std::env::var_os("RETINUE_BENCH_OUTPUT")
            .expect("use make bench BENCH_SUITE=render-quality"),
    );
    let samples: usize = std::env::var("RETINUE_BENCH_SAMPLES")
        .unwrap_or_else(|_| "10".into())
        .parse()
        .unwrap();
    let threads: u32 = std::env::var("RETINUE_BENCH_THREADS")
        .unwrap_or_else(|_| "4".into())
        .parse()
        .unwrap();
    assert!((10..=5000).contains(&samples));
    assert!((1..=8).contains(&threads));
    let selection = std::env::var("RETINUE_BENCH_CASES").unwrap_or_default();
    let policies = Policy::candidates();
    let assets = scenarios::Assets::new();
    let mut rows = Vec::new();
    for case in scenarios::cases()
        .into_iter()
        .filter(|c| selection.is_empty() || selection.split(',').any(|name| name == c.name))
    {
        let steps = case.steps();
        let reference = runner::reference(&assets, case, &steps);
        let diagnostics: Vec<_> = policies
            .iter()
            .map(|&policy| {
                runner::check(&assets, case, &steps, policy, threads, &reference)
                    .unwrap_or_else(|e| panic!("{} {}: {e:?}", case.name, policy.name()))
                    .decisions
            })
            .collect();
        drop(reference);
        let mut measurements: Vec<Vec<runner::Trace>> =
            (0..policies.len()).map(|_| Vec::new()).collect();
        for repetition in 0..samples {
            for position in 0..policies.len() {
                let order = if repetition % 2 == 0 {
                    position
                } else {
                    policies.len() - 1 - position
                };
                let index = (order + repetition / 2) % policies.len();
                measurements[index].push(runner::run(
                    &assets,
                    case,
                    &steps,
                    policies[index],
                    threads,
                    false,
                    |_, _| {},
                ));
            }
        }
        for ((policy, samples), decisions) in policies.iter().zip(measurements).zip(diagnostics) {
            let actual_threads = samples[0].frames[0].actual_threads;
            assert!(
                samples
                    .iter()
                    .flat_map(|s| &s.frames)
                    .all(|f| f.actual_threads == actual_threads)
            );
            rows.push(json!({
                "case": case.name, "size": case.size, "scale": case.scale,
                "scenario": case, "candidate": policy.name(), "policy": policy,
                "actual_threads": actual_threads,
                "samples": samples, "decisions": decisions,
                "pixel_oracle": {"reference": Policy::REFERENCE.name(), "alpha_tolerance": 0, "rgb_tolerance": runner::tolerance(case, *policy)}
            }));
        }
    }
    assert!(!rows.is_empty(), "no matching scenarios");
    fs::create_dir_all(&output).unwrap();
    let report = json!({
        "schema": 1, "selected": Policy::DEFAULT.name(), "requested_threads": threads,
        "alternatives": policies.iter().map(|p| p.name()).collect::<Vec<_>>(),
        "scope": "Complete cold-renderer traces with warm native pipelines. Includes source program construction, frame updates, planning, raster preparation and native completion. Excludes SDK layout, host presentation, diagnostics and correctness checks. Fixed policy alternatives, not every possible per-frame schedule; all timings are advisory.",
        "cases": rows,
    });
    fs::write(
        output.join("quality.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
