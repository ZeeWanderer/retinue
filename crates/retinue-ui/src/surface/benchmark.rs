//! Opt-in mapped-surface measurements, excluded from normal companion builds.
use super::Surface;
use rustix::time::{ClockId, clock_gettime};
use serde::Serialize;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::GlobalList, protocol::wl_surface,
};
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};

const MAX_FRAMES: usize = 20_000;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Polling,
    Deadlines,
}

impl Policy {
    pub fn wakes(self) -> bool {
        matches!(self, Self::Deadlines)
    }

    pub(super) fn timeout(self, mapped: bool, deadline: Option<Duration>) -> Option<Duration> {
        match self {
            Self::Polling => Some(Duration::from_millis(if mapped { 16 } else { 100 })),
            Self::Deadlines => deadline,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Frame {
    pub revision: Option<u64>,
    pub ready_ns: Option<u64>,
    pub paint_start_ns: u64,
    pub paint_end_ns: u64,
    pub submitted_ns: u64,
    pub submit_clock_ns: Option<u64>,
    pub presented_ns: Option<u64>,
    pub refresh_ns: Option<u32>,
    pub flags: Option<u32>,
    pub discarded: bool,
}

impl Frame {
    pub fn commit_to_present_ns(&self) -> Option<u64> {
        self.presented_ns?.checked_sub(self.submit_clock_ns?)
    }

    pub fn ready_to_present_ns(&self) -> Option<u64> {
        self.submitted_ns
            .checked_sub(self.ready_ns?)?
            .checked_add(self.commit_to_present_ns()?)
    }
}

#[derive(Clone, Serialize)]
pub struct Report {
    pub clock_id: Option<u32>,
    pub dispatches: u64,
    pub coalesced_updates: u64,
    pub overflow: u64,
    pub measurement: Option<Period>,
    pub frames: Vec<Frame>,
}

#[derive(Clone, Serialize)]
pub struct Period {
    pub elapsed_ns: u64,
    pub process_cpu_ns: u64,
    pub caller_cpu_ns: u64,
    pub dispatches: u64,
}

struct Counters {
    at: Instant,
    process_cpu: u64,
    caller_cpu: u64,
    dispatches: u64,
}

struct State {
    start: Instant,
    clock: Option<ClockId>,
    pending: Option<(u64, Instant)>,
    measurement: Option<Counters>,
    report: Report,
}

#[derive(Clone)]
pub struct Capture(Rc<RefCell<State>>);

impl Default for Capture {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(State {
            start: Instant::now(),
            clock: None,
            pending: None,
            measurement: None,
            report: Report {
                clock_id: None,
                dispatches: 0,
                coalesced_updates: 0,
                overflow: 0,
                measurement: None,
                frames: Vec::with_capacity(MAX_FRAMES),
            },
        })))
    }
}

impl Capture {
    pub fn begin_measurement(&self) {
        let mut state = self.0.borrow_mut();
        state.measurement = Some(Counters {
            at: Instant::now(),
            process_cpu: clock_ns(ClockId::ProcessCPUTime),
            caller_cpu: clock_ns(ClockId::ThreadCPUTime),
            dispatches: state.report.dispatches,
        });
    }

    pub fn mark(&self, revision: u64, ready: Instant) {
        let mut state = self.0.borrow_mut();
        if state.pending.replace((revision, ready)).is_some() {
            state.report.coalesced_updates += 1;
        }
    }

    pub fn report(&self) -> Report {
        let state = self.0.borrow();
        let measurement = state.measurement.as_ref().map(|start| Period {
            elapsed_ns: start.at.elapsed().as_nanos() as u64,
            process_cpu_ns: clock_ns(ClockId::ProcessCPUTime) - start.process_cpu,
            caller_cpu_ns: clock_ns(ClockId::ThreadCPUTime) - start.caller_cpu,
            dispatches: state.report.dispatches - start.dispatches,
        });
        let mut report = state.report.clone();
        report.measurement = measurement;
        report
    }

    fn submitted(&self, start: Instant, end: Instant) -> Option<usize> {
        let mut state = self.0.borrow_mut();
        if state.report.frames.len() == MAX_FRAMES {
            state.report.overflow += 1;
            return None;
        }
        let pending = state.pending.take();
        let clock = state.clock.map(clock_ns);
        let origin = state.start;
        let relative =
            |instant: Instant| instant.saturating_duration_since(origin).as_nanos() as u64;
        let frame = Frame {
            revision: pending.map(|p| p.0),
            ready_ns: pending.map(|p| relative(p.1)),
            paint_start_ns: relative(start),
            paint_end_ns: relative(end),
            submitted_ns: relative(Instant::now()),
            submit_clock_ns: clock,
            presented_ns: None,
            refresh_ns: None,
            flags: None,
            discarded: false,
        };
        let id = state.report.frames.len();
        state.report.frames.push(frame);
        Some(id)
    }
}

