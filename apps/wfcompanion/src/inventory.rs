use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runtime::{inbox, session::Session};
use retinue_warframe::game_observer::gep;
use retinue_warframe::observation::debug_output::Runtime;
use retinue_warframe::observation::{gep as sampling, mailbox::Mailbox};
use serde::Serialize;
use serde_json::{Map, Value};

const INVENTORY_MARKER: &[u8] = b"LastInventorySync";
const SCHEMA_VERSION: u32 = 2;

mod handoff;
mod observation;
mod refresh;
pub(crate) use handoff::Checkpoint;

#[derive(Serialize)]
pub(crate) struct PipelineReport {
    #[serde(flatten)]
    sampling: sampling::Report,
    decoded: inbox::Stats,
}

pub(crate) type Sender = inbox::Sender<Event>;

pub(crate) fn channel() -> (Sender, inbox::Receiver<Event>) {
    inbox::channel(
        "inventory",
        inbox::Limit {
            items: 2,
            bytes: 32 * 1024 * 1024,
        },
        inbox::Limit {
            items: 1,
            bytes: size_of::<Event>(),
        },
    )
}

#[derive(Debug)]
pub(crate) enum Event {
    Inventory {
        game_pid: u32,
        collector: &'static str,
        process_pid: u32,
        data: Value,
    },
    Native {
        game_pid: u32,
        data: Value,
    },
    Account {
        game_pid: u32,
        seed: u32,
    },
}

impl inbox::Message for Event {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Inventory { data, .. } | Self::Native { data, .. } => {
                    inbox::value_bytes(data)
                }
                Self::Account { .. } => 0,
            }
    }

    fn control(&self) -> bool {
        matches!(self, Self::Account { .. })
    }

    fn replaces(&self, queued: &Self) -> bool {
        match (self, queued) {
            (Self::Inventory { game_pid: new, .. }, Self::Inventory { game_pid: old, .. })
            | (Self::Native { game_pid: new, .. }, Self::Native { game_pid: old, .. })
            | (Self::Account { game_pid: new, .. }, Self::Account { game_pid: old, .. }) => {
                new == old
            }
            _ => false,
        }
    }
}

pub(crate) struct Bridge {
    game_pid: u32,
    sampler: sampling::Sampler,
    worker: Option<JoinHandle<Option<Checkpoint>>>,
    checkpoint: Option<Checkpoint>,
    events: Sender,
}

impl Bridge {
    pub(crate) fn start(
        runtime: &Runtime,
        session: Session,
        events: Sender,
        checkpoint: Option<&Checkpoint>,
    ) -> Result<Self, String> {
        let game_pid = runtime.game_pid();
        if session.pid() != game_pid {
            return Err("inventory game session PID mismatch".to_owned());
        }
        session.check()?;
        let stream = observation::Stream::new(&session)?;
        let mem = File::open(format!("/proc/{game_pid}/mem"))
            .map_err(|error| format!("could not open Warframe memory: {error}"))?;
        let sources = session.read(gep::Sources::discover)?;
        let (queue, item_base, body) = sources.response_offsets();
        crate::incident::info(
            "inventory.native_gep_ready",
            format!(
                "game_pid={game_pid} global=0x{:x} queue=0x{queue:x} item=0x{item_base:x} body=0x{body:x}",
                sources.manager_global(),
            ),
        );
        let sampler_session = session.clone();
        let sampler = sampling::Sampler::start(
            mem,
            sources,
            sampling::Options {
                inventory_only: true,
                account_seed: true,
                ..Default::default()
            },
            move || sampler_session.is_current(),
        )?;
        let samples = sampler.queue.clone();
        let metrics = sampler.metrics.clone();
        let stopping = sampler.stopping.clone();
        let prefix = runtime.prefix().to_owned();
        let decoded = events.clone();
        let mut decoder = Decoder::new(session, stream, prefix, events);
        if let Some(checkpoint) = checkpoint {
            decoder.restore(checkpoint.clone())?;
        }
        let worker = thread::Builder::new()
            .name("wfcompanion-inventory".into())
            .spawn(move || {
                decode(&mut decoder, samples, metrics, stopping);
                decoder.checkpoint().ok()
            })
            .map_err(|error| format!("could not start inventory decoder: {error}"))?;
        Ok(Self {
            game_pid,
            sampler,
            worker: Some(worker),
            checkpoint: None,
            events: decoded,
        })
    }

