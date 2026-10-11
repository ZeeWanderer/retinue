use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::game_observer::{self, DebugOutputEvent};
use crate::observation::debug_output::{self, Bridge, Event, Runtime};
use crate::observation::mailbox;

#[derive(Debug, Serialize)]
pub struct DebugWatch {
    pub game_pid: u32,
    pub duration_ms: u128,
    pub events: Vec<DebugRecord>,
    pub queue: mailbox::Stats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DebugRecord {
    pub game_pid: u32,
    pub observed_at_unix_ms: u128,
    pub observed_after_ms: u128,
    pub windows_pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<DebugOutputEvent>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DebugWatchSummary {
    pub game_pid: u32,
    pub duration_ms: u128,
    pub records: usize,
    pub queue: mailbox::Stats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<String>,
}

pub fn watch(duration: Duration, limit: usize) -> Result<DebugWatch, String> {
    let mut events = Vec::new();
    let summary = watch_stream(duration, limit, |record| {
        events.push(record.clone());
        Ok(())
    })?;
    Ok(DebugWatch {
        game_pid: summary.game_pid,
        duration_ms: summary.duration_ms,
        events,
        queue: summary.queue,
        stopped: summary.stopped,
    })
}

pub fn watch_stream(
    duration: Duration,
    limit: usize,
    mut emit: impl FnMut(&DebugRecord) -> Result<(), String>,
) -> Result<DebugWatchSummary, String> {
    validate_bounds(duration, limit)?;
    let game = game_observer::find_warframe();
    let attach = game
        .attach()
        .ok_or_else(|| "Warframe is not running with a discoverable Proton prefix".to_owned())?;
    let runtime = Runtime::discover(
        attach.pid(),
        attach.process_dir(),
        attach.environment(),
        attach.compat_data(),
    )
    .ok_or_else(|| "could not discover Warframe Proton runtime".to_owned())?;
    let receiver = debug_output::inbox();
    let started = Instant::now();
    let bridge = Bridge::start(&runtime, receiver.clone())?;
    let deadline = started + duration;
    let mut records = 0;
    let mut stopped = None;
    while Instant::now() < deadline && records < limit {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(Event::Record {
                sender_pid,
                message,
                observed_at,
                observed_at_unix_ms,
                ..
            }) => {
                let record = DebugRecord {
                    game_pid: runtime.game_pid(),
                    observed_at_unix_ms,
                    observed_after_ms: observed_at.saturating_duration_since(started).as_millis(),
                    windows_pid: sender_pid,
                    event: game_observer::classify_debug_output(&message),
                    message,
                };
                emit(&record)?;
                records += 1;
            }
            Ok(Event::Stopped { reason, .. }) => {
                stopped = Some(reason);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stopped = Some("DBWIN event channel closed".to_owned());
                break;
            }
        }
    }
    drop(bridge);
    Ok(DebugWatchSummary {
        game_pid: runtime.game_pid(),
        duration_ms: started.elapsed().as_millis(),
        records,
        queue: receiver.stats(),
        stopped,
    })
}

fn validate_bounds(duration: Duration, limit: usize) -> Result<(), String> {
    if duration.is_zero() || duration > Duration::from_secs(300) {
        return Err("duration must be between 1 ms and 300 seconds".to_owned());
    }
    if limit == 0 || limit > 10_000 {
        return Err("event limit must be between 1 and 10000".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_is_bounded() {
        assert!(validate_bounds(Duration::ZERO, 1).is_err());
        assert!(validate_bounds(Duration::from_secs(301), 1).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 0).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 10_001).is_err());
    }
}
