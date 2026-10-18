use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use retinue_client::{
    child::{self, Monitor, Outcome},
    paths::Paths,
    reload,
    signals::Signals,
};
use retinue_poe2::inspect::Observer;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

use crate::{Observation, Publisher, Sample, now_ms};

pub const RELOAD: reload::Config = reload::Config {
    name: "poe2companion-reload",
    enable_env: "POE2COMPANION_DEV_RELOAD",
    handoff_env: "POE2COMPANION_RELOAD_FD",
    schema: 1,
    ready: "poe2companion-reload/1\n",
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    stream: String,
    sequence: u64,
    last: Option<Observation>,
    last_at: u64,
    pending: Option<Sample>,
    queued: Option<Sample>,
}

impl Default for State {
    fn default() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            stream: format!("{}:{nonce}", std::process::id()),
            sequence: 0,
            last: None,
            last_at: 0,
            pending: None,
            queued: None,
        }
    }
}

impl reload::Checkpoint for State {
    fn validate(&self) -> Result<(), String> {
        if self.stream.is_empty() || self.stream.len() > 128 || self.sequence >= u64::MAX - 1 {
            return Err("invalid PoE2 reload stream".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("PoE2 reload state exceeds limit".into());
        }
        for sample in self.pending.iter().chain(self.queued.iter()) {
            if sample.stream != self.stream
                || sample.sequence == 0
                || sample.sequence > self.sequence
                || serde_json::to_vec(sample).map_err(|e| e.to_string())?.len() > 65536
            {
                return Err("invalid PoE2 pending observation".into());
            }
        }
        if let (Some(pending), Some(queued)) = (&self.pending, &self.queued)
            && pending.sequence >= queued.sequence
        {
            return Err("invalid PoE2 reload ordering".into());
        }
        Ok(())
    }
}

impl State {
    fn observe(&mut self, observation: Observation) -> Result<(), String> {
        let now = now_ms();
        if self.last.as_ref() == Some(&observation) && now.saturating_sub(self.last_at) < 30_000 {
            return Ok(());
        }
        if let Some(error) = &observation.error
            && self.last.as_ref().and_then(|last| last.error.as_ref()) != Some(error)
        {
            eprintln!("poe2companion: observation unavailable: {error}");
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .filter(|seq| *seq < u64::MAX - 1)
            .ok_or("observation sequence exhausted")?;
        self.last = Some(observation.clone());
        self.last_at = now;
        self.queued = Some(Sample {
            stream: self.stream.clone(),
            sequence: self.sequence,
            observed_at: now,
            observation,
        });
        Ok(())
    }

    fn dispatch(&mut self, send: &mpsc::Sender<Sample>, busy: &mut bool) -> Result<(), String> {
        if *busy {
            return Ok(());
        }
        if self.pending.is_none() {
            self.pending = self.queued.take();
        }
        if let Some(sample) = &self.pending {
            send.try_send(sample.clone())
                .map_err(|e| format!("publisher admission: {e}"))?;
            *busy = true;
        }
        Ok(())
    }

    fn acknowledge(&mut self, sequence: u64, busy: &mut bool) -> Result<(), String> {
        if self.pending.as_ref().map(|sample| sample.sequence) != Some(sequence) {
            return Err("publisher acknowledgement does not match pending observation".into());
        }
        self.pending = None;
        *busy = false;
        Ok(())
    }
}

pub fn run(
    mut launch: Option<Vec<OsString>>,
    pid: Option<u32>,
    resumed: Option<reload::Handoff<State>>,
    overlay: bool,
) -> Result<u8, String> {
    RELOAD.enabled()?;
    let (installed, mut child, mut state) = match resumed {
        Some(handoff) => {
            launch = None;
            eprintln!("poe2companion: reload.resumed");
            (handoff.installed, handoff.child, handoff.state)
        }
        None => (
            retinue_client::executable_path()
                .ok_or("executable path unavailable")?
                .to_owned(),
            None,
            State::default(),
        ),
    };
    let mode = if launch.is_some() || child.is_some() {
        "launch"
    } else {
        "watch"
    };
    let mut rejected = None;
    loop {
        let stopping = Arc::new(AtomicBool::new(false));
        let signals = Signals::start(stopping.clone(), "poe2-signals", |_| {})?;
        let mut game = if let Some(identity) = child.take() {
            Some(Monitor::resume(
                identity,
                stopping.clone(),
                "poe2-game",
                child_event,
            )?)
        } else {
            launch
                .take()
                .map(|command| {
                    Monitor::launch(&command, stopping.clone(), "poe2-game", child_event)
                })
                .transpose()?
        };
        let gate = reload::Gate::default();
        let mut watch = reload::Watch::start(
            &RELOAD,
            installed.clone(),
            gate.clone(),
            stopping.clone(),
            rejected,
            reload_event,
        )?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let result = runtime.block_on(observe(
            pid,
            mode,
            &mut state,
            stopping.clone(),
            gate.clone(),
            overlay,
        ));
        drop(runtime);
        stopping.store(true, Ordering::Release);
        let outcome = game
            .as_mut()
            .map(Monitor::stop_monitor)
            .transpose()?
            .flatten();
        let candidate = watch.finish()?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        child = match outcome {
            Some(Outcome::Exited(status)) => {
                return Ok(status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
                    as u8);
            }
            Some(Outcome::Running(child)) => Some(child),
            None => None,
        };
        result?;
        if !gate.approved() {
            return Ok(0);
        }
        let candidate = candidate.ok_or("reload approved without replacement")?;
        rejected = Some(candidate.stamp);
        let handoff = reload::Handoff::new(&RELOAD, installed.clone(), child.take(), state);
        eprintln!("poe2companion: reload.exec");
        let error = candidate
            .replace(&RELOAD, &handoff, || !signals.requested())
            .unwrap_err();
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        eprintln!("poe2companion: reload.failed: {error}");
        child = handoff.child;
        state = handoff.state;
    }
}

struct Collector {
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Collector {
    fn start(
        pid: Option<u32>,
        stopping: Arc<AtomicBool>,
    ) -> Result<(Self, watch::Receiver<Option<Observation>>), String> {
        let (send, receive) = watch::channel(None);
        let stop = stopping.clone();
        let worker = thread::Builder::new()
            .name("poe2-observer".into())
            .spawn(move || {
                let mut observer = Observer::new(pid);
                while !stop.load(Ordering::Acquire) {
                    let next = Instant::now() + Duration::from_secs(1);
                    if send
                        .send(Some(Observation::from_result(observer.poll())))
                        .is_err()
                    {
                        break;
                    }
                    while Instant::now() < next && !stop.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok((
            Self {
                stopping,
                worker: Some(worker),
            },
            receive,
        ))
    }

    fn finish(&mut self) -> Result<(), String> {
        self.stopping.store(true, Ordering::Release);
        self.worker
            .take()
            .map(|w| w.join().map_err(|_| "observer panicked".to_owned()))
            .transpose()
            .map(|_| ())
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

async fn observe(
    pid: Option<u32>,
    mode: &str,
    state: &mut State,
    stopping: Arc<AtomicBool>,
    gate: reload::Gate,
    overlay: bool,
) -> Result<(), String> {
    let (mut collector, mut updates) = Collector::start(pid, stopping.clone())?;
    let _overlay = if overlay && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Some(crate::evaluate::Service::start(
            updates.clone(),
            stopping.clone(),
            gate.clone(),
        )?)
    } else {
        None
    };
    let (send, mut receive) = mpsc::channel::<Sample>(1);
    let (ack_send, mut ack) = mpsc::channel(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelled = cancel.clone();
    let mut publisher = Publisher::new(Paths::from_env().daemon_socket().to_owned(), mode);
    let worker = tokio::spawn(async move {
        while let Some(sample) = receive.recv().await {
            let result = publisher.publish(&sample, &cancelled).await;
            let failed = result.is_err();
            if ack_send.send((sample.sequence, result)).await.is_err() || failed {
                break;
            }
        }
    });
    let mut busy = false;
    let mut failed = false;
    let mut control = tokio::time::interval(Duration::from_millis(100));
    let result = loop {
        if !failed && let Err(error) = state.dispatch(&send, &mut busy) {
            break Err(error);
        }
        tokio::select! {
            _ = control.tick() => {
                if stopping.load(Ordering::Acquire) || gate.quiesce_if_idle(true, &stopping) { break Ok(()); }
            }
            changed = updates.changed() => {
                if changed.is_err() { break Err("native observer stopped".into()); }
                if let Some(observation) = updates.borrow_and_update().clone()
                    && let Err(error) = state.observe(observation) { break Err(error); }
            }
            result = ack.recv(), if busy && !failed => {
                match result {
                    Some((sequence, Ok(()))) => {
                        if let Err(error) = state.acknowledge(sequence, &mut busy) { break Err(error); }
                    }
                    failure => {
                        let error = match failure { Some((_, Err(error))) => error.to_string(), _ => "publisher stopped".into() };
                        if mode != "launch" { break Err(error); }
                        eprintln!("poe2companion: {error}; waiting for launched command or development reload");
                        failed = true;
                    }
                }
            }
        }
    };
    let joined = collector.finish();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    if !failed {
        loop {
            if state.dispatch(&send, &mut busy).is_err() || !busy {
                break;
            }
            match tokio::time::timeout_at(deadline, ack.recv()).await {
                Ok(Some((sequence, Ok(())))) => {
                    if state.acknowledge(sequence, &mut busy).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    }
    cancel.store(true, Ordering::Release);
    drop(send);
    drop(ack);
    worker.await.map_err(|e| e.to_string())?;
    if state.pending.is_some() {
        eprintln!("poe2companion: retaining unacknowledged observation");
    }
    joined?;
    result
}

fn child_event(event: child::Event) {
    match event {
        child::Event::WaitFailed(error) => eprintln!("poe2companion: {error}"),
        child::Event::MonitorPanicked => eprintln!("poe2companion: game monitor panicked"),
    }
}

fn reload_event(event: reload::Event) {
    match event {
        reload::Event::Watching(_) => eprintln!("poe2companion: reload.watching"),
        reload::Event::Pending(reason) => eprintln!("poe2companion: reload.pending: {reason}"),
        reload::Event::WatchFailed(error) => {
            eprintln!("poe2companion: reload.watch_failed: {error}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_client::reload::Checkpoint;

    #[test]
    fn queued_updates_coalesce_but_unacknowledged_sample_survives_reload() {
        let mut state = State::default();
        state.observe(Observation::default()).unwrap();
        let (send, mut receive) = mpsc::channel(1);
        let mut busy = false;
        state.dispatch(&send, &mut busy).unwrap();
        let initial = receive.try_recv().unwrap();
        state
            .observe(Observation::from_result(Err("one".into())))
            .unwrap();
        state
            .observe(Observation::from_result(Err("two".into())))
            .unwrap();
        let encoded = serde_json::to_vec(&state).unwrap();
        let mut restored: State = serde_json::from_slice(&encoded).unwrap();
        restored.validate().unwrap();
        busy = false;
        restored.dispatch(&send, &mut busy).unwrap();
        assert_eq!(
            serde_json::to_value(receive.try_recv().unwrap()).unwrap(),
            serde_json::to_value(initial).unwrap()
        );
        restored.acknowledge(1, &mut busy).unwrap();
        restored.dispatch(&send, &mut busy).unwrap();
        assert_eq!(
            receive.try_recv().unwrap().observation.error.as_deref(),
            Some("two")
        );
    }

    #[test]
    fn unchanged_observations_wait_for_heartbeat_and_bad_ack_is_rejected() {
        let mut state = State::default();
        state.observe(Observation::default()).unwrap();
        state.observe(Observation::default()).unwrap();
        assert_eq!(state.sequence, 1);
        state.last_at = 0;
        state.observe(Observation::default()).unwrap();
        assert_eq!(state.sequence, 2);
        assert!(state.acknowledge(999, &mut false).is_err());
    }
}
