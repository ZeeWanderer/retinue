use super::*;
use crate::render_bench;
use retinue_ui::{
    damage::Damage,
    schedule::{self, Deadlines},
    surface::{
        self, Application, Input,
        benchmark::{Capture, Policy},
    },
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const WARMUP: Duration = Duration::from_millis(600);
const TAIL: Duration = Duration::from_millis(200);
const ANIMATION: Duration = Duration::from_millis(70);

struct Signal {
    revision: u64,
    ready: Instant,
}

struct StopOnDrop(Arc<AtomicBool>, schedule::Ping);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
        self.1.ping();
    }
}

struct Panel {
    view: View,
    model: Model,
    interaction: Interaction,
    capture: Capture,
    scenario: &'static str,
    scale: f32,
    started: Option<Instant>,
    first_paint: Option<mpsc::SyncSender<Instant>>,
    updates: mpsc::Receiver<Signal>,
    next_change: Instant,
    next_control: Instant,
    revision: u64,
    measuring: bool,
    size: Rc<RefCell<(u32, u32)>>,
}

use std::{cell::RefCell, rc::Rc};

impl Application for Panel {
    fn visible(&self) -> bool {
        true
    }
    fn keyboard_interactive(&self) -> bool {
        false
    }
    fn pointer_regions(&self) -> Vec<Rect> {
        Vec::new()
    }
    fn input(&mut self, _: Input) -> Damage {
        Damage::None
    }

    fn deadlines(&self, now: Instant) -> Deadlines {
        let mut deadlines = Deadlines::default();
        deadlines.control(Some(self.next_control));
        if let Some(started) = self.started
            && (now < started + WARMUP || self.scenario == "loading")
        {
            deadlines.visual(Some(self.next_change));
        }
        deadlines
    }

    fn tick(&mut self, drawable: bool) -> Damage {
        let now = Instant::now();
        if now >= self.next_control {
            self.next_control = now + Duration::from_millis(100);
        }
        let Some(started) = self.started else {
            return Damage::Full;
        };
        if now < started + WARMUP {
            if drawable && now >= self.next_change {
                self.next_change = now + Duration::from_millis(20);
                return Damage::Full;
            }
            return Damage::None;
        }
        if !self.measuring {
            self.measuring = true;
            self.capture.begin_measurement();
            self.next_change = started + WARMUP + ANIMATION;
        }
        let mut dirty = Damage::None;
        while let Ok(signal) = self.updates.try_recv() {
            self.capture.mark(signal.revision, signal.ready);
            self.revision = signal.revision;
            if self.scenario == "async-hover" {
                let old = self.interaction.hover;
                self.interaction.hover = (signal.revision % 2 == 1).then_some(Target::Search);
                dirty = dirty
                    .union(self.view.target_damage(old))
                    .union(self.view.target_damage(self.interaction.hover));
            } else {
                self.interaction.scroll = signal.revision as usize % 10;
                dirty = Damage::Full;
            }
        }
        if self.scenario == "loading" && drawable && now >= self.next_change {
            self.revision += 1;
            self.capture.mark(self.revision, self.next_change);
            self.next_change = now + ANIMATION;
            dirty = dirty.union(self.view.animation_damage());
        }
        dirty
    }

    fn paint(
        &mut self,
        pixels: &mut [u8],
        size: (u32, u32),
        dpr: u32,
        repair: Rect,
    ) -> Result<(), String> {
        *self.size.borrow_mut() = size;
        self.view.paint_region(
            pixels,
            Frame {
                size,
                scale: self.scale * dpr as f32,
                zoom: self.scale,
                phase: (self.revision % 20) as f32 / 20.0,
            },
            &self.model,
            &self.interaction,
            repair,
        )?;
        if let Some(send) = self.first_paint.take() {
            let start = Instant::now();
            self.started = Some(start);
            self.next_change = start;
            let _ = send.send(start);
        }
        Ok(())
    }
}