fn clock_ns(clock: ClockId) -> u64 {
    let time = clock_gettime(clock);
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

pub(super) struct Measurement {
    pub policy: Policy,
    capture: Capture,
    presentation: wp_presentation::WpPresentation,
    pending: usize,
}

impl Measurement {
    pub fn bind(
        globals: &GlobalList,
        qh: &QueueHandle<Surface>,
        capture: Capture,
        policy: Policy,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let presentation = globals.bind(qh, 1..=2, ())?;
        Ok(Self {
            policy,
            capture,
            presentation,
            pending: 0,
        })
    }

    pub fn submit(
        &mut self,
        surface: &wl_surface::WlSurface,
        qh: &QueueHandle<Surface>,
        start: Instant,
        end: Instant,
    ) {
        if let Some(id) = self.capture.submitted(start, end) {
            self.presentation.feedback(surface, qh, id);
            self.pending += 1;
        }
    }

    pub fn dispatched(&self) {
        self.capture.0.borrow_mut().report.dispatches += 1;
    }

    pub fn pending(&self) -> bool {
        self.pending != 0
    }
}

impl Dispatch<wp_presentation::WpPresentation, ()> for Surface {
    fn event(
        state: &mut Self,
        _: &wp_presentation::WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event
            && let Some(measurement) = &state.measurement
        {
            let mut state = measurement.capture.0.borrow_mut();
            state.report.clock_id = Some(clk_id);
            state.clock = i32::try_from(clk_id)
                .ok()
                .and_then(|id| ClockId::try_from(id).ok());
        }
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, usize> for Surface {
    fn event(
        state: &mut Self,
        _: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(measurement) = &mut state.measurement else {
            return;
        };
        let mut state = measurement.capture.0.borrow_mut();
        let Some(frame) = state.report.frames.get_mut(*id) else {
            return;
        };
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                flags,
                ..
            } => {
                let seconds = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                frame.presented_ns = seconds
                    .checked_mul(1_000_000_000)
                    .and_then(|ns| ns.checked_add(u64::from(tv_nsec)));
                frame.refresh_ns = Some(refresh);
                frame.flags = Some(flags.into());
                measurement.pending -= 1;
            }
            wp_presentation_feedback::Event::Discarded => {
                frame.discarded = true;
                measurement.pending -= 1;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_restores_previous_mapped_and_hidden_intervals() {
        assert_eq!(
            Policy::Polling.timeout(true, None),
            Some(Duration::from_millis(16))
        );
        assert_eq!(
            Policy::Polling.timeout(false, None),
            Some(Duration::from_millis(100))
        );
        assert_eq!(Policy::Deadlines.timeout(true, None), None);
        assert_eq!(
            Policy::Deadlines.timeout(true, Some(Duration::ZERO)),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn capture_counts_coalescing_without_mixing_clock_domains() {
        let capture = Capture::default();
        let ready = Instant::now();
        capture.mark(1, ready);
        capture.mark(2, ready);
        capture.submitted(ready, ready + Duration::from_millis(2));
        let report = capture.report();
        assert_eq!(report.coalesced_updates, 1);
        let mut frame = report.frames[0].clone();
        assert_eq!(frame.revision, Some(2));
        assert_eq!(frame.ready_to_present_ns(), None);
        frame.submit_clock_ns = Some(100_000);
        frame.presented_ns = Some(150_000);
        assert_eq!(
            frame.ready_to_present_ns(),
            Some(frame.submitted_ns - frame.ready_ns.unwrap() + 50_000)
        );
        frame.presented_ns = Some(99_999);
        assert_eq!(frame.ready_to_present_ns(), None);
    }

    #[test]
    fn trace_storage_is_bounded() {
        let capture = Capture::default();
        let now = Instant::now();
        for _ in 0..MAX_FRAMES {
            assert!(capture.submitted(now, now).is_some());
        }
        assert_eq!(capture.submitted(now, now), None);
        assert_eq!(capture.report().overflow, 1);
    }

    #[test]
    fn measured_period_excludes_warmup_dispatches() {
        let capture = Capture::default();
        capture.0.borrow_mut().report.dispatches = 100;
        capture.begin_measurement();
        capture.0.borrow_mut().report.dispatches += 3;
        assert_eq!(capture.report().measurement.unwrap().dispatches, 3);
        assert_eq!(capture.report().dispatches, 103);
    }
}
