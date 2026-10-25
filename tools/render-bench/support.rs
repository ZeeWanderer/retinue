use rustix::time::{ClockId, clock_gettime};
use serde_json::{Value, json};
use std::{
    fs,
    hint::black_box,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

pub fn selected(name: &str) -> bool {
    matches_case(
        name,
        &std::env::var("RETINUE_BENCH_CASES").unwrap_or_default(),
    )
}

fn matches_case(name: &str, selection: &str) -> bool {
    selection.trim().is_empty() || selection.split(',').any(|case| case.trim() == name)
}

fn cadence_hz() -> u32 {
    let hz: u32 = std::env::var("RETINUE_BENCH_HZ")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .expect("BENCH_HZ must be an integer");
    assert!(hz <= 1000, "BENCH_HZ must be 0..1000");
    hz
}

fn cpu_percent(cpu_ns: u64, elapsed_ns: u64) -> Option<f64> {
    (elapsed_ns > 0).then(|| cpu_ns as f64 / elapsed_ns as f64 * 100.0)
}

pub fn render_threads() -> std::num::NonZeroU32 {
    parse_threads(&std::env::var("RETINUE_BENCH_THREADS").unwrap_or_else(|_| "1".into()))
}

fn parse_threads(value: &str) -> std::num::NonZeroU32 {
    let threads: u32 = value.parse().expect("BENCH_THREADS must be an integer");
    assert!(
        (1..=8).contains(&threads),
        "benchmark supports 1 through 8 total threads"
    );
    std::num::NonZeroU32::new(threads).unwrap()
}

pub fn measure<T>(
    name: &str,
    size: (u32, u32),
    scale: f32,
    prepare: impl FnOnce() -> T,
    mut paint: impl FnMut(&mut T, usize),
    resources: impl FnOnce(&T) -> Value,
) -> Value {
    let count: usize = std::env::var("RETINUE_BENCH_SAMPLES")
        .unwrap_or_else(|_| "60".into())
        .parse()
        .expect("RETINUE_BENCH_SAMPLES must be an integer");
    assert!(
        (10..=5000).contains(&count),
        "sample count must be 10..5000"
    );
    let hz = cadence_hz();
    let interval = (hz > 0).then(|| Duration::from_secs_f64(1.0 / f64::from(hz)));
    let available_cpus = std::thread::available_parallelism().ok().map(|n| n.get());
    let before = memory();
    let started = Instant::now();
    let mut context = prepare();
    let prepare_ns = started.elapsed().as_nanos() as u64;
    let started = Instant::now();
    paint(black_box(&mut context), 0);
    let first_paint_ns = started.elapsed().as_nanos() as u64;
    for index in 1..=5 {
        paint(black_box(&mut context), index);
    }
    let process_before = cpu_ns(ClockId::ProcessCPUTime);
    let cpu_before = cpu_ns(ClockId::ThreadCPUTime);
    let started = Instant::now();
    let samples: Vec<_> = (0..count)
        .map(|index| {
            let started = Instant::now();
            paint(black_box(&mut context), index + 6);
            let elapsed = started.elapsed();
            if let Some(interval) = interval {
                std::thread::sleep(interval.saturating_sub(elapsed));
            }
            elapsed.as_nanos() as u64
        })
        .collect();
    let elapsed_ns = started.elapsed().as_nanos() as u64;
    let thread_cpu_ns = cpu_ns(ClockId::ThreadCPUTime).saturating_sub(cpu_before);
    let process_cpu_ns = cpu_ns(ClockId::ProcessCPUTime).saturating_sub(process_before);
    let process_cpu_percent = cpu_percent(process_cpu_ns, elapsed_ns);
    let native_images = retinue_ui::painter::image_memory();
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    let percentile = |percent: usize| sorted[(sorted.len() * percent).div_ceil(100) - 1];
    json!({
        "case": name, "size": [size.0, size.1], "scale": scale,
        "prepare_ns": prepare_ns, "first_paint_ns": first_paint_ns,
        "samples": count, "warmup_frames": 5,
        "warm": {
            "p50_ns": percentile(50), "p95_ns": percentile(95),
            "p99_ns": percentile(99), "max_ns": sorted.last(),
            "elapsed_ns": elapsed_ns, "thread_cpu_ns": thread_cpu_ns,
            "process_cpu_ns": process_cpu_ns,
            "process_cpu_percent": process_cpu_percent,
            "process_cpu_percent_available": process_cpu_percent.zip(available_cpus)
                .map(|(percent, count)| percent / count as f64),
            "available_logical_cpus": available_cpus,
            "frames_per_second": count as f64 / (elapsed_ns as f64 / 1e9),
            "over_budget_frames": interval.map(|interval| {
                samples.iter().filter(|&&ns| ns > interval.as_nanos() as u64).count()
            }),
            "frame_ns": samples
        },
        "process_memory_before": before, "process_memory_after": memory(),
        "known_resources": resources(&context),
        "native_images": {
            "images": native_images.images, "pixel_bytes": native_images.pixel_bytes
        }
    })
}

pub fn report(game: &str, cases: Vec<Value>) {
    report_with_scope(
        game,
        cases,
        "Headless rendering including native completion; excludes Wayland/KWin, game, network and frame scheduling. Process CPU includes workers. RSS/HWM include the test process; HWM is cumulative, not per case.",
    );
}

pub fn report_with_scope(game: &str, cases: Vec<Value>, scope: &str) {
    assert!(!cfg!(debug_assertions), "run the benchmark with --release");
    let output =
        PathBuf::from(std::env::var_os("RETINUE_BENCH_OUTPUT").expect("use make bench-overlays"));
    fs::create_dir_all(&output).unwrap();
    let path = output.join(format!("{game}.json"));
    let command = |program: &str, arguments: &[&str]| {
        Command::new(program)
            .args(arguments)
            .output()
            .ok()
            .filter(|result| result.status.success())
            .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
    };
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "model name").then(|| value.trim().to_owned())
        })
    });
    let report = json!({
        "schema": 1, "game": game, "version": env!("CARGO_PKG_VERSION"),
        "profile": "release", "git": command("git", &["rev-parse", "HEAD"]),
        "dirty": command("git", &["status", "--porcelain"]).map(|value| !value.is_empty()),
        "rustc": command("rustc", &["-Vv"]), "cpu": cpu,
        "kernel": fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|s| s.trim().to_owned()),
        "render_mode": "queued", "requested_threads": render_threads().get(),
        "cadence_hz": cadence_hz(),
        "scope": scope,
        "cases": cases
    });
    fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    eprintln!("render benchmark: {}", path.display());
}

#[test]
#[should_panic(expected = "benchmark supports 1 through 8")]
fn benchmark_rejects_synchronous_rendering() {
    parse_threads("0");
}

#[test]
fn benchmark_cpu_percent_includes_all_workers() {
    assert_eq!(cpu_percent(2_500, 1_000), Some(250.0));
    assert_eq!(cpu_percent(5, 1_000), Some(0.5));
    assert_eq!(cpu_percent(0, 0), None);
}

#[test]
fn benchmark_case_selection_is_exact() {
    assert!(matches_case("hover", ""));
    assert!(matches_case("hover", "warm-repaint, hover"));
    assert!(!matches_case("hover", "warm-repaint,listing-scroll"));
    assert!(!matches_case("hover", "hover-other"));
}

fn cpu_ns(clock: ClockId) -> u64 {
    let time = clock_gettime(clock);
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

pub fn memory() -> Value {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = |key: &str| -> Option<u64> {
        status.lines().find_map(|line| {
            line.strip_prefix(key)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
                .map(|kib| kib * 1024)
        })
    };
    json!({"rss_bytes": kib("VmRSS:"), "high_water_bytes": kib("VmHWM:")})
}
