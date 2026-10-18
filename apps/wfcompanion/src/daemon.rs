use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::thread;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
#[cfg(test)]
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{self, MissedTickBehavior};
use tokio_util::codec::FramedRead;

use crate::relic::{CaptureArm, Trigger as RelicTrigger};
use crate::runtime::{diagnostics, inbox::value_bytes, presentation};
use crate::{UiEvent, incident};
use retinue_client::local_protocol::{
    ENVELOPE_VERSION, INTERFACE_DIAGNOSTICS, companion_interfaces,
};
#[cfg(test)]
use retinue_client::local_protocol::{
    INTERFACE_ASSETS, INTERFACE_CHECKS, INTERFACE_DATASETS, INTERFACE_GAME_METADATA,
    INTERFACE_MARKET, INTERFACE_PLAYER, INTERFACE_RELICS,
};

const CLIENT_VERSION: &str = env!("WFCLI_VERSION");
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RELIC_SUGGESTION_LIMIT: u64 = 32;
const STOP_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const START_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CLIENT_FRAME_BYTES: usize = 8 * 1024 * 1024;

mod framing;
mod outbox;
mod replay;
use framing::ServerFrames;
pub(crate) use replay::Replay;

use outbox::Receiver as OutboundReceiver;
pub(crate) use outbox::{Sender as OutboundSender, channel as outbound_channel};
type ReplySender = std_mpsc::Sender<Result<Value, String>>;
type PublicationKey = (&'static str, &'static str);

pub(crate) struct PendingRequest {
    receiver: std_mpsc::Receiver<Result<Value, String>>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl PendingRequest {
    pub(crate) fn poll(&self) -> Option<Result<Value, String>> {
        match self.receiver.try_recv() {
            Ok(reply) => Some(reply),
            Err(std_mpsc::TryRecvError::Disconnected) => {
                Some(Err("daemon request cancelled".into()))
            }
            Err(std_mpsc::TryRecvError::Empty) if Instant::now() >= self.deadline => {
                Some(Err("daemon request timed out".into()))
            }
            Err(std_mpsc::TryRecvError::Empty) => None,
        }
    }
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn begin_request(
    outbound: &OutboundSender,
    build: impl FnOnce(RequestReply) -> Outbound,
) -> Result<PendingRequest, String> {
    let (sender, receiver) = std_mpsc::channel();
    let reply = RequestReply::new(sender);
    let pending = PendingRequest {
        receiver,
        deadline: reply.deadline,
        cancelled: reply.cancelled.clone(),
    };
    outbound.send(build(reply)).map_err(str::to_owned)?;
    Ok(pending)
}

pub(crate) fn begin_market_quote(
    outbound: &OutboundSender,
    slug: String,
) -> Result<PendingRequest, String> {
    begin_request(outbound, |reply| Outbound::MarketQuote {
        items: vec![slug],
        reply,
    })
}

pub(crate) fn begin_ducat_inventory(outbound: &OutboundSender) -> Result<PendingRequest, String> {
    begin_request(outbound, |reply| Outbound::DucatInventory { reply })
}

pub(crate) struct Connection {
    outbound: OutboundSender,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Replay>>,
}

impl Connection {
    pub(crate) fn outbound(&self) -> OutboundSender {
        self.outbound.clone()
    }

    pub(crate) fn quiesce(&self) {
        self.outbound.cancel_requests();
    }

    pub(crate) fn finish(&mut self) -> Replay {
        self.quiesce();
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(replay) => return replay,
                Err(_) => incident::error("runtime.worker_panicked", "worker=daemon"),
            }
        }
        Replay::default()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.finish();
    }
}

#[derive(Clone)]
struct ServerEvents {
    ui: presentation::Sender,
    relic: crate::relic::Sender,
    diagnostics: diagnostics::Sender,
}

#[derive(Debug)]
pub(crate) struct RequestReply {
    sender: ReplySender,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl RequestReply {
    fn new(sender: ReplySender) -> Self {
        Self {
            sender,
            deadline: Instant::now() + REQUEST_TIMEOUT,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn failure(&self) -> Option<&'static str> {
        if self.cancelled.load(Ordering::Acquire) {
            Some("companion stopping")
        } else if Instant::now() >= self.deadline {
            Some("daemon request timed out")
        } else {
            None
        }
    }

    pub(crate) fn send(&self, result: Result<Value, String>) {
        let _ = self.sender.send(result);
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ClientMessage<'a> {
    Hello {
        id: u64,
        game: &'a str,
        envelope: u32,
        interfaces: &'a BTreeMap<&'static str, u32>,
        features: &'a [&'static str],
        client: &'a str,
        version: &'a str,
        pid: u32,
        mode: &'a str,
    },
    Get {
        id: u64,
        dataset: &'a str,
    },
    Subscribe {
        id: u64,
        dataset: &'a str,
        view: &'a str,
    },
    Publish {
        id: u64,
        dataset: &'a str,
        source: &'a str,
        data: &'a Value,
    },
    MarketResolve {
        id: u64,
        labels: &'a [String],
        limit: u64,
    },
    DucatInventory {
        id: u64,
    },
    MarketQuote {
        id: u64,
        items: &'a [String],
    },
    AssetResolve {
        id: u64,
        assets: &'a [Value],
    },
    RelicContext {
        id: u64,
        items: &'a [String],
    },
    RelicRecommendations {
        id: u64,
        era: &'a str,
        fetch_prices: bool,
        limit: u64,
    },
    DiagnosticsReport {
        id: u64,
        issues: &'a [Value],
    },
    CompanionDiagnostics {
        request_id: &'a str,
        data: &'a Value,
    },
}

#[derive(Debug)]
pub(crate) enum Outbound {
    DatasetGet {
        dataset: &'static str,
        reply: RequestReply,
    },
    Publish {
        dataset: &'static str,
        source: &'static str,
        data: Value,
    },
    MarketResolve {
        labels: Vec<String>,
        limit: u64,
        reply: RequestReply,
    },
    DucatInventory {
        reply: RequestReply,
    },
    MarketQuote {
        items: Vec<String>,
        reply: RequestReply,
    },
    AssetResolve {
        assets: Vec<Value>,
        reply: RequestReply,
    },
    RelicContext {
        items: Vec<String>,
        reply: RequestReply,
    },
    RelicRecommendations {
        era: String,
        fetch_prices: bool,
        limit: u64,
        reply: RequestReply,
    },
    DiagnosticsReport {
        issues: Vec<Value>,
    },
}

pub(crate) fn dataset_get(
    outbound: &OutboundSender,
    dataset: &'static str,
) -> Result<Value, String> {
    request(outbound, |reply| Outbound::DatasetGet { dataset, reply })
}

pub(crate) fn report_diagnostics(outbound: &OutboundSender, issues: Vec<Value>) {
    let _ = outbound.send(Outbound::DiagnosticsReport { issues });
}

pub(crate) fn market_resolve(
    outbound: &OutboundSender,
    labels: Vec<String>,
    limit: u64,
) -> Result<Value, String> {
    request(outbound, |reply| Outbound::MarketResolve {
        labels: labels.clone(),
        limit,
        reply,
    })
}

pub(crate) fn asset_resolve(
    outbound: &OutboundSender,
    assets: Vec<Value>,
) -> Result<Value, String> {
    request(outbound, |reply| Outbound::AssetResolve {
        assets: assets.clone(),
        reply,
    })
}

pub(crate) fn relic_context(
    outbound: &OutboundSender,
    items: Vec<String>,
) -> Result<Value, String> {
    request(outbound, |reply| Outbound::RelicContext {
        items: items.clone(),
        reply,
    })
}

pub(crate) fn relic_recommendations(
    outbound: &OutboundSender,
    era: String,
    fetch_prices: bool,
) -> Result<Value, String> {
    request(outbound, |reply| Outbound::RelicRecommendations {
        era: era.clone(),
        fetch_prices,
        limit: RELIC_SUGGESTION_LIMIT,
        reply,
    })
}

fn request(
    outbound: &OutboundSender,
    build: impl Fn(RequestReply) -> Outbound,
) -> Result<Value, String> {
    match request_once(outbound, &build) {
        Err(error) if error == "daemon connection closed" => {
            incident::warn("daemon.request_retry", &error);
            request_once(outbound, &build)
        }
        result => result,
    }
}

fn request_once(
    outbound: &OutboundSender,
    build: &impl Fn(RequestReply) -> Outbound,
) -> Result<Value, String> {
    let (reply_tx, reply_rx) = std_mpsc::channel();
    let mut reply = RequestReply::new(reply_tx);
    let cancelled = outbound.cancellation();
    reply.cancelled = cancelled.clone();
    let deadline = reply.deadline;
    outbound.send(build(reply)).map_err(str::to_owned)?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err("companion stopping".to_owned());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("daemon request timed out".to_owned());
        }
        match reply_rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(result) => return result,
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                return Err("daemon connection worker stopped".to_owned());
            }
        }
    }
}

