use std::fs::File;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::game_observer::gep;
use serde::Serialize;

use super::mailbox::{self, Mailbox, SendError};

const MAX_SAMPLES: usize = 16;
const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const POLL_INTERVAL: Duration = Duration::from_millis(7);
const ACCOUNT_INTERVAL: Duration = Duration::from_secs(1);
const INVENTORY_MARKER: &[u8] = b"LastInventorySync";

#[derive(Default)]
pub struct Options {
    pub inventory_only: bool,
    pub skip_initial: bool,
    pub account_seed: bool,
    pub deadline: Option<Instant>,
}

pub enum Content {
    Payload {
        source: &'static str,
        bytes: Vec<u8>,
    },
    Account(u32),
    AccountState(AccountState),
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccountState {
    #[default]
    NotObserved,
    Available,
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AccountReport {
    attempts: u64,
    failures: u64,
    #[serde(flatten)]
    state: AccountState,
}

impl AccountReport {
    fn observe(&mut self, result: &std::io::Result<u32>) -> bool {
        self.attempts += 1;
        let next = match result {
            Ok(_) => AccountState::Available,
            Err(error) => {
                self.failures += 1;
                AccountState::Unavailable {
                    reason: error.to_string(),
                }
            }
        };
        let changed = std::mem::discriminant(&self.state) != std::mem::discriminant(&next);
        self.state = next;
        changed
    }
}

pub struct Sample {
    pub sequence: u64,
    pub captured: Instant,
    pub collected_at: u128,
    pub content: Content,
}

impl Sample {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + match &self.content {
                Content::Payload { bytes, .. } => bytes.capacity(),
                Content::Account(_) => 0,
                Content::AccountState(AccountState::Unavailable { reason }) => reason.capacity(),
                Content::AccountState(_) => 0,
            }
    }
}

#[derive(Default)]
pub struct Timing {
    count: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

impl Timing {
    pub fn record(&self, duration: Duration) {
        let micros = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_us.fetch_add(micros, Ordering::Relaxed);
        self.max_us.fetch_max(micros, Ordering::Relaxed);
    }