    pub(crate) fn game_pid(&self) -> u32 {
        self.game_pid
    }

    pub(crate) fn is_running(&mut self) -> bool {
        self.sampler.is_running()
            && self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
    }

    pub(crate) fn pipeline_report(&self) -> PipelineReport {
        PipelineReport {
            sampling: self.sampler.report(),
            decoded: self.events.stats(),
        }
    }

    pub(crate) fn stop(&mut self) {
        if let Err(error) = self.sampler.stop() {
            crate::incident::error("runtime.worker_panicked", error);
        }
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(checkpoint) => self.checkpoint = checkpoint,
                Err(_) => crate::incident::error("runtime.worker_panicked", "worker=inventory"),
            }
        }
    }

    pub(crate) fn take_checkpoint(&mut self) -> Option<Checkpoint> {
        self.stop();
        self.checkpoint.take()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Default, Serialize)]
struct Profile {
    player_name: Option<String>,
    player_level: Option<i64>,
    regular_credits: Option<i64>,
    premium_credits: Option<i64>,
    premium_credits_free: Option<i64>,
    fusion_points: Option<i64>,
    trades_remaining: Option<i64>,
    daily_focus: Option<i64>,
    focus_capacity: Option<i64>,
    last_region_played: Option<String>,
}

fn decode(
    decoder: &mut Decoder,
    samples: Arc<Mailbox<sampling::Sample>>,
    metrics: Arc<sampling::Metrics>,
    stopping: Arc<AtomicBool>,
) {
    let game_pid = decoder.session.pid();
    let refresh = refresh::Refresh::start(decoder.session.clone(), stopping);
    let mut next_refresh_warning = Instant::now() + Duration::from_secs(30);
    let mut reported_drops = 0;
    while decoder.session.is_current() {
        let received = samples.recv_timeout(Duration::from_millis(50));
        let dropped = samples.stats().dropped_items;
        if dropped != reported_drops {
            crate::incident::warn(
                "inventory.sampling_gap",
                format!(
                    "game_pid={game_pid} dropped={} total={dropped}; retrying retained buffers",
                    dropped - reported_drops
                ),
            );
            reported_drops = dropped;
        }
        let sample = match received {
            Ok(sample) => Some(sample),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if !decoder.session.is_current() {
            break;
        }
        if let Some(sample) = sample {
            metrics.queue_delay.record(sample.captured.elapsed());
            let started = Instant::now();
            let sequence = sample.sequence;
            if decoder.accept(sample).is_err() {
                return;
            }
            metrics.decode.record(started.elapsed());
            metrics.decoded_sequence.store(sequence, Ordering::Relaxed);
        }
        // Drain older HTTP observations before applying a later native read.
        if samples.stats().items != 0 {
            continue;
        }
        for (started, finished, collected_at, result) in refresh.receiver.try_iter() {
            match result {
                Ok(snapshot) => {
                    if decoder
                        .native(snapshot, started, finished, collected_at)
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) if Instant::now() >= next_refresh_warning => {
                    crate::incident::warn("inventory.native_refresh_failed", error);
                    next_refresh_warning = Instant::now() + Duration::from_secs(60);
                }
                Err(_) => {}
            }
        }
    }
}

struct Decoder {
    session: Session,
    prefix: PathBuf,
    player_name: Option<String>,
    seen_payloads: HashSet<u64>,
    stream: observation::Stream,
    baseline: Option<Baseline>,
    last_native: Option<retinue_warframe::game_observer::inventory::Snapshot>,
    events: Sender,
}

struct Baseline {
    sequence: u64,
    captured: Instant,
    sync: String,
}

impl Decoder {
    fn new(session: Session, stream: observation::Stream, prefix: PathBuf, events: Sender) -> Self {
        Self {
            session,
            player_name: player_name_from_log(&prefix),
            prefix,
            seen_payloads: HashSet::new(),
            stream,
            baseline: None,
            last_native: None,
            events,
        }
    }

    fn accept(&mut self, sample: sampling::Sample) -> Result<(), ()> {
        if !self.session.is_current() {
            return Err(());
        }
        let game_pid = self.session.pid();
        match sample.content {
            sampling::Content::AccountState(state) => {
                match state {
                    sampling::AccountState::Unavailable { reason } => {
                        self.session.record::<()>("account", &Err(reason.clone()));
                        crate::incident::warn("inventory.account_seed_unavailable", reason)
                    }
                    sampling::AccountState::Available => {
                        self.session.record("account", &Ok(()));
                        crate::incident::info(
                            "inventory.account_seed_ready",
                            format!("game_pid={game_pid}"),
                        );
                    }
                    sampling::AccountState::NotObserved => {}
                }
                Ok(())
            }
            sampling::Content::Account(seed) => {
                self.session.record("account", &Ok(()));
                self.events
                    .send(Event::Account { game_pid, seed })
                    .map_err(|_| ())
            }
            sampling::Content::Payload { source, bytes } => {
                match decode_new_payload(
                    &bytes,
                    game_pid,
                    &self.prefix,
                    &mut self.player_name,
                    &mut self.seen_payloads,
                ) {
                    Ok(Some((mut data, snapshot))) => {
                        self.session.record("http_inventory", &Ok(()));
                        crate::incident::info(
                            "inventory.native_payload_accepted",
                            format!(
                                "source={source} bytes={} snapshot={snapshot:016x}",
                                bytes.len()
                            ),
                        );
                        data["collected_at"] = serde_json::json!(sample.collected_at);
                        data["observation"] =
                            self.stream.stamp(None, sample.captured, sample.captured);
                        self.baseline = Some(Baseline {
                            sequence: data["observation"]["sequence"].as_u64().unwrap(),
                            captured: sample.captured,
                            sync: value_key(&data["sync"]),
                        });
                        self.last_native = None;
                        publish_inventory(data, &self.session, "native_http_buffer", &self.events)
                    }
                    Ok(None) => {
                        // A known-good duplicate verifies decoding without another publication.
                        if memchr::memmem::find(&bytes, INVENTORY_MARKER).is_some() {
                            self.session.record("http_inventory", &Ok(()));
                        }
                        Ok(())
                    }
                    Err(error) => {
                        self.session
                            .record::<()>("http_inventory", &Err(error.clone()));
                        crate::incident::warn(
                            "inventory.native_payload_rejected",
                            format!("source={source} bytes={} error={error}", bytes.len()),
                        );
                        Ok(())
                    }
                }
            }
        }
    }
    fn native(
        &mut self,
        snapshot: retinue_warframe::game_observer::inventory::Snapshot,
        started: Instant,
        finished: Instant,
        collected_at: u128,
    ) -> Result<(), ()> {
        if !self.session.is_current() {
            return Err(());
        }
        let Some(baseline) = &self.baseline else {
            return Ok(());
        };
        if started < baseline.captured {
            return Ok(());
        }
        if snapshot.sync != baseline.sync {
            return Ok(());
        }
        if self.last_native.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        let data = serde_json::json!({
            "schema": 1, "collector": "native_inventory",
            "process_pid": self.session.pid(), "collected_at": collected_at,
            "sync": snapshot.sync, "fields": snapshot.fields,
            "observation": self.stream.stamp(Some(baseline.sequence), started, finished),
        });
        if self
            .events
            .send(Event::Native {
                game_pid: self.session.pid(),
                data,
            })
            .is_err()
        {
            return Err(());
        }
        self.last_native = Some(snapshot);
        Ok(())
    }
}

fn publish_inventory(
    data: Value,
    session: &Session,
    collector: &'static str,
    events: &Sender,
) -> Result<(), ()> {
    if !session.is_current() {
        return Err(());
    }
    let game_pid = session.pid();
    events
        .send(Event::Inventory {
            game_pid,
            collector,
            process_pid: game_pid,
            data,
        })
        .map_err(|_| ())
}

fn decode_new_payload(
    payload: &[u8],
    game_pid: u32,
    prefix: &Path,
    player_name: &mut Option<String>,
    seen_payloads: &mut HashSet<u64>,
) -> Result<Option<(Value, u64)>, String> {
    if memchr::memmem::find(payload, INVENTORY_MARKER).is_none() {
        return Ok(None);
    }
    let fingerprint = hash_bytes(payload);
    if seen_payloads.contains(&fingerprint) {
        return Ok(None);
    }
    if player_name.is_none() {
        *player_name = player_name_from_log(prefix);
    }
    let data = parse_observation(
        payload,
        "native_http_buffer",
        game_pid,
        player_name.as_deref(),
    )?;
    let mut hasher = DefaultHasher::new();
    data["raw"].hash(&mut hasher);
    let snapshot = hasher.finish();
    seen_payloads.insert(fingerprint);
    Ok(Some((data, snapshot)))
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    u64::from_le_bytes(Sha256::digest(bytes)[..8].try_into().unwrap())
}

fn player_name_from_log(prefix: &Path) -> Option<String> {
    let users = prefix.join("drive_c/users");
    let steam_log = users.join("steamuser/AppData/Local/Warframe/EE.log");
    let log = if steam_log.is_file() {
        steam_log
    } else {
        fs::read_dir(users)
            .ok()?
            .flatten()
            .map(|entry| entry.path().join("AppData/Local/Warframe/EE.log"))
            .find(|path| path.is_file())?
    };
    BufReader::new(File::open(log).ok()?)
        .lines()
        .map_while(Result::ok)
        .find_map(|line| {
            let (_, login) = line.split_once("Logged in ")?;
            let name = login.split('(').next()?.trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
}

fn parse_observation(
    payload: &[u8],
    collector: &'static str,
    process_pid: u32,
    player_name: Option<&str>,
) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| format!("inventory payload is not valid JSON: {error}"))?;
    let shape = root_shape(&value);
    let raw = extract_inventory(value).ok_or_else(|| {
        format!("inventory payload contains no complete inventory object; {shape}")
    })?;
    let object = raw
        .as_object()
        .ok_or_else(|| "inventory payload root is not an object".to_owned())?;
    let sync = object
        .get("LastInventorySync")
        .cloned()
        .ok_or_else(|| "inventory payload has no LastInventorySync".to_owned())?;
    let sync_key = value_key(&sync);
    if sync_key.is_empty() {
        return Err("inventory LastInventorySync is empty".to_owned());
    }
    let mut data = serde_json::json!({
        "schema": SCHEMA_VERSION,
        "collector": collector,
        "collected_at": unix_time_millis(),
        "process_pid": process_pid,
        "sync": sync,
        "profile": profile(object, player_name),
    });
    data["raw"] = raw;
    Ok(data)
}

fn extract_inventory(value: Value) -> Option<Value> {
    match value {
        Value::Object(mut object) => {
            if is_inventory_object(&object) {
                return Some(Value::Object(object));
            }
            if let Some(inventory) = object.remove("InventoryJSON")
                && let Some(found) = extract_inventory(inventory)
            {
                return Some(found);
            }
            object.into_values().find_map(extract_inventory)
        }
        Value::Array(values) => values.into_iter().find_map(extract_inventory),
        Value::String(encoded) if encoded.contains("LastInventorySync") => {
            serde_json::from_str(&encoded)
                .ok()
                .and_then(extract_inventory)
        }
        _ => None,
    }
}

fn is_inventory_object(object: &Map<String, Value>) -> bool {
    object.contains_key("LastInventorySync")
        && [
            "Suits",
            "LongGuns",
            "Pistols",
            "Melee",
            "SpaceSuits",
            "MiscItems",
            "XPInfo",
            "Recipes",
        ]
        .iter()
        .any(|key| object.contains_key(*key))
}

fn root_shape(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            keys.truncate(16);
            format!("root=object keys={}", keys.join(","))
        }
        Value::Array(values) => format!("root=array length={}", values.len()),
        Value::String(_) => "root=string".to_owned(),
        Value::Null => "root=null".to_owned(),
        Value::Bool(_) => "root=boolean".to_owned(),
        Value::Number(_) => "root=number".to_owned(),
    }
}