pub(crate) fn spawn(
    ui: presentation::Sender,
    relic: crate::relic::Sender,
    diagnostics: diagnostics::Sender,
    mode: &'static str,
    replay: Replay,
) -> Result<Connection, String> {
    let latest = replay.latest()?;
    let (outbound_tx, outbound_rx) = outbound_channel();
    let events = ServerEvents {
        ui,
        relic,
        diagnostics,
    };
    let stopping = Arc::new(AtomicBool::new(false));
    let worker_stopping = stopping.clone();
    let worker = thread::Builder::new()
        .name("wfcompanion-daemon".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(connection_loop(
                    outbound_rx,
                    events,
                    worker_stopping,
                    mode,
                    latest,
                )),
                Err(error) => {
                    incident::error("daemon.runtime_failed", error.to_string());
                    replay
                }
            }
        })
        .map_err(|error| format!("could not start daemon worker: {error}"))?;
    Ok(Connection {
        outbound: outbound_tx,
        stopping,
        worker: Some(worker),
    })
}

async fn connection_loop(
    mut outbound: OutboundReceiver,
    events: ServerEvents,
    stopping: Arc<AtomicBool>,
    mode: &'static str,
    mut latest: BTreeMap<PublicationKey, Value>,
) -> Replay {
    let path = daemon_socket_path();
    let mut start_attempted = false;
    let mut queued = VecDeque::new();
    loop {
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        drain_outbound(&mut outbound, &mut latest, &mut queued);
        let connection = time::timeout(CONNECT_TIMEOUT, UnixStream::connect(&path))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "daemon connect timed out"))
            .and_then(|result| result);
        match connection {
            Ok(stream) => {
                start_attempted = false;
                if let Err(error) = connection_session(
                    stream,
                    &mut outbound,
                    &mut latest,
                    &mut queued,
                    &events,
                    &stopping,
                    mode,
                )
                .await
                {
                    let daemon_outdated = error.kind() == io::ErrorKind::Unsupported;
                    incident::warn("daemon.disconnected", error.to_string());
                    let _ = events.ui.send(UiEvent::Disconnected(error.to_string()));
                    if daemon_outdated {
                        ensure_daemon(&stopping).await;
                    }
                }
            }
            Err(error) => {
                incident::warn(
                    "daemon.connect_failed",
                    format!("{}: {error}", path.display()),
                );
                let _ = events.ui.send(UiEvent::Disconnected(format!(
                    "{}: {error}",
                    path.display()
                )));
                if !start_attempted {
                    start_attempted = true;
                    ensure_daemon(&stopping).await;
                }
            }
        }
        if stopping.load(Ordering::Relaxed)
            || !wait_for_reconnect(&mut outbound, &mut latest, &mut queued, &stopping).await
        {
            break;
        }
    }
    outbound.close();
    drain_outbound(&mut outbound, &mut latest, &mut queued);
    Replay::from_latest(latest)
}

async fn wait_for_reconnect(
    outbound: &mut OutboundReceiver,
    latest: &mut BTreeMap<PublicationKey, Value>,
    queued: &mut VecDeque<Outbound>,
    stopping: &AtomicBool,
) -> bool {
    let delay = time::sleep(RECONNECT_INTERVAL);
    tokio::pin!(delay);
    let mut stop_check = time::interval(STOP_CHECK_INTERVAL);
    stop_check.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = &mut delay => return true,
            message = outbound.recv() => match message {
                Some(message) => retain_outbound(message, latest, queued),
                None => return false,
            },
            _ = stop_check.tick() => {
                expire_queued(queued);
                if stopping.load(Ordering::Relaxed) {
                    return false;
                }
            }
        }
    }
}

async fn connection_session(
    mut stream: UnixStream,
    outbound: &mut OutboundReceiver,
    latest: &mut BTreeMap<PublicationKey, Value>,
    queued: &mut VecDeque<Outbound>,
    events: &ServerEvents,
    stopping: &AtomicBool,
    mode: &'static str,
) -> io::Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = FramedRead::new(reader, ServerFrames::new());
    let interfaces = companion_interfaces();
    let features = [
        "companion.command",
        "companion.diagnostics",
        "diagnostics.report",
    ];
    let hello = time::timeout(HANDSHAKE_TIMEOUT, async {
        send_message(
            &mut writer,
            &ClientMessage::Hello {
                id: 1,
                game: "warframe",
                envelope: ENVELOPE_VERSION,
                interfaces: &interfaces,
                features: &features,
                client: "wfcompanion",
                version: CLIENT_VERSION,
                pid: std::process::id(),
                mode,
            },
        )
        .await?;
        read_message(&mut reader).await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "daemon handshake timed out"))??;
    let negotiated = validate_hello(&hello)?;
    incident::info(
        "daemon.connected",
        format!("local_envelope={ENVELOPE_VERSION} mode={mode}"),
    );
    let _ = events.ui.send(UiEvent::Connected(
        hello
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    ));

    send_message(
        &mut writer,
        &ClientMessage::Subscribe {
            id: 2,
            dataset: "player",
            view: "hud",
        },
    )
    .await?;
    send_message(
        &mut writer,
        &ClientMessage::Get {
            id: 3,
            dataset: "daemon",
        },
    )
    .await?;

    let mut replay = latest
        .iter()
        .map(|(&(dataset, source), data)| Outbound::Publish {
            dataset,
            source,
            data: data.clone(),
        })
        .collect::<VecDeque<_>>();
    replay.append(queued);
    let mut pending = BTreeMap::new();

    let result = active_session(ActiveSession {
        writer: &mut writer,
        reader: &mut reader,
        outbound,
        queued: &mut replay,
        latest,
        events,
        stopping,
        next_id: 10,
        pending: &mut pending,
        diagnostics_report: negotiated.diagnostics_report,
    })
    .await;
    fail_pending(&mut pending, "daemon connection closed");
    for message in replay {
        if let Outbound::Publish {
            dataset,
            source,
            data,
        } = message
        {
            retain_publication(latest, dataset, source, data);
        } else if let Some(reply) = message.reply() {
            reply.send(Err("daemon connection closed".to_owned()));
        }
    }
    result
}

struct ActiveSession<'a, R, W> {
    writer: &'a mut W,
    reader: &'a mut FramedRead<R, ServerFrames>,
    outbound: &'a mut OutboundReceiver,
    queued: &'a mut VecDeque<Outbound>,
    latest: &'a mut BTreeMap<PublicationKey, Value>,
    events: &'a ServerEvents,
    stopping: &'a AtomicBool,
    next_id: u64,
    pending: &'a mut BTreeMap<u64, RequestReply>,
    diagnostics_report: bool,
}