    fn snapshot(&self) -> TimingReport {
        let count = self.count.load(Ordering::Relaxed);
        TimingReport {
            count,
            mean_us: self.total_us.load(Ordering::Relaxed) / count.max(1),
            max_us: self.max_us.load(Ordering::Relaxed),
        }
    }
}

#[derive(Default)]
pub struct Metrics {
    work: Timing,
    interval: Timing,
    pub decode: Timing,
    pub queue_delay: Timing,
    candidates: AtomicU64,
    candidate_bytes: AtomicU64,
    retained_buffer_rechecks: AtomicU64,
    pub decoded_sequence: AtomicU64,
    account: Mutex<AccountReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub poll: TimingReport,
    interval: TimingReport,
    decode: TimingReport,
    queue_delay: TimingReport,
    candidates: u64,
    candidate_bytes: u64,
    retained_buffer_rechecks: u64,
    decoded_sequence: u64,
    pub queue: mailbox::Stats,
    pub account: AccountReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct TimingReport {
    pub count: u64,
    mean_us: u64,
    pub max_us: u64,
}

pub struct Sampler {
    pub queue: Arc<Mailbox<Sample>>,
    pub metrics: Arc<Metrics>,
    pub stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Sampler {
    pub fn start(
        mem: File,
        sources: gep::Sources,
        options: Options,
        active: impl Fn() -> bool + Send + 'static,
    ) -> Result<Self, String> {
        let queue = Arc::new(Mailbox::new(MAX_SAMPLES, MAX_BYTES));
        let metrics = Arc::new(Metrics::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let output = queue.clone();
        let counters = metrics.clone();
        let stop = stopping.clone();
        let worker = thread::Builder::new()
            .name("wf-gep-sampler".into())
            .spawn(move || {
                let _close = CloseQueue(output.clone());
                sample(mem, sources, options, active, &output, &counters, &stop);
            })
            .map_err(|error| format!("could not start inventory sampler: {error}"))?;
        Ok(Self {
            queue,
            metrics,
            stopping,
            worker: Some(worker),
        })
    }

    pub fn report(&self) -> Report {
        self.metrics.report(self.queue.stats())
    }

    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker
                .join()
                .map_err(|_| "GEP sampler panicked".to_owned())?;
        }
        Ok(())
    }
}

impl Metrics {
    fn report(&self, queue: mailbox::Stats) -> Report {
        Report {
            account: self
                .account
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone(),
            poll: self.work.snapshot(),
            interval: self.interval.snapshot(),
            decode: self.decode.snapshot(),
            queue_delay: self.queue_delay.snapshot(),
            candidates: self.candidates.load(Ordering::Relaxed),
            candidate_bytes: self.candidate_bytes.load(Ordering::Relaxed),
            retained_buffer_rechecks: self.retained_buffer_rechecks.load(Ordering::Relaxed),
            decoded_sequence: self.decoded_sequence.load(Ordering::Relaxed),
            queue,
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("{error}");
        }
    }
}

struct CloseQueue(Arc<Mailbox<Sample>>);

impl Drop for CloseQueue {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn sample(
    mem: File,
    sources: gep::Sources,
    options: Options,
    active: impl Fn() -> bool,
    output: &Mailbox<Sample>,
    metrics: &Metrics,
    stopping: &AtomicBool,
) {
    let mut state = gep::PollState::default();
    if options.skip_initial {
        let _ = sources.persistent_payloads(&mem, &mut state);
    }
    let mut account_seed = None;
    let mut account_status_pending = true;
    let mut next_account = Instant::now();
    let mut previous_poll = None;
    let mut sequence = 0;
    let mut retry_retained = false;
    while !stopping.load(Ordering::Acquire)
        && active()
        && options
            .deadline
            .is_none_or(|deadline| Instant::now() < deadline)
    {
        let started = Instant::now();
        if let Some(previous) = previous_poll.replace(started) {
            metrics.interval.record(started.duration_since(previous));
        }
        if retry_retained {
            let queued = output.stats();
            if queued.items < MAX_SAMPLES / 2 && queued.bytes < MAX_BYTES / 2 {
                state.invalidate();
                account_seed = None;
                account_status_pending = true;
                next_account = started;
                retry_retained = false;
                metrics
                    .retained_buffer_rechecks
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        let mut emit = |content| {
            sequence += 1;
            let sample = Sample {
                sequence,
                captured: Instant::now(),
                collected_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
                content,
            };
            let bytes = sample.bytes();
            match output.send(sample, bytes) {
                Ok(dropped) => {
                    retry_retained |= dropped > 0;
                    true
                }
                Err(SendError::Oversized) => {
                    retry_retained = true;
                    true
                }
                Err(SendError::Closed) => false,
            }
        };
        if options.account_seed && started >= next_account {
            let result = sources.account_seed(&mem);
            let status = {
                let mut report = metrics
                    .account
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                let changed = report.observe(&result);
                (changed || account_status_pending).then(|| report.state.clone())
            };
            if let Some(status) = status {
                if !emit(Content::AccountState(status)) {
                    return;
                }
                account_status_pending = false;
            }
            if let Ok(seed) = result
                && account_seed != Some(seed)
            {
                if !emit(Content::Account(seed)) {
                    return;
                }
                account_seed = Some(seed);
            }
            next_account = Instant::now() + ACCOUNT_INTERVAL;
        }
        for (source, bytes) in sources.persistent_payloads(&mem, &mut state) {
            if options.inventory_only && memchr::memmem::find(&bytes, INVENTORY_MARKER).is_none() {
                continue;
            }
            metrics.candidates.fetch_add(1, Ordering::Relaxed);
            metrics
                .candidate_bytes
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if !emit(Content::Payload { source, bytes }) {
                return;
            }
        }
        metrics.work.record(started.elapsed());
        thread::park_timeout(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_health_reports_transitions_without_repeating_failures() {
        let mut report = AccountReport::default();
        assert!(report.observe(&Err(std::io::Error::other("missing binding"))));
        for _ in 0..20 {
            assert!(!report.observe(&Err(std::io::Error::other("another read error"))));
        }
        assert!(
            matches!(&report.state, AccountState::Unavailable { reason } if reason == "another read error")
        );
        assert!(report.observe(&Ok(123)));
        assert!(!report.observe(&Ok(456)));
        assert_eq!((report.attempts, report.failures), (23, 21));
        assert!(report.observe(&Err(std::io::Error::other("profile unloaded"))));
        assert!(!serde_json::to_string(&report).unwrap().contains("123"));
    }

    #[test]
    fn stalled_decoder_retains_order_and_acquisition_time_with_bounded_storage() {
        let queue = Mailbox::new(2, MAX_BYTES);
        let captured = Instant::now();
        for sequence in 1..=1000 {
            let sample = Sample {
                sequence,
                captured,
                collected_at: u128::from(sequence),
                content: Content::Account(sequence as u32),
            };
            assert!(queue.send(sample, size_of::<Sample>()).is_ok());
        }
        let stats = queue.stats();
        assert_eq!((stats.items, stats.dropped_items), (2, 998));
        assert_eq!(stats.bytes, 2 * size_of::<Sample>());
        for sequence in [999, 1000] {
            let sample = queue.recv_timeout(Duration::ZERO).unwrap();
            assert_eq!(sample.sequence, sequence);
            assert_eq!(sample.captured, captured);
            assert_eq!(sample.collected_at, u128::from(sequence));
        }
    }

    #[test]
    fn charges_allocation_capacity_not_only_payload_length() {
        let mut bytes = Vec::with_capacity(4096);
        bytes.extend_from_slice(b"{}");
        let capacity = bytes.capacity();
        let sample = Sample {
            sequence: 1,
            captured: Instant::now(),
            collected_at: 0,
            content: Content::Payload {
                source: "direct",
                bytes,
            },
        };
        assert_eq!(sample.bytes(), size_of::<Sample>() + capacity);
    }

    #[test]
    fn reports_work_separately_from_poll_interval_and_queue_delay() {
        let metrics = Metrics::default();
        metrics.work.record(Duration::from_micros(10));
        metrics.work.record(Duration::from_micros(30));
        metrics.interval.record(Duration::from_micros(7020));
        metrics.queue_delay.record(Duration::from_millis(12));
        metrics.decoded_sequence.store(3, Ordering::Relaxed);
        let report = serde_json::to_value(metrics.report(mailbox::Stats::default())).unwrap();
        assert_eq!(
            report["poll"],
            serde_json::json!({"count":2, "mean_us":20, "max_us":30})
        );
        assert_eq!(report["interval"]["mean_us"], 7020);
        assert_eq!(report["queue_delay"]["mean_us"], 12000);
        assert_eq!(report["decoded_sequence"], 3);
        assert_eq!(report["decode"]["mean_us"], 0);
    }
}