fn profile(object: &Map<String, Value>, player_name: Option<&str>) -> Profile {
    Profile {
        player_name: player_name.map(str::to_owned),
        player_level: integer(object, "PlayerLevel"),
        regular_credits: integer(object, "RegularCredits"),
        premium_credits: integer(object, "PremiumCredits"),
        premium_credits_free: integer(object, "PremiumCreditsFree"),
        fusion_points: integer(object, "FusionPoints"),
        trades_remaining: integer(object, "TradesRemaining"),
        daily_focus: integer(object, "DailyFocus"),
        focus_capacity: integer(object, "FocusCapacity"),
        last_region_played: text(object, "LastRegionPlayed"),
    }
}

fn integer(object: &Map<String, Value>, key: &str) -> Option<i64> {
    object.get(key).and_then(Value::as_i64)
}

fn text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn value_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Object(object) => ["$oid", "oid", "$date", "$numberLong"]
            .iter()
            .find_map(|key| object.get(*key).map(value_key))
            .unwrap_or_else(|| serde_json::to_string(value).unwrap_or_default()),
        Value::Null => String::new(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "LastInventorySync":{"$oid":"abcdef"},
        "PlayerLevel":18,
        "RegularCredits":12345,
        "Suits":[{"ItemType":"/Lotus/Powersuits/Excalibur","ItemId":{"$oid":"suit1"},"XP":9000}],
        "MiscItems":[{"ItemType":"/Lotus/Types/Items/MiscItems/ArgonCrystal","ItemCount":3}],
        "XPInfo":[{"ItemType":"/Lotus/Weapons/Tenno/Rifle/Braton","XP":450000}],
        "PendingRecipes":[{"ItemType":"/Lotus/Weapons/Tenno/Rifle/Braton","ItemId":{"$oid":"recipe1"}}],
        "UnknownFutureField":{"kept":true}
    }"#;

    #[test]
    fn parses_inventory_without_dropping_raw_fields() {
        let value = parse_observation(
            SAMPLE.as_bytes(),
            "native_http_queue",
            42,
            Some("TestTenno"),
        )
        .unwrap();
        assert_eq!(value["sync"]["$oid"], "abcdef");
        assert_eq!(value["schema"], 2);
        assert_eq!(value["collector"], "native_http_queue");
        assert_eq!(value["process_pid"], 42);
        assert_eq!(value["profile"]["player_name"], "TestTenno");
        assert_eq!(value["profile"]["player_level"], 18);
        assert!(value.get("index").is_none());
        assert_eq!(value["raw"]["Suits"][0]["XP"], 9000);
        assert_eq!(value["raw"]["MiscItems"][0]["ItemCount"], 3);
        assert_eq!(value["raw"]["XPInfo"][0]["XP"], 450000);
        assert_eq!(value["raw"]["UnknownFutureField"]["kept"], true);
    }

    #[test]
    fn unwraps_inventory_json_envelope() {
        let wrapped = serde_json::json!({"InventoryJSON": SAMPLE}).to_string();
        let value = parse_observation(wrapped.as_bytes(), "native_http_queue", 7, None).unwrap();
        assert_eq!(value["sync"]["$oid"], "abcdef");
        assert_eq!(value["process_pid"], 7);
    }

    #[test]
    fn extracts_nested_inventory_object() {
        let inventory: Value = serde_json::from_str(SAMPLE).unwrap();
        let wrapped = serde_json::json!({"response": {"data": inventory}}).to_string();
        let value = parse_observation(wrapped.as_bytes(), "native_http_buffer", 7, None).unwrap();
        assert_eq!(value["sync"]["$oid"], "abcdef");
        assert_eq!(value["raw"]["Suits"][0]["XP"], 9000);
    }

    #[test]
    fn extracts_nested_encoded_inventory() {
        let wrapped = serde_json::json!({"response": {"body": SAMPLE}}).to_string();
        let value = parse_observation(wrapped.as_bytes(), "native_http_buffer", 7, None).unwrap();
        assert_eq!(value["sync"]["$oid"], "abcdef");
        assert_eq!(value["raw"]["XPInfo"][0]["XP"], 450000);
    }

    #[test]
    fn rejects_marker_without_inventory_collections() {
        let error = parse_observation(
            br#"{"metadata":{"LastInventorySync":"not-an-inventory"}}"#,
            "native_http_buffer",
            7,
            None,
        )
        .unwrap_err();
        assert!(error.contains("no complete inventory object"));
    }

    #[test]
    fn deduplicates_payloads_not_inventory_sync_markers() {
        let changed = SAMPLE.replace("\"ItemCount\":3", "\"ItemCount\":4");
        let mut seen = HashSet::new();
        let mut player_name = None;
        let prefix = Path::new("/nonexistent");
        let (sender, receiver) = channel();
        let (first, _) =
            decode_new_payload(SAMPLE.as_bytes(), 42, prefix, &mut player_name, &mut seen)
                .unwrap()
                .unwrap();
        assert!(
            decode_new_payload(SAMPLE.as_bytes(), 42, prefix, &mut player_name, &mut seen,)
                .unwrap()
                .is_none()
        );
        let (second, _) =
            decode_new_payload(changed.as_bytes(), 42, prefix, &mut player_name, &mut seen)
                .unwrap()
                .unwrap();
        let session = Session::for_test(42);
        publish_inventory(first, &session, "test", &sender).unwrap();
        let Event::Inventory { data: first, .. } = receiver.recv().unwrap() else {
            panic!("expected first inventory event");
        };
        publish_inventory(second, &session, "test", &sender).unwrap();
        let Event::Inventory { data: second, .. } = receiver.recv().unwrap() else {
            panic!("expected second inventory event");
        };
        assert_eq!(first["raw"]["MiscItems"][0]["ItemCount"], 3);
        assert_eq!(second["raw"]["MiscItems"][0]["ItemCount"], 4);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn fingerprints_inventory_independent_of_response_envelope() {
        let nested = serde_json::json!({"response": {"body": SAMPLE}}).to_string();
        let mut direct_seen = HashSet::new();
        let mut nested_seen = HashSet::new();
        let mut player_name = None;
        let prefix = Path::new("/nonexistent");
        let (_, direct_fingerprint) = decode_new_payload(
            SAMPLE.as_bytes(),
            42,
            prefix,
            &mut player_name,
            &mut direct_seen,
        )
        .unwrap()
        .unwrap();
        let (_, nested_fingerprint) = decode_new_payload(
            nested.as_bytes(),
            42,
            prefix,
            &mut player_name,
            &mut nested_seen,
        )
        .unwrap()
        .unwrap();
        assert_eq!(direct_fingerprint, nested_fingerprint);
    }

    #[test]
    fn retired_session_cannot_publish_decoded_inventory() {
        let session = Session::for_test(u32::MAX);
        let (sender, receiver) = channel();
        let data = parse_observation(SAMPLE.as_bytes(), "test", session.pid(), None).unwrap();
        assert!(session.check().is_err());
        assert!(publish_inventory(data, &session, "test", &sender).is_err());
        assert!(receiver.try_recv().is_err());
    }

    fn sample(sequence: u64, count: u32) -> sampling::Sample {
        sampling::Sample {
            sequence,
            captured: Instant::now() - Duration::from_secs(1),
            collected_at: 1000,
            content: sampling::Content::Payload {
                source: "direct",
                bytes: SAMPLE
                    .replace("\"ItemCount\":3", &format!("\"ItemCount\":{count}"))
                    .into_bytes(),
            },
        }
    }

    #[test]
    fn deduplicated_reads_still_resolve_health_failures() {
        let (sender, receiver) = channel();
        let mut decoder = decoder(sender);
        decoder.accept(sample(1, 3)).unwrap();
        receiver.recv().unwrap();
        let mut bad = sample(2, 3);
        bad.content = sampling::Content::Payload {
            source: "direct",
            bytes: br#"{"LastInventorySync":"invalid"}"#.to_vec(),
        };
        decoder.accept(bad).unwrap();
        assert_eq!(
            decoder.session.health().1["http_inventory"]["state"],
            "fail"
        );
        let mut unrelated = sample(3, 3);
        unrelated.content = sampling::Content::Payload {
            source: "direct",
            bytes: b"{}".to_vec(),
        };
        decoder.accept(unrelated).unwrap();
        assert_eq!(
            decoder.session.health().1["http_inventory"]["state"],
            "fail"
        );
        decoder.accept(sample(4, 3)).unwrap();
        assert_eq!(
            decoder.session.health().1["http_inventory"]["state"],
            "pass"
        );
        assert!(receiver.try_recv().is_err());

        for (sequence, state) in [
            (
                5,
                sampling::AccountState::Unavailable {
                    reason: "read failed".into(),
                },
            ),
            (6, sampling::AccountState::Available),
        ] {
            let mut account = sample(sequence, 3);
            account.content = sampling::Content::AccountState(state);
            decoder.accept(account).unwrap();
        }
        let health = decoder.session.health().1;
        assert_eq!(health["account"]["state"], "pass");
        assert_eq!(health["account"]["last_failure"]["error"], "read failed");
    }

    #[test]
    fn delayed_decode_preserves_capture_time_and_accepts_newer_native_state() {
        let (sender, receiver) = channel();
        let mut decoder = decoder(sender);
        let sample = sample(1, 3);
        let native_started = sample.captured + Duration::from_millis(1);
        decoder.accept(sample).unwrap();
        let Event::Inventory { data, .. } = receiver.recv().unwrap() else {
            panic!("expected inventory");
        };
        assert_eq!(data["collected_at"], 1000);
        let mut fields = data["raw"].as_object().unwrap().clone();
        fields.get_mut("MiscItems").unwrap()[0]["ItemCount"] = 4.into();
        decoder
            .native(
                retinue_warframe::game_observer::inventory::Snapshot {
                    sync: "abcdef".into(),
                    fields,
                },
                native_started,
                native_started + Duration::from_millis(1),
                1001,
            )
            .unwrap();
        let Event::Native { data: native, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert_eq!(native["fields"]["MiscItems"][0]["ItemCount"], 4);
        assert!(native.get("raw").is_none());
        assert_eq!(
            native["observation"]["baseline"],
            data["observation"]["sequence"]
        );
        assert_eq!(native["collected_at"], 1001);
    }

    fn decoder(sender: Sender) -> Decoder {
        let session = Session::for_test(42);
        let stream = observation::Stream::new(&session).unwrap();
        Decoder::new(session, stream, "/nonexistent".into(), sender)
    }

    fn native_snapshot(count: u32) -> retinue_warframe::game_observer::inventory::Snapshot {
        retinue_warframe::game_observer::inventory::Snapshot {
            sync: "abcdef".into(),
            fields: serde_json::json!({"MiscItems": [{"ItemType":"resource", "ItemCount":count}],
                                      "Recipes":[], "PendingRecipes":[]})
            .as_object()
            .unwrap()
            .clone(),
        }
    }

    #[test]
    fn reload_preserves_baseline_order_and_payload_deduplication() {
        let mut sessions = crate::runtime::session::Sessions::default();
        sessions.update(Some(std::process::id())).unwrap();
        let session = sessions.current().unwrap().clone();
        let (events, received) = channel();
        let mut before = Decoder::new(
            session.clone(),
            observation::Stream::new(&session).unwrap(),
            "/nonexistent".into(),
            events.clone(),
        );
        let mut http = sample(1, 3);
        http.captured = Instant::now();
        before.accept(http).unwrap();
        let Event::Inventory { data: http, .. } = received.recv().unwrap() else {
            panic!()
        };
        let now = Instant::now();
        before.native(native_snapshot(4), now, now, 1002).unwrap();
        received.recv().unwrap();
        let saved = serde_json::to_vec(&before.checkpoint().unwrap()).unwrap();
        let mut after = Decoder::new(
            session.clone(),
            observation::Stream::new(&session).unwrap(),
            "/nonexistent".into(),
            events,
        );
        after
            .restore(serde_json::from_slice(&saved).unwrap())
            .unwrap();
        after.accept(sample(2, 3)).unwrap();
        assert!(
            received.try_recv().is_err(),
            "old HTTP payload must not overwrite native changes"
        );
        let now = Instant::now();
        after.native(native_snapshot(5), now, now, 1003).unwrap();
        let Event::Native { data, .. } = received.recv().unwrap() else {
            panic!()
        };
        for key in [
            "boot_id",
            "stream",
            "game_started",
            "generation",
            "baseline",
        ] {
            assert_eq!(data["observation"][key], http["observation"][key], "{key}");
        }
        assert_eq!(data["observation"]["sequence"], 3);
        assert!(
            data["observation"]["started_ns"].as_u64().unwrap()
                >= http["observation"]["finished_ns"].as_u64().unwrap()
        );
        assert_eq!(data["fields"]["MiscItems"][0]["ItemCount"], 5);
        let mut wrong: Value = serde_json::from_slice(&saved).unwrap();
        wrong["process"]["started"] = 0.into();
        assert!(
            after
                .restore(serde_json::from_value(wrong).unwrap())
                .is_err()
        );
    }

    #[test]
    fn stale_native_read_does_not_suppress_valid_retry_or_new_baseline() {
        let (sender, receiver) = channel();
        let mut decoder = decoder(sender);
        let first = sample(1, 3);
        let captured = first.captured;
        decoder.accept(first).unwrap();
        receiver.recv().unwrap();
        let before = captured - Duration::from_millis(1);
        decoder
            .native(native_snapshot(4), before, captured, 1001)
            .unwrap();
        assert!(receiver.try_recv().is_err());
        decoder
            .native(native_snapshot(4), captured, captured, 1002)
            .unwrap();
        let Event::Native { data, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert_eq!(data["observation"]["baseline"], 1);
        decoder
            .native(native_snapshot(4), captured, captured, 1003)
            .unwrap();
        assert!(receiver.try_recv().is_err());
        let second = sample(2, 5);
        let captured = second.captured;
        decoder.accept(second).unwrap();
        let Event::Inventory { data: full, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        decoder
            .native(native_snapshot(4), captured, captured, 1004)
            .unwrap();
        let Event::Native { data, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert_eq!(
            data["observation"]["baseline"],
            full["observation"]["sequence"]
        );
        assert_eq!(data["observation"]["sequence"], 4);
    }

    #[test]
    fn slow_observer_keeps_both_scopes_without_full_native_copy() {
        let (sender, receiver) = channel();
        let mut decoder = decoder(sender.clone());
        decoder.accept(sample(1, 3)).unwrap();
        for count in 0..100 {
            let captured = Instant::now();
            decoder
                .native(native_snapshot(count), captured, captured, 1001)
                .unwrap();
        }
        assert_eq!(sender.stats().items, 2);
        assert_eq!(sender.stats().coalesced, 99);
        assert!(matches!(receiver.recv().unwrap(), Event::Inventory { .. }));
        let Event::Native { data, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert_eq!(data["fields"]["MiscItems"][0]["ItemCount"], 99);
        assert!(data.get("raw").is_none());
    }

    #[test]
    fn retained_replay_recovers_unseen_payload_but_never_reapplies_accepted_old_state() {
        let (sender, receiver) = channel();
        let mut decoder = decoder(sender);
        decoder.accept(sample(1, 3)).unwrap();
        decoder.accept(sample(2, 4)).unwrap();
        let queue = Mailbox::new(1, 8192);
        assert!(matches!(queue.send(sample(3, 5), 1024), Ok(0)));
        assert!(matches!(queue.send(sample(4, 3), 1024), Ok(1)));
        decoder
            .accept(queue.recv_timeout(Duration::ZERO).unwrap())
            .unwrap();
        assert_eq!(decoder.baseline.as_ref().unwrap().sequence, 2);
        decoder.accept(sample(5, 5)).unwrap();
        let counts: Vec<_> = receiver
            .drain(2)
            .into_iter()
            .map(|event| match event {
                Event::Inventory { data, .. } => {
                    data["raw"]["MiscItems"][0]["ItemCount"].as_u64().unwrap()
                }
                _ => panic!("expected inventory"),
            })
            .collect();
        assert_eq!(counts, [5]);
    }

    #[test]
    fn slow_observer_keeps_latest_full_inventory_and_separate_account_seed() {
        let (sender, receiver) = channel();
        let session = Session::for_test(42);
        sender
            .send(Event::Account {
                game_pid: 42,
                seed: 123,
            })
            .unwrap();
        for count in 0..1000 {
            let data = serde_json::json!({ "raw": {"MiscItems": [{"ItemCount": count}], "NewField": count} });
            publish_inventory(data, &session, "test", &sender).unwrap();
        }
        assert_eq!(sender.stats().items, 2);
        assert_eq!(sender.stats().coalesced, 999);
        drop(sender);
        assert!(matches!(
            receiver.recv().unwrap(),
            Event::Account { seed: 123, .. }
        ));
        let Event::Inventory { data, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert_eq!(data["raw"]["MiscItems"][0]["ItemCount"], 999);
        assert_eq!(data["raw"]["NewField"], 999);
        assert!(receiver.recv().is_err());
    }

    #[test]
    fn rejects_payload_without_sync_marker() {
        assert!(
            parse_observation(br#"{"MiscItems":[]}"#, "native_http_queue", 1, None)
                .unwrap_err()
                .contains("no complete inventory object")
        );
    }

    #[test]
    fn reads_player_name_from_proton_log() {
        let prefix = std::env::temp_dir().join(format!(
            "wfcompanion-inventory-{}-{}",
            std::process::id(),
            unix_time_millis()
        ));
        let log = prefix.join("drive_c/users/steamuser/AppData/Local/Warframe/EE.log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, "0.0 Sys [Info]: Logged in TestTenno (abcdef)\n").unwrap();
        assert_eq!(player_name_from_log(&prefix).as_deref(), Some("TestTenno"));
        fs::remove_dir_all(prefix).unwrap();
    }
}