async fn active_session<R, W>(session: ActiveSession<'_, R, W>) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let ActiveSession {
        writer,
        reader,
        outbound,
        queued,
        latest,
        events,
        stopping,
        mut next_id,
        pending,
        diagnostics_report,
    } = session;
    let mut stop_check = time::interval(STOP_CHECK_INTERVAL);
    stop_check.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut frame: Option<Writing> = None;
    let mut shutdown_deadline = None;
    let (diagnostic_tx, mut diagnostic_rx) = tokio::sync::mpsc::channel(32);
    loop {
        if stopping.load(Ordering::Relaxed) && shutdown_deadline.is_none() {
            outbound.close();
            fail_pending(pending, "companion stopping");
            shutdown_deadline = Some(Instant::now() + WRITE_TIMEOUT);
        }
        let mut input_exhausted = false;
        if frame.is_none()
            && let Ok(response) = diagnostic_rx.try_recv()
        {
            frame = Some(diagnostic_frame(response)?);
        }
        if frame.is_none() {
            for _ in 0..outbox::MAX_MESSAGES {
                let Some(message) = queued.pop_front().or_else(|| outbound.try_recv().ok()) else {
                    input_exhausted = true;
                    break;
                };
                if shutdown_deadline.is_some() && message.reply().is_some() {
                    continue;
                }
                let encoded =
                    match encode_outbound(next_id, message, latest, pending, diagnostics_report) {
                        Ok(encoded) => encoded,
                        Err(error) => {
                            if let Some(reply) = pending.remove(&next_id) {
                                reply.send(Err(error.to_string()));
                            }
                            incident::warn("daemon.outbound_rejected", error.to_string());
                            None
                        }
                    };
                next_id += 1;
                if let Some(bytes) = encoded {
                    frame = Some(Writing {
                        bytes,
                        offset: 0,
                        deadline: Instant::now() + WRITE_TIMEOUT,
                    });
                    break;
                }
            }
            if frame.is_none() && input_exhausted && shutdown_deadline.is_some() {
                return time::timeout_at(shutdown_deadline.unwrap().into(), writer.flush())
                    .await
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "daemon shutdown flush timed out")
                    })?;
            }
        }
        let write_deadline = frame.as_ref().map(|frame| frame.deadline);
        let deadline = write_deadline.into_iter().chain(shutdown_deadline).min();
        tokio::select! {
            response = diagnostic_rx.recv(), if frame.is_none() => {
                if let Some(response) = response {
                    frame = Some(diagnostic_frame(response)?);
                }
            },
            _ = stop_check.tick() => expire_pending(pending),
            _ = tokio::task::yield_now(), if frame.is_none() && !input_exhausted => {},
            _ = wait_until(deadline) => return Err(io::Error::new(io::ErrorKind::TimedOut, "daemon write timed out")),
            result = async {
                let frame = frame.as_ref().unwrap();
                writer.write(&frame.bytes[frame.offset..]).await
            }, if frame.is_some() => {
                let written = result?;
                if written == 0 {
                    return Err(io::Error::new(io::ErrorKind::WriteZero, "daemon write returned zero"));
                }
                let writing = frame.as_mut().unwrap();
                writing.offset += written;
                if writing.offset == writing.bytes.len() {
                    frame = None;
                }
            },
            message = outbound.recv(), if frame.is_none() && queued.is_empty() => match message {
                Some(message) => {
                    queued.push_back(message);
                }
                None => return Ok(()),
            },
            message = reader.next() => match message {
                Some(message) => {
                    let message = message?;
                    if message.get("event").and_then(Value::as_str) == Some("companion_diagnostics") {
                        diagnostics::route(&message, &events.diagnostics, &diagnostic_tx)?;
                    } else {
                        handle_server_message(message, events, pending);
                    }
                },
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "daemon closed",
                    ));
                }
            },
        }
    }
}

struct Writing {
    bytes: Vec<u8>,
    offset: usize,
    deadline: Instant,
}

fn diagnostic_frame(response: diagnostics::Response) -> io::Result<Writing> {
    Ok(Writing {
        bytes: encode_message(&ClientMessage::CompanionDiagnostics {
            request_id: &response.request_id,
            data: &response.data,
        })?,
        offset: 0,
        deadline: Instant::now() + WRITE_TIMEOUT,
    })
}

async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline.into()).await,
        None => std::future::pending().await,
    }
}

fn drain_outbound(
    outbound: &mut OutboundReceiver,
    latest: &mut BTreeMap<PublicationKey, Value>,
    queued: &mut VecDeque<Outbound>,
) {
    for _ in 0..outbox::MAX_MESSAGES {
        let Ok(message) = outbound.try_recv() else {
            break;
        };
        retain_outbound(message, latest, queued);
    }
}

fn retain_outbound(
    message: Outbound,
    latest: &mut BTreeMap<PublicationKey, Value>,
    queued: &mut VecDeque<Outbound>,
) {
    match message {
        Outbound::Publish {
            dataset,
            source,
            data,
        } if outbox::snapshot(dataset, source) => {
            retain_publication(latest, dataset, source, data);
        }
        message => {
            expire_queued(queued);
            let bytes = outbox::memory_cost(&message);
            if queued.len() >= outbox::MAX_MESSAGES
                || bytes
                    > outbox::MAX_BYTES.saturating_sub(queued.iter().map(outbox::memory_cost).sum())
            {
                outbox::reject(message, "daemon reconnect queue is full");
            } else {
                queued.push_back(message);
            }
        }
    }
}

fn expire_queued(queued: &mut VecDeque<Outbound>) {
    queued.retain(|message| {
        if let Some(reply) = message.reply()
            && let Some(reason) = reply.failure()
        {
            reply.send(Err(reason.to_owned()));
            false
        } else {
            true
        }
    });
}

fn retain_publication(
    latest: &mut BTreeMap<PublicationKey, Value>,
    dataset: &'static str,
    source: &'static str,
    data: Value,
) -> bool {
    let key = (dataset, source);
    let bytes = latest
        .iter()
        .filter(|(existing, _)| **existing != key)
        .map(|(_, data)| value_bytes(data))
        .sum::<usize>();
    if (latest.len() >= outbox::MAX_MESSAGES && !latest.contains_key(&key))
        || value_bytes(&data) > outbox::MAX_BYTES.saturating_sub(bytes)
    {
        incident::warn(
            "daemon.publication_rejected",
            format!("dataset={dataset} source={source} replay cache full"),
        );
        latest.remove(&key);
        false
    } else {
        latest.insert(key, data);
        true
    }
}

fn encode_outbound(
    id: u64,
    message: Outbound,
    latest: &mut BTreeMap<PublicationKey, Value>,
    pending: &mut BTreeMap<u64, RequestReply>,
    diagnostics_report: bool,
) -> io::Result<Option<Vec<u8>>> {
    match message {
        Outbound::DatasetGet { dataset, reply } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::Get { id, dataset }).map(Some)
        }
        Outbound::Publish {
            dataset,
            source,
            data,
        } => {
            let frame = encode_message(&ClientMessage::Publish {
                id,
                dataset,
                source,
                data: &data,
            })?;
            if outbox::snapshot(dataset, source) {
                retain_publication(latest, dataset, source, data);
            }
            Ok(Some(frame))
        }
        Outbound::MarketResolve {
            labels,
            limit,
            reply,
        } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::MarketResolve {
                id,
                labels: &labels,
                limit,
            })
            .map(Some)
        }
        Outbound::AssetResolve { assets, reply } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::AssetResolve {
                id,
                assets: &assets,
            })
            .map(Some)
        }
        Outbound::DucatInventory { reply } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::DucatInventory { id }).map(Some)
        }
        Outbound::MarketQuote { items, reply } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::MarketQuote { id, items: &items }).map(Some)
        }
        Outbound::RelicContext { items, reply } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::RelicContext { id, items: &items }).map(Some)
        }
        Outbound::RelicRecommendations {
            era,
            fetch_prices,
            limit,
            reply,
        } => {
            if !register_pending(pending, id, reply) {
                return Ok(None);
            }
            encode_message(&ClientMessage::RelicRecommendations {
                id,
                era: &era,
                fetch_prices,
                limit,
            })
            .map(Some)
        }
        Outbound::DiagnosticsReport { issues } => {
            if !diagnostics_report {
                return Ok(None);
            }
            encode_message(&ClientMessage::DiagnosticsReport {
                id,
                issues: &issues,
            })
            .map(Some)
        }
    }
}