fn percentile(samples: impl Iterator<Item = u64>) -> Value {
    let mut samples: Vec<_> = samples.collect();
    samples.sort_unstable();
    if samples.is_empty() {
        return Value::Null;
    }
    let at = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
    json!({"samples": samples.len(), "p50_ns": at(50), "p95_ns": at(95),
        "p99_ns": at(99), "max_ns": samples.last()})
}

fn interval(scenario: &str, revision: u64) -> Duration {
    Duration::from_millis(if scenario == "async-hover" {
        [29, 47, 83, 31, 59][revision as usize % 5]
    } else {
        33
    })
}

fn run(scenario: &'static str, scale: f32, policy: Policy, count: u64, pass: usize) -> Value {
    let data: Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/evaluate-reference.json"
    ))
    .unwrap();
    let mut model = Model::new("Forbidden Rites".into());
    model.evaluated(
        0,
        serde_json::from_value(data["evaluation"].clone()).unwrap(),
    );
    if scenario == "async-scroll" {
        let mut results: super::super::model::Results =
            serde_json::from_value(data["results"].clone()).unwrap();
        results.listings = results.listings.iter().cloned().cycle().take(30).collect();
        model.results = Some(results);
    }
    model.loading = scenario == "loading";
    let mut view = View::new().unwrap();
    view.preview_time = Some(1791885600);
    view.render_budget = render_bench::render_threads();
    let (wake, source) = schedule::make_ping().unwrap();
    let (ready, start) = mpsc::sync_channel(1);
    let (send, updates) = mpsc::sync_channel(32);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let producer = thread::spawn(move || -> Result<u64, String> {
        let _stop = StopOnDrop(stopping.clone(), wake.clone());
        let start = start
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())?;
        let wait = |at: Instant| thread::sleep(at.saturating_duration_since(Instant::now()));
        let mut at = start + WARMUP;
        let mut emitted = 0;
        match scenario {
            "idle" => wait(at + Duration::from_secs(2)),
            "loading" => wait(at + ANIMATION * count as u32),
            _ => {
                for revision in 1..=count {
                    at += interval(scenario, revision);
                    wait(at);
                    if stopping.load(Ordering::Acquire) {
                        return Err("surface stopped before updates completed".into());
                    }
                    send.try_send(Signal {
                        revision,
                        ready: Instant::now(),
                    })
                    .map_err(|e| e.to_string())?;
                    emitted += 1;
                    if policy.wakes() {
                        wake.ping();
                    }
                }
            }
        }
        thread::sleep(TAIL);
        Ok(emitted)
    });
    let capture = Capture::default();
    let size = Rc::new(RefCell::new((0, 0)));
    let app = Panel {
        view,
        model,
        interaction: Interaction {
            position: Some((32, 72)),
            ..Interaction::default()
        },
        capture: capture.clone(),
        scenario,
        scale,
        started: None,
        first_paint: Some(ready),
        updates,
        next_change: Instant::now(),
        next_control: Instant::now(),
        revision: 0,
        measuring: false,
        size: size.clone(),
    };
    let result = surface::run_measured(
        "retinue-ui-benchmark",
        Box::new(app),
        stop.clone(),
        source,
        capture.clone(),
        policy,
    );
    stop.store(true, Ordering::Release);
    let emitted = producer.join().unwrap().unwrap();
    result.unwrap();
    let report = capture.report();
    let period = report
        .measurement
        .as_ref()
        .expect("measurement never started");
    assert_eq!(report.overflow, 0);
    assert!(
        report.clock_id.is_some(),
        "compositor supplied no presentation clock"
    );
    let frames: Vec<_> = report
        .frames
        .iter()
        .filter(|f| f.revision.is_some())
        .collect();
    let presented = frames.iter().filter(|f| f.presented_ns.is_some()).count();
    if scenario != "idle" {
        assert!(presented > 0, "no measured updates presented");
        assert!(
            frames.iter().any(|f| f.ready_to_present_ns().is_some()),
            "presentation clock cannot be correlated"
        );
    }
    let intervals = frames
        .windows(2)
        .filter_map(|pair| pair[1].presented_ns?.checked_sub(pair[0].presented_ns?));
    let paint = percentile(frames.iter().map(|f| f.paint_end_ns - f.paint_start_ns));
    let ready_to_paint = percentile(
        frames
            .iter()
            .filter_map(|f| f.paint_start_ns.checked_sub(f.ready_ns?)),
    );
    let ready_to_present = percentile(frames.iter().filter_map(|f| f.ready_to_present_ns()));
    let commit_to_present = percentile(frames.iter().filter_map(|f| f.commit_to_present_ns()));
    eprintln!(
        "scheduling: {scenario} {scale} {policy:?} pass{pass}: presented={presented}, ready->present p95={}ms",
        ready_to_present["p95_ns"].as_u64().unwrap_or(0) as f64 / 1e6
    );
    json!({"case": scenario, "scale": scale, "size": *size.borrow(), "policy": policy, "pass": pass,
        "warmup_ms": WARMUP.as_millis(), "source_updates": emitted,
        "measured_frames": frames.len(), "presented_frames": presented,
        "discarded_frames": frames.iter().filter(|f| f.discarded).count(),
        "unresolved_feedback": frames.iter().filter(|f| !f.discarded && f.presented_ns.is_none()).count(),
        "paint": paint, "ready_to_paint": ready_to_paint, "ready_to_present": ready_to_present,
        "commit_to_present": commit_to_present, "presentation_intervals": percentile(intervals),
        "elapsed_ns": period.elapsed_ns, "process_cpu_ns": period.process_cpu_ns, "caller_cpu_ns": period.caller_cpu_ns,
        "process_cpu_percent": 100.0 * period.process_cpu_ns as f64 / period.elapsed_ns as f64,
        "caller_cpu_percent": 100.0 * period.caller_cpu_ns as f64 / period.elapsed_ns as f64,
        "memory": render_bench::memory(), "trace": report})
}

