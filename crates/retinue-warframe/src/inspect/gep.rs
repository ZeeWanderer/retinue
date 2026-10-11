use std::fs::File;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::game_observer::gep::{PollState, Sources};
use crate::observation::gep::{self as sampling, Content, POLL_INTERVAL, Sampler};

const INVENTORY_MARKER: &[u8] = b"LastInventorySync";

#[derive(Debug, Serialize)]
pub struct GepState {
    pub identity: crate::game_observer::ProcessIdentity,
    pub captured_at_unix_ms: u128,
    pub discovery_ms: u128,
    pub read_ms: u128,
    pub layout: GepLayout,
    pub account_seed: ProbeValue<u32>,
    pub payloads: Vec<Payload>,
}

#[derive(Debug, Serialize)]
pub struct GepLayout {
    pub manager_global: String,
    pub account: ProbeValue<serde_json::Value>,
    pub queue_table_offset: String,
    pub item_base_offset: String,
    pub body_offset: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeValue<T> {
    Available { value: T },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Serialize)]
pub struct Payload {
    pub source: &'static str,
    pub bytes: usize,
    pub sha256: String,
    pub contains_inventory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_after_ms: Option<u128>,
    pub observed_at_unix_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct GepWatch {
    pub duration_ms: u128,
    pub poll_interval_ms: u128,
    pub polls: u64,
    pub max_poll_us: u128,
    pub events: Vec<Payload>,
    pub sampling: sampling::Report,
}

#[derive(Debug, Serialize)]
pub struct GepWatchSummary {
    pub identity: crate::game_observer::ProcessIdentity,
    pub duration_ms: u128,
    pub poll_interval_ms: u128,
    pub polls: u64,
    pub max_poll_us: u128,
    pub events: usize,
    pub sampling: sampling::Report,
}

pub fn state(pid: u32) -> Result<GepState, String> {
    state_with_payloads(pid, None)
}

pub fn state_with_payloads(pid: u32, payload_dir: Option<&Path>) -> Result<GepState, String> {
    let identity = crate::game_observer::identify_process(pid)?;
    let mem = open_memory(pid)?;
    let started = Instant::now();
    let sources = Sources::discover(&identity)?;
    let discovery_ms = started.elapsed().as_millis();
    let layout = layout(&sources);
    let account_seed = match sources.account_seed(&mem) {
        Ok(value) => ProbeValue::Available { value },
        Err(error) => ProbeValue::Unavailable {
            reason: error.to_string(),
        },
    };
    let mut poll = PollState::default();
    let started = Instant::now();
    let payloads = sources
        .persistent_payloads(&mem, &mut poll)
        .into_iter()
        .map(|(source, payload)| {
            capture_payload(source, &payload, None, super::unix_time_ms(), payload_dir)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GepState {
        identity,
        captured_at_unix_ms: super::unix_time_ms(),
        discovery_ms,
        read_ms: started.elapsed().as_millis(),
        layout,
        account_seed,
        payloads,
    })
}

pub fn watch(pid: u32, duration: Duration, limit: usize) -> Result<GepWatch, String> {
    let mut events = Vec::new();
    let summary = watch_stream(pid, duration, limit, |event| {
        events.push(event.clone());
        Ok(())
    })?;
    Ok(GepWatch {
        duration_ms: summary.duration_ms,
        poll_interval_ms: summary.poll_interval_ms,
        polls: summary.polls,
        max_poll_us: summary.max_poll_us,
        events,
        sampling: summary.sampling,
    })
}

pub fn watch_stream(
    pid: u32,
    duration: Duration,
    limit: usize,
    emit: impl FnMut(&Payload) -> Result<(), String>,
) -> Result<GepWatchSummary, String> {
    watch_payloads(pid, duration, limit, None, emit)
}

pub fn watch_payloads(
    pid: u32,
    duration: Duration,
    limit: usize,
    payload_dir: Option<&Path>,
    mut emit: impl FnMut(&Payload) -> Result<(), String>,
) -> Result<GepWatchSummary, String> {
    validate_bounds(duration, limit)?;
    let identity = crate::game_observer::identify_process(pid)?;
    let mem = open_memory(pid)?;
    let sources = Sources::discover(&identity)?;
    let started = Instant::now();
    let deadline = started + duration;
    let mut sampler = Sampler::start(
        mem,
        sources,
        sampling::Options {
            skip_initial: true,
            deadline: Some(deadline),
            ..Default::default()
        },
        || true,
    )?;
    let mut events = 0;
    while Instant::now() < deadline && events < limit {
        let sample = match sampler
            .queue
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(sample) => sample,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        };
        let Content::Payload { source, bytes } = sample.content else {
            continue;
        };
        sampler
            .metrics
            .queue_delay
            .record(sample.captured.elapsed());
        let processing = Instant::now();
        let event = capture_payload(
            source,
            &bytes,
            Some(
                sample
                    .captured
                    .saturating_duration_since(started)
                    .as_millis(),
            ),
            sample.collected_at,
            payload_dir,
        )?;
        emit(&event)?;
        sampler.metrics.decode.record(processing.elapsed());
        sampler
            .metrics
            .decoded_sequence
            .store(sample.sequence, Ordering::Relaxed);
        events += 1;
    }
    sampler.stop()?;
    let sampling = sampler.report();
    Ok(GepWatchSummary {
        identity,
        duration_ms: started.elapsed().as_millis(),
        poll_interval_ms: POLL_INTERVAL.as_millis(),
        polls: sampling.poll.count,
        max_poll_us: u128::from(sampling.poll.max_us),
        events,
        sampling,
    })
}

fn open_memory(pid: u32) -> Result<File, String> {
    File::open(format!("/proc/{pid}/mem"))
        .map_err(|error| format!("could not open Warframe memory: {error}"))
}

fn layout(sources: &Sources) -> GepLayout {
    let (queue, item, body) = sources.response_offsets();
    GepLayout {
        manager_global: hex(sources.manager_global()),
        account: match sources.account_bindings() {
            Ok(value) => ProbeValue::Available { value },
            Err(reason) => ProbeValue::Unavailable { reason },
        },
        queue_table_offset: hex(queue),
        item_base_offset: hex(item),
        body_offset: hex(body),
    }
}

fn describe(
    source: &'static str,
    payload: &[u8],
    observed_after_ms: Option<u128>,
    observed_at_unix_ms: u128,
) -> Payload {
    Payload {
        source,
        bytes: payload.len(),
        sha256: format!("{:x}", Sha256::digest(payload)),
        contains_inventory: memchr::memmem::find(payload, INVENTORY_MARKER).is_some(),
        observed_after_ms,
        observed_at_unix_ms,
        file: None,
    }
}

fn capture_payload(
    source: &'static str,
    payload: &[u8],
    elapsed: Option<u128>,
    observed_at: u128,
    directory: Option<&Path>,
) -> Result<Payload, String> {
    let mut report = describe(source, payload, elapsed, observed_at);
    if let Some(directory) = directory {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(|e| e.to_string())?;
        let file = directory.join(format!("{}.bin", report.sha256));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&file)
        {
            Ok(mut output) => {
                if let Err(error) = output.write_all(payload) {
                    drop(output);
                    let _ = std::fs::remove_file(&file);
                    return Err(error.to_string());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::fs::read(&file).map_err(|e| e.to_string())? != payload {
                    return Err(format!(
                        "existing payload evidence does not match its hash: {}",
                        file.display()
                    ));
                }
            }
            Err(error) => return Err(error.to_string()),
        }
        report.file = Some(file);
    }
    Ok(report)
}

fn validate_bounds(duration: Duration, limit: usize) -> Result<(), String> {
    if duration.is_zero() || duration > Duration::from_secs(300) {
        return Err("duration must be between 1 ms and 300 seconds".to_owned());
    }
    if limit == 0 || limit > 1000 {
        return Err("event limit must be between 1 and 1000".to_owned());
    }
    Ok(())
}

fn hex(value: u64) -> String {
    format!("0x{value:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_report_keeps_identity_not_body() {
        let payload = describe("direct", br#"{"LastInventorySync":1}"#, Some(7), 123);
        assert_eq!(payload.source, "direct");
        assert!(payload.contains_inventory);
        assert_eq!(payload.observed_after_ms, Some(7));
        assert_eq!(payload.sha256.len(), 64);
        assert_eq!(payload.observed_at_unix_ms, 123);
    }

    #[test]
    fn watch_is_bounded() {
        assert!(validate_bounds(Duration::ZERO, 1).is_err());
        assert!(validate_bounds(Duration::from_secs(301), 1).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 0).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 1001).is_err());
    }
}