fn register_pending(
    pending: &mut BTreeMap<u64, RequestReply>,
    id: u64,
    reply: RequestReply,
) -> bool {
    if let Some(reason) = reply.failure() {
        reply.send(Err(reason.to_owned()));
        false
    } else if pending.len() >= outbox::MAX_MESSAGES {
        reply.send(Err("too many pending daemon requests".to_owned()));
        false
    } else {
        pending.insert(id, reply);
        true
    }
}

fn expire_pending(pending: &mut BTreeMap<u64, RequestReply>) {
    pending.retain(|_, reply| {
        if let Some(reason) = reply.failure() {
            reply.send(Err(reason.to_owned()));
            false
        } else {
            true
        }
    });
}

fn fail_pending(pending: &mut BTreeMap<u64, RequestReply>, reason: &str) {
    for (_, reply) in std::mem::take(pending) {
        reply.send(Err(reason.to_owned()));
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct NegotiatedFeatures {
    diagnostics_report: bool,
}

fn validate_hello(message: &Value) -> io::Result<NegotiatedFeatures> {
    let compatible = message.get("id").and_then(Value::as_u64) == Some(1)
        && message.get("ok").and_then(Value::as_bool) == Some(true)
        && message.get("compatible").and_then(Value::as_bool) == Some(true);
    if !compatible {
        let daemon_envelope = message
            .get("envelope")
            .and_then(Value::as_u64)
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
        let mismatches = message
            .get("mismatches")
            .map_or_else(|| "unknown".to_owned(), Value::to_string);
        let kind = if daemon_contract_outdated(message) {
            io::ErrorKind::Unsupported
        } else {
            io::ErrorKind::InvalidData
        };
        return Err(io::Error::new(
            kind,
            format!(
                "daemon contract mismatch: companion envelope {ENVELOPE_VERSION}, daemon {daemon_envelope}; {mismatches}"
            ),
        ));
    }

    if message.get("envelope").and_then(Value::as_u64) != Some(u64::from(ENVELOPE_VERSION)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "daemon returned a different handshake envelope",
        ));
    }
    let offered = message
        .get("interfaces")
        .and_then(Value::as_object)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "daemon sent no interfaces"))?;
    for (name, version) in companion_interfaces() {
        if offered.get(name).and_then(Value::as_u64) != Some(u64::from(version)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("daemon interface mismatch: {name} requires {version}"),
            ));
        }
    }
    let diagnostics_report = message
        .get("features")
        .and_then(Value::as_array)
        .is_some_and(|features| {
            features
                .iter()
                .any(|value| value.as_str() == Some("diagnostics.report"))
        })
        && offered.get("diagnostics").and_then(Value::as_u64)
            == Some(u64::from(INTERFACE_DIAGNOSTICS));
    Ok(NegotiatedFeatures { diagnostics_report })
}

fn daemon_contract_outdated(message: &Value) -> bool {
    if message.get("envelope").is_none()
        && message.get("protocol").and_then(Value::as_u64).is_some()
    {
        return true;
    }
    let Some(envelope) = message.get("envelope").and_then(Value::as_u64) else {
        return false;
    };
    let Some(offered) = message.get("interfaces").and_then(Value::as_object) else {
        return false;
    };

    let mut mismatch = false;
    if envelope != u64::from(ENVELOPE_VERSION) {
        mismatch = true;
        if u64::from(ENVELOPE_VERSION) <= envelope {
            return false;
        }
    }
    for (name, required) in companion_interfaces() {
        let available = offered.get(name).and_then(Value::as_u64);
        if available == Some(u64::from(required)) {
            continue;
        }
        mismatch = true;
        if available.is_some_and(|version| u64::from(required) <= version) {
            return false;
        }
    }
    mismatch
}

async fn read_message<R>(reader: &mut FramedRead<R, ServerFrames>) -> io::Result<Value>
where
    R: AsyncRead + Unpin,
{
    match reader.next().await {
        None => Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "daemon closed during handshake",
        )),
        Some(message) => message,
    }
}

async fn send_message<W>(writer: &mut W, message: &ClientMessage<'_>) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let frame = encode_message(message)?;
    time::timeout(WRITE_TIMEOUT, writer.write_all(&frame))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "daemon write timed out"))?
}

fn encode_message(message: &ClientMessage<'_>) -> io::Result<Vec<u8>> {
    let mut frame = serde_json::to_vec(message)?;
    if frame.len() >= MAX_CLIENT_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "daemon outbound frame exceeds limit",
        ));
    }
    frame.push(b'\n');
    Ok(frame)
}

#[cfg(test)]
async fn send_outbound<W: AsyncWrite + Unpin>(
    writer: &mut W,
    id: u64,
    message: Outbound,
    latest: &mut BTreeMap<PublicationKey, Value>,
    pending: &mut BTreeMap<u64, RequestReply>,
    diagnostics_report: bool,
) -> io::Result<()> {
    if let Some(frame) = encode_outbound(id, message, latest, pending, diagnostics_report)? {
        writer.write_all(&frame).await?;
    }
    Ok(())
}

fn handle_server_message(
    message: Value,
    events: &ServerEvents,
    pending: &mut BTreeMap<u64, RequestReply>,
) {
    if message.get("event").and_then(Value::as_str) == Some("command") {
        let data = message.get("data");
        let command = data
            .and_then(|data| data.get("command"))
            .and_then(Value::as_str);
        match command {
            Some("overlay") => {
                if let Some(visible) = command_bool(data, "visible") {
                    let _ = events.ui.send(UiEvent::OverlayVisible(visible));
                }
            }
            Some("hud") => {
                if let Some(visible) = command_bool(data, "visible") {
                    let _ = events.ui.send(UiEvent::HudVisible(visible));
                }
            }
            Some("capture") => route_capture_command(data, &events.relic),
            _ => {}
        }
        return;
    }
    if message.get("event").and_then(Value::as_str) == Some("dataset") {
        send_snapshot(&message, &events.ui);
        return;
    }
    if message.get("event").and_then(Value::as_str) == Some("asset") {
        if let Some(data) = message.get("data")
            && let Ok(refresh) = serde_json::from_value::<crate::relic::AssetRefresh>(data.clone())
        {
            let _ = events.ui.send(UiEvent::AssetRefreshed(refresh));
        }
        return;
    }
    if let Some(reply) = message
        .get("id")
        .and_then(Value::as_u64)
        .and_then(|id| pending.remove(&id))
    {
        let result = if message.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(message)
        } else {
            Err(message
                .get("error")
                .map(Value::to_string)
                .unwrap_or_else(|| "daemon request failed".to_owned()))
        };
        reply.send(result);
        return;
    }
    if matches!(message.get("id").and_then(Value::as_u64), Some(2 | 3)) {
        send_snapshot(&message, &events.ui);
    }
}

fn command_bool(data: Option<&Value>, key: &str) -> Option<bool> {
    data.and_then(|data| data.get(key)).and_then(Value::as_bool)
}