#[test]
#[ignore = "make bench-overlay-scheduling (visible, click-through fixture panel)"]
fn overlay_scheduling_benchmark() {
    assert!(!cfg!(debug_assertions), "use release benchmark build");
    let count = std::env::var("RETINUE_BENCH_SAMPLES")
        .unwrap_or_else(|_| "60".into())
        .parse::<u64>()
        .unwrap();
    assert!((10..=1000).contains(&count));
    let scales = std::env::var("RETINUE_BENCH_SCALES").unwrap_or_else(|_| "1,1.25,1.5".into());
    let mut cases = Vec::new();
    for scale in scales.split(',').map(|s| s.parse::<f32>().unwrap()) {
        assert!((0.75..=2.0).contains(&scale));
        for scenario in ["idle", "async-hover", "async-scroll", "loading"] {
            if !render_bench::selected(scenario) {
                continue;
            }
            for (pass, policy) in [
                Policy::Polling,
                Policy::Deadlines,
                Policy::Deadlines,
                Policy::Polling,
            ]
            .into_iter()
            .enumerate()
            {
                cases.push(run(scenario, scale, policy, count, pass));
                render_bench::report_with_scope(
                    "poe2-scheduling",
                    cases.clone(),
                    "Real Evaluate rendering and Wayland surface. ABBA restores only the former16ms mapped/100ms hidden polling and lack of explicit wakes; other code is identical. Scripted worker updates and70ms visual deadlines, not physical input or full companion logic. No game input, network, or game-impact measurement. CPU includes producer, rendering and presentation drain; CPU and latency summaries exclude warmup. 100% CPU means one logical CPU.",
                );
            }
        }
    }
    assert!(!cases.is_empty(), "no scheduling cases selected");
}

#[test]
fn percentiles_exclude_missing_samples_instead_of_treating_them_as_zero() {
    assert_eq!(percentile(std::iter::empty()), Value::Null);
    assert_eq!(percentile([1, 2, 3, 10].into_iter())["p95_ns"], 10);
}