fn route_capture_command(data: Option<&Value>, relic: &crate::relic::Sender) {
    let action = data
        .and_then(|data| data.get("action"))
        .and_then(Value::as_str);
    let target = data
        .and_then(|data| data.get("target"))
        .and_then(Value::as_str);
    match (action, target) {
        (Some("arm"), Some("relic_reward")) => {
            let Some(directory) = data
                .and_then(|data| data.get("directory"))
                .and_then(Value::as_str)
                .filter(|directory| !directory.is_empty())
            else {
                incident::warn("relic.capture_command_rejected", "missing directory");
                return;
            };
            let timeout_ms = data
                .and_then(|data| data.get("timeout_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(30 * 60 * 1000)
                .clamp(1_000, 24 * 60 * 60 * 1000);
            let _ = relic.send(RelicTrigger::ArmCapture(CaptureArm {
                directory: PathBuf::from(directory),
                timeout: Duration::from_millis(timeout_ms),
            }));
        }
        (Some("cancel"), None | Some("relic_reward")) => {
            let _ = relic.send(RelicTrigger::CancelCapture);
        }
        _ => incident::warn("relic.capture_command_rejected", "invalid capture command"),
    }
}

fn send_snapshot(message: &Value, ui: &presentation::Sender) {
    if message.get("dataset").and_then(Value::as_str) != Some("player") {
        return;
    }
    let Some(data) = message.get("data") else {
        return;
    };
    let _ = ui.send(UiEvent::Player(presentation::PlayerStatus::from_snapshot(
        data,
    )));
}

async fn ensure_daemon(stopping: &AtomicBool) {
    let command = retinue_command();
    let invocation = format!("{} daemon ensure", command.display());
    let mut process = ProcessCommand::new(&command);
    process.args(["daemon", "ensure"]);
    sanitize_native_child(&mut process);
    process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match run_helper(process, stopping, START_TIMEOUT).await {
        Ok(status) if status.success() => {
            incident::info("daemon.ensure", format!("command={invocation}"));
        }
        Ok(status) => incident::warn(
            "daemon.ensure_failed",
            format!("command={invocation} status={status}"),
        ),
        Err(error) => incident::error(
            "daemon.ensure_failed",
            format!("command={invocation} error={error}"),
        ),
    }
}

async fn run_helper(
    process: ProcessCommand,
    stopping: &AtomicBool,
    timeout: Duration,
) -> io::Result<std::process::ExitStatus> {
    if stopping.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "companion stopping",
        ));
    }
    let mut child = tokio::process::Command::from(process)
        .kill_on_drop(true)
        .spawn()?;
    let deadline = time::sleep(timeout);
    tokio::pin!(deadline);
    let mut stop_check = time::interval(STOP_CHECK_INTERVAL);
    stop_check.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let error = loop {
        tokio::select! {
            result = child.wait() => return result,
            _ = &mut deadline => break io::Error::new(io::ErrorKind::TimedOut, "daemon ensure timed out"),
            _ = stop_check.tick() => {
                if stopping.load(Ordering::Relaxed) {
                    break io::Error::new(io::ErrorKind::Interrupted, "companion stopping");
                }
            }
        }
    };
    child.kill().await?;
    Err(error)
}

fn sanitize_native_child(process: &mut ProcessCommand) {
    for name in [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "STEAM_RUNTIME",
        "STEAM_RUNTIME_LIBRARY_PATH",
    ] {
        process.env_remove(name);
    }
}

fn retinue_command() -> PathBuf {
    if let Some(path) = std::env::var_os("RETINUE_COMMAND") {
        return PathBuf::from(path);
    }
    find_retinue(
        retinue_client::executable_path(),
        std::env::current_dir().ok().as_deref(),
    )
}

fn find_retinue(executable: Option<&Path>, current: Option<&Path>) -> PathBuf {
    if let Some(executable) = executable {
        for ancestor in executable.ancestors() {
            let candidate = ancestor.join("retinue");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    if let Some(current) = current {
        let candidate = current.join("retinue");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("retinue")
}

pub(crate) fn daemon_socket_path() -> PathBuf {
    retinue_client::local_protocol::socket_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_nonblocking_request_cancels_only_itself() {
        let (outbound, mut receiver) = outbound_channel();
        let pending = begin_ducat_inventory(&outbound).unwrap();
        assert!(pending.poll().is_none());
        let Outbound::DucatInventory { reply } = receiver.try_recv().unwrap() else {
            panic!()
        };
        drop(pending);
        assert_eq!(reply.failure(), Some("companion stopping"));
        assert!(!outbound.cancellation().load(Ordering::Acquire));
    }

    #[test]
    fn nonblocking_request_delivers_reply_and_times_out() {
        let (outbound, mut receiver) = outbound_channel();
        let mut pending = begin_market_quote(&outbound, "item".into()).unwrap();
        let Outbound::MarketQuote { reply, .. } = receiver.try_recv().unwrap() else {
            panic!()
        };
        reply.send(Ok(serde_json::json!({"data": {"items": []}})));
        assert!(pending.poll().unwrap().is_ok());
        pending.deadline = Instant::now() - Duration::from_secs(1);
        assert!(pending.poll().unwrap().is_err());
    }

    #[tokio::test]
    async fn diagnostics_use_connection_scoped_replies_without_player_replay() {
        let (client, server) = tokio::io::duplex(4096);
        let (read, mut write) = tokio::io::split(client);
        let mut reader = FramedRead::new(read, ServerFrames::new());
        let (_sender, mut outbound) = outbound_channel();
        let (mut events, _, _) = server_events();
        let (requests, receiver) = diagnostics::channel();
        events.diagnostics = requests.clone();
        let stopping = AtomicBool::new(false);
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        let mut pending = BTreeMap::new();
        let session = active_session(ActiveSession {
            writer: &mut write,
            reader: &mut reader,
            outbound: &mut outbound,
            queued: &mut queued,
            latest: &mut latest,
            events: &events,
            stopping: &stopping,
            next_id: 10,
            pending: &mut pending,
            diagnostics_report: false,
        });
        let id = "0123456789abcdef0123456789abcdef";
        let serve = async {
            let (read, mut write) = tokio::io::split(server);
            write
                .write_all(
                    format!(
                        "{}\n",
                        serde_json::json!({
                            "event": "companion_diagnostics", "request_id": id,
                            "request": {"action": "watch", "topic": "inventory", "seconds": 60}
                        })
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let response = FramedRead::new(read, ServerFrames::new())
                .next()
                .await
                .unwrap()
                .unwrap();
            assert_eq!(response["op"], "companion_diagnostics");
            assert_eq!(response["request_id"], id);
            assert_eq!(response["data"]["snapshot"]["receipts"], 7);
        };
        let observe = async {
            while requests.stats().items == 0 {
                tokio::task::yield_now().await;
            }
            let mut watches = diagnostics::Watches::new(receiver);
            watches.tick(|| serde_json::json!({"receipts": 7}));
            watches
        };
        let (result, (), mut watches) = time::timeout(Duration::from_secs(2), async {
            tokio::join!(session, serve, observe)
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ConnectionReset);
        assert!(latest.is_empty() && queued.is_empty() && pending.is_empty());
        let (replies, mut output) = tokio::sync::mpsc::channel(32);
        diagnostics::route(
            &serde_json::json!({"request_id": id,
            "request": {"action": "status"}}),
            &requests,
            &replies,
        )
        .unwrap();
        watches.tick(|| serde_json::json!({}));
        let status = output.try_recv().unwrap().data;
        assert_eq!(status["jobs"][0]["state"], "cancelled");
        assert_eq!(status["jobs"][0]["reason"], "connection closed");
    }

    #[tokio::test]
    async fn stalled_write_still_routes_controls_and_resumes_exact_frame() {
        let (client, server) = tokio::io::duplex(32);
        let (read, mut write) = tokio::io::split(client);
        let mut reader = FramedRead::new(read, ServerFrames::new());
        let (sender, mut outbound) = outbound_channel();
        let (events, ui, _) = server_events();
        let payload = "x".repeat(10_000);
        sender
            .send(Outbound::Publish {
                dataset: "player",
                source: "inventory_http",
                data: Value::String(payload.clone()),
            })
            .unwrap();
        let stopping = AtomicBool::new(false);
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        let mut pending = BTreeMap::new();
        let session = active_session(ActiveSession {
            writer: &mut write,
            reader: &mut reader,
            outbound: &mut outbound,
            queued: &mut queued,
            latest: &mut latest,
            events: &events,
            stopping: &stopping,
            next_id: 10,
            pending: &mut pending,
            diagnostics_report: false,
        });
        let serve = async {
            let (read, mut write) = tokio::io::split(server);
            write.write_all(b"{\"event\":\"command\",\"data\":{\"command\":\"overlay\",\"visible\":false}}\n").await.unwrap();
            time::timeout(Duration::from_secs(1), async {
                loop {
                    match ui.try_recv() {
                        Ok(UiEvent::OverlayVisible(false)) => break,
                        Err(std_mpsc::TryRecvError::Empty) => {
                            time::sleep(Duration::from_millis(1)).await
                        }
                        event => panic!("unexpected control: {event:?}"),
                    }
                }
            })
            .await
            .expect("control blocked behind outbound write");
            let mut frames = FramedRead::new(read, ServerFrames::new());
            let published = frames.next().await.unwrap().unwrap();
            assert_eq!(published["data"], payload);
            assert_eq!(published["id"], 10);
            stopping.store(true, Ordering::Release);
            drop(sender);
            // Keep the peer alive until the session notices shutdown.
            time::sleep(Duration::from_millis(20)).await;
        };
        let (result, ()) = time::timeout(Duration::from_secs(2), async {
            tokio::join!(session, serve)
        })
        .await
        .unwrap();
        result.unwrap();
    }

    #[test]
    fn quiescence_interrupts_waiting_requests_but_accepts_final_publications() {
        let (sender, mut receiver) = outbound_channel();
        let caller = sender.clone();
        let worker = thread::spawn(move || dataset_get(&caller, "player"));
        let message = receiver.blocking_recv().unwrap();
        sender.cancel_requests();
        assert_eq!(worker.join().unwrap(), Err("companion stopping".into()));
        let mut pending = BTreeMap::new();
        let Outbound::DatasetGet { reply, .. } = message else {
            panic!()
        };
        assert!(!register_pending(&mut pending, 1, reply));
        assert_eq!(
            dataset_get(&sender, "player"),
            Err("companion stopping".into())
        );
        sender
            .send(Outbound::Publish {
                dataset: "player",
                source: "game",
                data: serde_json::json!({"running": false}),
            })
            .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            Outbound::Publish { .. }
        ));
    }

    #[test]
    fn reconnect_queue_is_bounded_and_expires_requests() {
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        let mut replies = Vec::new();
        for _ in 0..outbox::MAX_MESSAGES + 1 {
            let (sender, receiver) = std_mpsc::channel();
            replies.push(receiver);
            retain_outbound(
                Outbound::DatasetGet {
                    dataset: "player",
                    reply: RequestReply::new(sender),
                },
                &mut latest,
                &mut queued,
            );
        }
        assert_eq!(queued.len(), outbox::MAX_MESSAGES);
        assert_eq!(
            replies.pop().unwrap().try_recv().unwrap(),
            Err("daemon reconnect queue is full".into())
        );
        for message in &queued {
            message
                .reply()
                .unwrap()
                .cancelled
                .store(true, Ordering::Release);
        }
        expire_queued(&mut queued);
        assert!(queued.is_empty());
        for reply in replies {
            assert_eq!(reply.try_recv().unwrap(), Err("companion stopping".into()));
        }
    }

    #[tokio::test]
    async fn broken_frames_fail_pending_requests_and_next_session_recovers() {
        for response in [
            b"{invalid}\n".as_slice(),
            b"{\"id\":10}",
            b"{\"id\":10,\"ok\":true}\n",
        ] {
            let (client, mut server) = UnixStream::pair().unwrap();
            let (sender, mut outbound) = outbound_channel();
            let (reply, result) = std_mpsc::channel();
            let mut queued = VecDeque::from([Outbound::DatasetGet {
                dataset: "player",
                reply: RequestReply::new(reply),
            }]);
            let mut latest = BTreeMap::new();
            let (events, _, _) = server_events();
            let stopping = AtomicBool::new(false);
            let session = connection_session(
                client,
                &mut outbound,
                &mut latest,
                &mut queued,
                &events,
                &stopping,
                "standalone",
            );
            let serve = async {
                let (read, mut write) = server.split();
                let mut frames = FramedRead::new(read, ServerFrames::new());
                assert_eq!(frames.next().await.unwrap().unwrap()["op"], "hello");
                let mut hello = serde_json::to_vec(&serde_json::json!({
                    "id": 1, "ok": true, "compatible": true, "envelope": ENVELOPE_VERSION,
                    "interfaces": companion_interfaces()
                }))
                .unwrap();
                hello.push(b'\n');
                write.write_all(&hello).await.unwrap();
                for id in [2, 3, 10] {
                    let request = frames.next().await.unwrap().unwrap();
                    assert_eq!(request["id"], id);
                    if id == 2 {
                        assert_eq!(request["view"], "hud");
                    }
                }
                write.write_all(response).await.unwrap();
                write.shutdown().await.unwrap();
            };
            let (session_result, ()) = time::timeout(Duration::from_secs(2), async {
                tokio::join!(session, serve)
            })
            .await
            .unwrap();
            assert!(session_result.is_err());
            assert!(queued.is_empty());
            let reply = result.try_recv().unwrap();
            if response == b"{\"id\":10,\"ok\":true}\n" {
                assert_eq!(reply.unwrap()["id"], 10);
            } else {
                assert_eq!(reply, Err("daemon connection closed".to_owned()));
            }
            drop(sender);
        }
    }

    #[tokio::test]
    async fn partial_frame_does_not_block_shutdown() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let (read, mut write) = client.split();
        let mut reader = FramedRead::new(read, ServerFrames::new());
        server.write_all(b"{\"id\":").await.unwrap();
        let (_sender, mut outbound) = outbound_channel();
        let (events, _, _) = server_events();
        let stopping = AtomicBool::new(true);
        let mut latest = BTreeMap::new();
        let mut pending = BTreeMap::new();
        time::timeout(
            Duration::from_secs(2),
            active_session(ActiveSession {
                reader: &mut reader,
                writer: &mut write,
                outbound: &mut outbound,
                queued: &mut VecDeque::new(),
                latest: &mut latest,
                pending: &mut pending,
                events: &events,
                stopping: &stopping,
                next_id: 10,
                diagnostics_report: false,
            }),
        )
        .await
        .unwrap()
        .unwrap();
    }

    #[tokio::test]
    async fn shutdown_drains_publications_but_not_new_requests() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let (read, mut write) = client.split();
        let mut reader = FramedRead::new(read, ServerFrames::new());
        let (sender, mut outbound) = outbound_channel();
        let (reply, result) = std_mpsc::channel();
        sender
            .send(Outbound::DatasetGet {
                dataset: "player",
                reply: RequestReply::new(reply),
            })
            .unwrap();
        sender
            .send(Outbound::Publish {
                dataset: "player",
                source: "capture_result",
                data: serde_json::json!({"state": "saved"}),
            })
            .unwrap();
        let (events, _, _) = server_events();
        let stopping = AtomicBool::new(true);
        let mut latest = BTreeMap::new();
        let mut pending = BTreeMap::new();
        let mut queued = VecDeque::new();
        for _ in 0..outbox::MAX_MESSAGES {
            let (reply, _) = std_mpsc::channel();
            queued.push_back(Outbound::DatasetGet {
                dataset: "player",
                reply: RequestReply::new(reply),
            });
        }
        time::timeout(
            Duration::from_secs(2),
            active_session(ActiveSession {
                reader: &mut reader,
                writer: &mut write,
                outbound: &mut outbound,
                queued: &mut queued,
                latest: &mut latest,
                pending: &mut pending,
                events: &events,
                stopping: &stopping,
                next_id: 10,
                diagnostics_report: false,
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(sender.is_closed());
        assert!(pending.is_empty());
        assert!(matches!(
            result.try_recv(),
            Err(std_mpsc::TryRecvError::Disconnected)
        ));
        let mut frames = FramedRead::new(&mut server, ServerFrames::new());
        let frame = time::timeout(Duration::from_secs(2), frames.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(frame["op"], "publish");
        assert_eq!(frame["source"], "capture_result");
        assert_eq!(frame["data"]["state"], "saved");
    }

    #[tokio::test]
    async fn helper_timeout_and_stop_leave_async_runtime_responsive() {
        let stopping = AtomicBool::new(false);
        let mut command = ProcessCommand::new("sleep");
        command.arg("60");
        let helper = run_helper(command, &stopping, Duration::from_millis(30));
        let (result, ()) = tokio::join!(helper, async {
            time::sleep(Duration::from_millis(1)).await;
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        let mut command = ProcessCommand::new("sleep");
        command.arg("60");
        let (result, ()) = time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                run_helper(command, &stopping, Duration::from_secs(30)),
                async {
                    time::sleep(Duration::from_millis(1)).await;
                    stopping.store(true, Ordering::Relaxed);
                }
            )
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn daemon_start_uses_companions_install_prefix() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("wfcompanion-cli-{}-{unique}", std::process::id()));
        for directory in ["", "dev/bin", "prod/bin"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
            std::fs::write(root.join(directory).join("retinue"), b"").unwrap();
        }
        let selected: Vec<_> = ["dev", "prod"]
            .into_iter()
            .map(|mode| {
                let bin = root.join(mode).join("bin");
                (
                    find_retinue(Some(&bin.join("wfcompanion")), Some(&root)),
                    bin.join("retinue"),
                )
            })
            .collect();
        assert_eq!(find_retinue(None, Some(&root)), root.join("retinue"));
        assert_eq!(find_retinue(None, None), PathBuf::from("retinue"));
        std::fs::remove_dir_all(root).unwrap();
        for (actual, expected) in selected {
            assert_eq!(actual, expected);
        }
    }

    fn server_events() -> (ServerEvents, presentation::Receiver, crate::relic::Receiver) {
        let (ui, ui_events) = presentation::channel();
        let (relic, relic_events) = crate::relic::channel();
        let (diagnostics, _) = diagnostics::channel();
        (
            ServerEvents {
                ui,
                relic,
                diagnostics,
            },
            ui_events,
            relic_events,
        )
    }

    #[test]
    fn records_latest_value_for_reconnect_replay() {
        let (sender, mut receiver) = outbound_channel();
        sender
            .send(Outbound::Publish {
                dataset: "player",
                source: "game",
                data: serde_json::json!({"running": false}),
            })
            .unwrap();
        sender
            .send(Outbound::Publish {
                dataset: "player",
                source: "game",
                data: serde_json::json!({"running": true}),
            })
            .unwrap();

        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        drain_outbound(&mut receiver, &mut latest, &mut queued);
        assert_eq!(latest[&("player", "game")]["running"], true);
        assert!(queued.is_empty());
    }

    #[test]
    fn reconnect_replay_keeps_publications_from_distinct_datasets() {
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        retain_outbound(
            Outbound::Publish {
                dataset: "player",
                source: "inventory_http",
                data: serde_json::json!({"inventory": true}),
            },
            &mut latest,
            &mut queued,
        );
        retain_outbound(
            Outbound::Publish {
                dataset: "game_metadata",
                source: "warframe",
                data: serde_json::json!({"schema": 2}),
            },
            &mut latest,
            &mut queued,
        );
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[&("player", "inventory_http")]["inventory"], true);
        assert_eq!(latest[&("game_metadata", "warframe")]["schema"], 2);
    }

    #[test]
    fn reconnect_replays_full_inventory_before_native_scope() {
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        for (source, sequence) in [
            ("inventory_http", 1),
            ("inventory_native", 2),
            ("inventory_native", 3),
        ] {
            retain_outbound(
                Outbound::Publish {
                    dataset: "player",
                    source,
                    data: serde_json::json!({"sequence": sequence}),
                },
                &mut latest,
                &mut queued,
            );
        }
        assert!(queued.is_empty());
        let replay: Vec<_> = latest
            .iter()
            .map(|((_, source), data)| (*source, data["sequence"].as_u64().unwrap()))
            .collect();
        assert_eq!(replay, [("inventory_http", 1), ("inventory_native", 3)]);
    }

    #[test]
    fn preserves_disconnected_diagnostics_reports() {
        let mut latest = BTreeMap::new();
        let mut queued = VecDeque::new();
        retain_outbound(
            Outbound::DiagnosticsReport {
                issues: vec![serde_json::json!({"identity": "old"})],
            },
            &mut latest,
            &mut queued,
        );
        retain_outbound(
            Outbound::DiagnosticsReport {
                issues: vec![serde_json::json!({"identity": "new"})],
            },
            &mut latest,
            &mut queued,
        );

        assert_eq!(queued.len(), 2);
        for expected in ["old", "new"] {
            let Some(Outbound::DiagnosticsReport { issues }) = queued.pop_front() else {
                panic!("expected diagnostics report");
            };
            assert_eq!(issues[0]["identity"], expected);
        }
    }

    #[test]
    fn active_session_wakes_for_outbound_publish() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut client, server) = UnixStream::pair().unwrap();
            let (reader, mut writer) = client.split();
            let mut reader = FramedRead::new(reader, ServerFrames::new());
            let mut server = BufReader::new(server).lines();
            let (sender, mut outbound) = outbound_channel();
            let (events, _ui_events, _relic_events) = server_events();
            let stopping = AtomicBool::new(false);
            let mut latest = BTreeMap::new();
            let mut pending = BTreeMap::new();
            let mut queued = VecDeque::new();
            let session = active_session(ActiveSession {
                writer: &mut writer,
                reader: &mut reader,
                outbound: &mut outbound,
                queued: &mut queued,
                latest: &mut latest,
                events: &events,
                stopping: &stopping,
                next_id: 10,
                pending: &mut pending,
                diagnostics_report: false,
            });
            tokio::pin!(session);

            let line = time::timeout(Duration::from_secs(1), async {
                time::sleep(Duration::from_millis(10)).await;
                sender
                    .send(Outbound::Publish {
                        dataset: "player",
                        source: "game",
                        data: serde_json::json!({"running": true}),
                    })
                    .unwrap();
                tokio::select! {
                    result = &mut session => panic!("session ended before publish: {result:?}"),
                    line = server.next_line() => line.unwrap().unwrap(),
                }
            })
            .await
            .unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(message["op"], "publish");
            assert_eq!(message["source"], "game");
            assert_eq!(message["data"]["running"], true);

            drop(server);
            assert!(
                time::timeout(Duration::from_secs(1), &mut session)
                    .await
                    .unwrap()
                    .is_err()
            );
        });
    }

    #[test]
    fn sends_dataset_get_and_routes_its_reply() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut client, server) = UnixStream::pair().unwrap();
            let mut server = BufReader::new(server).lines();
            let (reply, result) = std_mpsc::channel();
            let mut latest = BTreeMap::new();
            let mut pending = BTreeMap::new();
            send_outbound(
                &mut client,
                10,
                Outbound::DatasetGet {
                    dataset: "game_metadata",
                    reply: RequestReply::new(reply),
                },
                &mut latest,
                &mut pending,
                false,
            )
            .await
            .unwrap();

            let request: Value = serde_json::from_str(
                &time::timeout(Duration::from_secs(1), server.next_line())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(request["op"], "get");
            assert_eq!(request["dataset"], "game_metadata");

            let (events, _ui_events, _relic_events) = server_events();
            handle_server_message(
                serde_json::json!({"id":10,"ok":true,"data":{"data":{"schema":2}}}),
                &events,
                &mut pending,
            );
            assert_eq!(result.recv().unwrap().unwrap()["data"]["data"]["schema"], 2);
        });
    }

    #[test]
    fn expired_request_is_not_registered() {
        let (sender, result) = std_mpsc::channel();
        let reply = RequestReply {
            sender,
            deadline: Instant::now() - Duration::from_millis(1),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let mut pending = BTreeMap::new();

        assert!(!register_pending(&mut pending, 17, reply));
        assert_eq!(
            result.recv().unwrap(),
            Err("daemon request timed out".to_owned())
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn rejects_incompatible_handshake() {
        let result = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": false,
            "compatible": false,
            "envelope": 2,
            "mismatches": [{"kind": "envelope"}]
        }));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn requests_update_only_for_older_daemon_contract() {
        let result = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": false,
            "compatible": false,
            "envelope": ENVELOPE_VERSION,
            "interfaces": {
                "player": INTERFACE_PLAYER,
                "market": INTERFACE_MARKET,
                "relics": INTERFACE_RELICS,
                "assets": INTERFACE_ASSETS,
                "diagnostics": INTERFACE_DIAGNOSTICS,
                "checks": INTERFACE_CHECKS
            }
        }));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Unsupported);

        let result = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": false,
            "compatible": false,
            "envelope": ENVELOPE_VERSION,
            "interfaces": {
                "datasets": INTERFACE_DATASETS + 1,
                "player": INTERFACE_PLAYER,
                "market": INTERFACE_MARKET,
                "relics": INTERFACE_RELICS,
                "assets": INTERFACE_ASSETS,
                "diagnostics": INTERFACE_DIAGNOSTICS,
                "checks": INTERFACE_CHECKS
            }
        }));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);

        let result = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": false,
            "compatible": false,
            "protocol": 13
        }));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn rejects_handshake_missing_required_interface() {
        let result = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": true,
            "compatible": true,
            "envelope": ENVELOPE_VERSION,
            "interfaces": {
                "datasets": INTERFACE_DATASETS,
                "player": INTERFACE_PLAYER
            }
        }));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("interface mismatch")
        );
    }

    #[test]
    fn accepts_handshake_with_required_interfaces() {
        let negotiated = validate_hello(&serde_json::json!({
            "id": 1,
            "ok": true,
            "compatible": true,
            "envelope": ENVELOPE_VERSION,
            "interfaces": {
                "datasets": INTERFACE_DATASETS,
                "player": INTERFACE_PLAYER,
                "game_metadata": INTERFACE_GAME_METADATA,
                "market": INTERFACE_MARKET,
                "relics": INTERFACE_RELICS,
                "assets": INTERFACE_ASSETS,
                "diagnostics": INTERFACE_DIAGNOSTICS,
                "checks": INTERFACE_CHECKS
            },
            "features": ["companion.command", "diagnostics.report"]
        }))
        .unwrap();
        assert!(negotiated.diagnostics_report);
    }

    #[test]
    fn routes_correlated_request_reply() {
        let (events, _ui_events, _relic_events) = server_events();
        let (reply, result) = std_mpsc::channel();
        let mut pending = BTreeMap::from([(17, RequestReply::new(reply))]);
        handle_server_message(
            serde_json::json!({"id":17,"ok":true,"data":{"matches":[]}}),
            &events,
            &mut pending,
        );
        assert_eq!(result.recv().unwrap().unwrap()["id"], 17);
        assert!(pending.is_empty());
    }

    #[test]
    fn player_events_project_hud_state_without_retaining_unrelated_datasets() {
        let (events, ui, _relic) = server_events();
        let mut pending = BTreeMap::new();
        handle_server_message(
            serde_json::json!({"event":"dataset", "dataset":"worldstate", "data":{"data":{"large":"unused"}}}),
            &events,
            &mut pending,
        );
        assert!(ui.try_recv().is_err());
        for debug_lines in [1, 2] {
            handle_server_message(
                serde_json::json!({"event":"dataset", "dataset":"player", "data":{"data":{
                    "game":{"phase":"game", "pid":42},
                    "collector":{"debug_output_active":true,"debug_output_lines_observed":debug_lines},
                    "inventory":{"raw":"x".repeat(1024 * 1024)}
                }}}),
                &events,
                &mut pending,
            );
        }
        assert!(events.ui.stats().bytes < 1024);
        let UiEvent::Player(player) = ui.recv().unwrap() else {
            panic!()
        };
        assert_eq!(player.pid, Some(42));
        assert_eq!(player.debug_lines, 2);
        assert!(ui.try_recv().is_err());
    }

    #[test]
    fn routes_overlay_and_hud_visibility_independently() {
        let (events, ui_events, _relic_events) = server_events();
        let mut pending = BTreeMap::new();

        handle_server_message(
            serde_json::json!({"event":"command","data":{"command":"overlay","visible":false}}),
            &events,
            &mut pending,
        );
        assert!(matches!(
            ui_events.recv().unwrap(),
            UiEvent::OverlayVisible(false)
        ));

        handle_server_message(
            serde_json::json!({"event":"command","data":{"command":"hud","visible":true}}),
            &events,
            &mut pending,
        );
        assert!(matches!(
            ui_events.recv().unwrap(),
            UiEvent::HudVisible(true)
        ));
    }

    #[test]
    fn routes_asset_refresh_event() {
        let (events, ui_events, _relic_events) = server_events();
        let mut pending = BTreeMap::new();

        handle_server_message(
            serde_json::json!({"event":"asset","data":{"source":"market","image_name":"item.webp","path":"/cache/item.webp","digest":"new"}}),
            &events,
            &mut pending,
        );

        let UiEvent::AssetRefreshed(refresh) = ui_events.recv().unwrap() else {
            panic!("expected asset refresh");
        };
        assert_eq!(refresh.source, "market");
        assert_eq!(refresh.image_name, "item.webp");
        assert_eq!(refresh.digest, "new");
    }

    #[test]
    fn routes_armed_relic_capture() {
        let (events, _ui_events, triggers) = server_events();
        let mut pending = BTreeMap::new();

        handle_server_message(
            serde_json::json!({"event":"command","data":{"command":"capture","action":"arm","target":"relic_reward","directory":"/tmp/reward","timeout_ms":9000}}),
            &events,
            &mut pending,
        );

        let RelicTrigger::ArmCapture(request) = triggers.recv().unwrap() else {
            panic!("expected armed capture");
        };
        assert_eq!(request.directory, PathBuf::from("/tmp/reward"));
        assert_eq!(request.timeout, Duration::from_secs(9));
    }

    #[test]
    fn retries_request_closed_by_daemon_restart() {
        let (sender, mut receiver) = outbound_channel();
        let worker = thread::spawn(move || {
            let Outbound::RelicContext { reply, .. } = receiver.blocking_recv().unwrap() else {
                panic!("expected first relic context request");
            };
            reply.send(Err("daemon connection closed".to_owned()));

            let Outbound::RelicContext { reply, .. } = receiver.blocking_recv().unwrap() else {
                panic!("expected retried relic context request");
            };
            reply.send(Ok(serde_json::json!({"data": {"quotes": []}})));
        });

        let response = relic_context(&sender, vec!["forma-blueprint".to_owned()]).unwrap();
        assert_eq!(response["data"]["quotes"], serde_json::json!([]));
        worker.join().unwrap();
    }

    #[test]
    fn relic_recommendations_preserve_price_request() {
        let (sender, mut receiver) = outbound_channel();
        let worker = thread::spawn(move || {
            let Outbound::RelicRecommendations {
                fetch_prices,
                limit,
                reply,
                ..
            } = receiver.blocking_recv().unwrap()
            else {
                panic!("expected relic recommendations request");
            };
            assert!(fetch_prices);
            assert_eq!(limit, 32);
            reply.send(Ok(serde_json::json!({"data": {"items": []}})));
        });

        relic_recommendations(&sender, "lith".to_owned(), true).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn native_daemon_child_clears_steam_loader_environment() {
        let mut process = ProcessCommand::new("true");
        sanitize_native_child(&mut process);
        let environment: BTreeMap<_, _> = process.get_envs().collect();

        for name in [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "STEAM_RUNTIME",
            "STEAM_RUNTIME_LIBRARY_PATH",
        ] {
            assert_eq!(environment.get(std::ffi::OsStr::new(name)), Some(&None));
        }
    }
}
