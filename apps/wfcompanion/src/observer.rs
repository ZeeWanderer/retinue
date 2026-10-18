use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use retinue_warframe::game_observer::{self, DebugOutputEvent, GameState};
use retinue_warframe::observation::debug_output::{
    self, Bridge as DebugBridge, Event as DebugEvent, Runtime as DebugRuntime,
};
use serde_json::json;

use crate::daemon::{Outbound, OutboundSender};
use crate::game_metadata::{Bridge as MetadataBridge, Event as MetadataEvent};
use crate::incident;
use crate::inventory::{Bridge as InventoryBridge, Event as InventoryEvent, PipelineReport};
use crate::relic::Trigger as RelicTrigger;
use crate::runtime::diagnostics;
use crate::runtime::session::{Session, Sessions};

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const DEBUG_RESTART_DELAY: Duration = Duration::from_secs(10);
const EVENT_INTERVAL: Duration = Duration::from_millis(200);
const UI_CONSOLE_OPEN_GUARD: Duration = Duration::from_secs(1);

#[derive(Default)]
struct CollectorStatus {
    game_pid: Option<u32>,
    session_generation: Option<u64>,
    session_error: Option<String>,
    debug_lines: u64,
    inventory_updates: u64,
    account_updates: u64,
    metadata_updates: u64,
    debug_output_active: bool,
    inventory_active: bool,
    metadata_active: bool,
    debug_output_error: Option<String>,
    debug_output_queue: retinue_warframe::observation::mailbox::Stats,
    inventory_error: Option<String>,
    inventory_pipeline: Option<PipelineReport>,
    metadata_error: Option<String>,
    inventory_received_at: Option<u128>,
    metadata_received_at: Option<u128>,
    metadata_source: Option<&'static str>,
}

struct DebugCollector {
    bridge: Option<DebugBridge>,
    next_attempt: Instant,
    last_console_open: Option<Instant>,
}

impl Default for DebugCollector {
    fn default() -> Self {
        Self {
            bridge: None,
            next_attempt: Instant::now(),
            last_console_open: None,
        }
    }
}

pub(crate) fn spawn(
    outbound: OutboundSender,
    relic: crate::relic::Sender,
    requests: diagnostics::Receiver,
    stopping: Arc<AtomicBool>,
    mut checkpoint: Option<crate::inventory::Checkpoint>,
) -> thread::JoinHandle<Option<crate::inventory::Checkpoint>> {
    thread::spawn(move || {
        let debug_rx = debug_output::inbox();
        let (inventory_tx, inventory_rx) = crate::inventory::channel();
        let (metadata_tx, metadata_rx) = crate::game_metadata::channel();
        let mut previous: Option<GameState> = None;
        let mut sessions = Sessions::default();
        let mut debug = DebugCollector::default();
        let mut inventory_bridge: Option<InventoryBridge> = None;
        let mut metadata_bridge: Option<MetadataBridge> = None;
        let mut ui_discovery: Option<crate::runtime::Worker> = None;
        let mut status = CollectorStatus::default();
        let mut health_revision = 0;
        let mut diagnostics = diagnostics::Watches::new(requests);
        let mut next_scan = Instant::now();
        let mut next_inventory_attempt = Instant::now();
        publish_collector(&outbound, &status);

        while !stopping.load(Ordering::Relaxed) {
            if let Some(bridge) = &inventory_bridge {
                status.inventory_pipeline = Some(bridge.pipeline_report());
            }
            if Instant::now() >= next_scan {
                let current = game_observer::find_warframe();
                if previous.as_ref() != Some(&current) {
                    let data = serde_json::to_value(&current).unwrap_or_else(|_| json!({}));
                    let _ = outbound.send(Outbound::Publish {
                        dataset: "player",
                        source: "game",
                        data,
                    });
                    previous = Some(current);
                }

                let runtime = previous
                    .as_ref()
                    .and_then(GameState::attach)
                    .and_then(|attach| {
                        DebugRuntime::discover(
                            attach.pid(),
                            attach.process_dir(),
                            attach.environment(),
                            attach.compat_data(),
                        )
                    });
                let game_pid = runtime.as_ref().map(DebugRuntime::game_pid);
                let had_session = sessions.current().is_some();
                let session_changed = match sessions.update(game_pid) {
                    Ok(changed) => {
                        status.session_error = None;
                        changed
                    }
                    Err(error) => {
                        if status.session_error.as_ref() != Some(&error) {
                            incident::warn("observer.session_unavailable", &error);
                            status.session_error = Some(error);
                            publish_collector(&outbound, &status);
                        }
                        had_session
                    }
                };
                if session_changed {
                    health_revision = 0;
                    debug = DebugCollector::default();
                    drop(ui_discovery.take());
                    drop(inventory_bridge.take());
                    drop(metadata_bridge.take());
                    while debug_rx.recv_timeout(Duration::ZERO).is_ok() {}
                    while inventory_rx.try_recv().is_ok() {}
                    while metadata_rx.try_recv().is_ok() {}
                    if had_session {
                        let _ = relic.send(RelicTrigger::GameStopped);
                    }
                    status = CollectorStatus {
                        game_pid: sessions.current().map(Session::pid),
                        debug_output_queue: debug_rx.stats(),
                        session_generation: sessions.current().map(Session::generation),
                        session_error: status.session_error.take(),
                        ..CollectorStatus::default()
                    };
                    next_inventory_attempt = Instant::now();
                    publish_collector(&outbound, &status);
                    if let Some(session) = sessions.current() {
                        match session.prepare_ui() {
                            Ok(worker) => ui_discovery = Some(worker),
                            Err(error) => incident::warn("observer.ui_bindings_unavailable", error),
                        }
                    }
                }
                let bridge_is_current = match (&mut debug.bridge, runtime.as_ref()) {
                    (Some(open), Some(runtime)) => {
                        open.game_pid() == runtime.game_pid() && open.is_running()
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !bridge_is_current && debug.bridge.take().is_some() {
                    status.debug_output_active = false;
                    publish_collector(&outbound, &status);
                }
                if debug.bridge.is_none()
                    && let Some(runtime) = runtime.as_ref()
                    && sessions.current().is_some()
                    && Instant::now() >= debug.next_attempt
                {
                    match DebugBridge::start(runtime, debug_rx.clone()) {
                        Ok(open) => {
                            incident::info(
                                "observer.debug_output_started",
                                format!("game_pid={}", runtime.game_pid()),
                            );
                            debug.bridge = Some(open);
                            status.debug_output_error = None;
                            status.debug_output_active = true;
                            publish_collector(&outbound, &status);
                        }
                        Err(error) => {
                            if status.debug_output_error.as_deref() != Some(&error) {
                                incident::warn("observer.debug_output_failed", &error);
                                eprintln!("wfcompanion: {error}");
                                status.debug_output_error = Some(error);
                                publish_collector(&outbound, &status);
                            }
                            debug.next_attempt = Instant::now() + DEBUG_RESTART_DELAY;
                        }
                    }
                }

                let inventory_is_current = match (&mut inventory_bridge, runtime.as_ref()) {
                    (Some(open), Some(runtime)) => {
                        open.game_pid() == runtime.game_pid() && open.is_running()
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !inventory_is_current && inventory_bridge.take().is_some() {
                    status.inventory_active = false;
                    publish_collector(&outbound, &status);
                }
                if inventory_bridge.is_none()
                    && let Some(runtime) = runtime.as_ref()
                    && let Some(session) = sessions.current()
                    && Instant::now() >= next_inventory_attempt
                {
                    if checkpoint
                        .as_ref()
                        .is_some_and(|saved| !saved.matches(session))
                    {
                        checkpoint = None;
                        incident::info("inventory.reload_discarded", "game process changed");
                    }
                    match InventoryBridge::start(
                        runtime,
                        session.clone(),
                        inventory_tx.clone(),
                        checkpoint.as_ref(),
                    ) {
                        Ok(open) => {
                            checkpoint = None;
                            incident::info(
                                "observer.inventory_started",
                                format!("game_pid={}", runtime.game_pid()),
                            );
                            status.inventory_pipeline = Some(open.pipeline_report());
                            inventory_bridge = Some(open);
                            status.inventory_error = None;
                            status.inventory_active = true;
                            publish_collector(&outbound, &status);
                        }
                        Err(error) => {
                            if status.inventory_error.as_deref() != Some(&error) {
                                incident::warn("observer.inventory_failed", &error);
                                eprintln!("wfcompanion: {error}");
                                status.inventory_error = Some(error);
                                publish_collector(&outbound, &status);
                            }
                            next_inventory_attempt = Instant::now() + DEBUG_RESTART_DELAY;
                        }
                    }
                }

                let metadata_is_current = match (&metadata_bridge, runtime.as_ref()) {
                    (Some(open), Some(runtime)) => {
                        open.game_pid() == runtime.game_pid() && open.is_running()
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !metadata_is_current && metadata_bridge.take().is_some() {
                    status.metadata_active = false;
                    publish_collector(&outbound, &status);
                }
                if metadata_bridge.is_none()
                    && let Some(runtime) = runtime.as_ref()
                    && let Some(session) = sessions.current()
                {
                    match MetadataBridge::start(
                        session.clone(),
                        outbound.clone(),
                        metadata_tx.clone(),
                    ) {
                        Ok(open) => {
                            incident::info(
                                "observer.game_metadata_started",
                                format!("game_pid={}", runtime.game_pid()),
                            );
                            metadata_bridge = Some(open);
                            status.metadata_active = true;
                            publish_collector(&outbound, &status);
                        }
                        Err(error) => {
                            incident::warn("observer.game_metadata_failed", &error);
                            eprintln!("wfcompanion: {error}");
                            status.metadata_error = Some(error);
                            publish_collector(&outbound, &status);
                        }
                    }
                }
                next_scan = Instant::now() + SCAN_INTERVAL;
            }

            for event in inventory_rx.drain(2) {
                handle_inventory_event(event, &mut inventory_bridge, &outbound, &mut status);
            }
            for event in metadata_rx.drain(1) {
                handle_metadata_event(event, &metadata_bridge, &outbound, &mut status);
            }
            let queue = debug_rx.stats();
            if queue.dropped_items != status.debug_output_queue.dropped_items {
                incident::warn(
                    "observer.debug_output_gap",
                    format!("dropped={}", queue.dropped_items),
                );
                let _ = relic.send(RelicTrigger::IntakeGap);
            }
            status.debug_output_queue = queue;
            if let Some(session) = sessions.current() {
                let revision = session.health_revision();
                if revision != health_revision {
                    let (revision, checks) = session.health();
                    let mut context = json!({});
                    session.health_context(&mut context);
                    context["session"] = json!(format!(
                        "{}:{}:{}",
                        std::process::id(),
                        context["game_started"],
                        status.session_generation.unwrap_or_default()
                    ));
                    if outbound
                        .send(Outbound::Publish {
                            dataset: "checks",
                            source: "adapters",
                            data: json!({"context": context, "checks": checks}),
                        })
                        .is_ok()
                    {
                        health_revision = revision;
                    }
                }
            }
            diagnostics.tick(|| {
                let mut data = collector_snapshot(&status);
                if let Some(session) = sessions.current() {
                    data["adapters"] = session.health().1;
                    session.health_context(&mut data);
                }
                data["relic_inbox"] = json!(relic.stats());
                data
            });

            let wait = EVENT_INTERVAL.min(next_scan.saturating_duration_since(Instant::now()));
            if let Ok(event) = debug_rx.recv_timeout(wait) {
                handle_debug_event(
                    event,
                    &mut debug,
                    &relic,
                    &outbound,
                    &mut status,
                    sessions.current(),
                );
            }
        }
        diagnostics.shutdown();
        // Release DBWIN before waiting for the memory collectors to finish.
        drop(debug);
        drop(ui_discovery.take());
        if let Some(bridge) = &mut inventory_bridge {
            bridge.stop();
            status.inventory_pipeline = Some(bridge.pipeline_report());
        }
        if let Some(bridge) = &mut metadata_bridge {
            bridge.stop();
        }
        while let Ok(event) = inventory_rx.try_recv() {
            handle_inventory_event(event, &mut inventory_bridge, &outbound, &mut status);
        }
        while let Ok(event) = metadata_rx.try_recv() {
            handle_metadata_event(event, &metadata_bridge, &outbound, &mut status);
        }
        status.debug_output_active = false;
        status.inventory_active = false;
        status.metadata_active = false;
        publish_collector(&outbound, &status);
        inventory_bridge
            .as_mut()
            .and_then(InventoryBridge::take_checkpoint)
            .or(checkpoint)
    })
}

fn handle_metadata_event(
    event: MetadataEvent,
    bridge: &Option<MetadataBridge>,
    outbound: &OutboundSender,
    status: &mut CollectorStatus,
) {
    match event {
        MetadataEvent::Captured {
            game_pid,
            data,
            cached,
        } if bridge
            .as_ref()
            .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            status.metadata_updates += 1;
            status.metadata_received_at = Some(unix_time_millis());
            status.metadata_source = Some(if cached { "cache" } else { "memory" });
            status.metadata_error = None;
            incident::info(
                "observer.game_metadata_received",
                format!(
                    "game_pid={game_pid} source={}",
                    if cached { "cache" } else { "memory" }
                ),
            );
            let _ = outbound.send(Outbound::Publish {
                dataset: "game_metadata",
                source: "warframe",
                data,
            });
            publish_collector(outbound, status);
        }
        MetadataEvent::Unavailable { game_pid, reason }
            if bridge
                .as_ref()
                .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            incident::warn("observer.game_metadata_unavailable", &reason);
            status.metadata_error = Some(reason.clone());
            if let Some(data) = unsupported_game_metadata(&reason) {
                status.metadata_updates += 1;
                let _ = outbound.send(Outbound::Publish {
                    dataset: "game_metadata",
                    source: "warframe",
                    data,
                });
            }
            publish_collector(outbound, status);
        }
        _ => {}
    }
}

fn unsupported_game_metadata(reason: &str) -> Option<serde_json::Value> {
    let sha256 = reason
        .strip_prefix("unsupported Warframe executable ")?
        .split_whitespace()
        .next()?;
    (sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| {
        json!({
            "schema": 2,
            "executable": {"sha256": sha256},
            "unavailable": {"reason": "unsupported_executable"},
        })
    })
}

fn handle_debug_event(
    event: DebugEvent,
    debug: &mut DebugCollector,
    relic: &crate::relic::Sender,
    outbound: &OutboundSender,
    status: &mut CollectorStatus,
    session: Option<&Session>,
) {
    match event {
        DebugEvent::Record {
            game_pid,
            sender_pid,
            message,
            observed_at,
            observed_at_unix_ms,
        } if debug
            .bridge
            .as_ref()
            .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            status.debug_lines += 1;
            let observation = game_observer::classify_debug_output(&message);
            match observation {
                Some(DebugOutputEvent::RelicRewards) => {
                    incident::info(
                        "observer.relic_debug_output",
                        format!("event=rewards game_pid={game_pid} windows_pid={sender_pid}"),
                    );
                    publish_collector(outbound, status);
                }
                Some(DebugOutputEvent::RelicSuggestions) => {
                    incident::info(
                        "observer.relic_debug_output",
                        format!("event=suggestions game_pid={game_pid} windows_pid={sender_pid}"),
                    );
                }
                _ => {}
            }
            if let Some(session) = session.filter(|session| session.pid() == game_pid) {
                handle_relic_observation(
                    observation,
                    session,
                    relic,
                    &mut debug.last_console_open,
                    observed_at,
                    observed_at_unix_ms,
                );
            }
        }
        DebugEvent::Stopped { game_pid, reason }
            if debug
                .bridge
                .as_ref()
                .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            incident::warn("observer.debug_output_stopped", &reason);
            status.debug_output_error = Some(reason.clone());
            debug.bridge.take();
            debug.next_attempt = Instant::now() + DEBUG_RESTART_DELAY;
            status.debug_output_active = false;
            publish_collector(outbound, status);
        }
        _ => {}
    }
}

fn handle_inventory_event(
    event: InventoryEvent,
    bridge: &mut Option<InventoryBridge>,
    outbound: &OutboundSender,
    status: &mut CollectorStatus,
) {
    match event {
        InventoryEvent::Inventory {
            game_pid,
            collector,
            process_pid,
            data,
        } if bridge
            .as_ref()
            .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            status.inventory_updates += 1;
            status.inventory_received_at = Some(unix_time_millis());
            status.inventory_error = None;
            incident::info(
                "observer.inventory_received",
                format!("game_pid={game_pid} collector={collector} process_pid={process_pid}"),
            );
            let _ = outbound.send(Outbound::Publish {
                dataset: "player",
                source: "inventory_http",
                data,
            });
            publish_collector(outbound, status);
        }
        InventoryEvent::Native { game_pid, data }
            if bridge
                .as_ref()
                .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            status.inventory_updates += 1;
            status.inventory_received_at = Some(unix_time_millis());
            status.inventory_error = None;
            let _ = outbound.send(Outbound::Publish {
                dataset: "player",
                source: "inventory_native",
                data,
            });
            publish_collector(outbound, status);
        }
        InventoryEvent::Account { game_pid, seed }
            if bridge
                .as_ref()
                .is_some_and(|open| open.game_pid() == game_pid) =>
        {
            status.account_updates += 1;
            incident::info(
                "observer.account_seed_received",
                format!("game_pid={game_pid}"),
            );
            let _ = outbound.send(Outbound::Publish {
                dataset: "player",
                source: "account",
                data: json!({
                    "schema": 1,
                    "archimedea_seed": seed,
                    "collected_at": unix_time_millis(),
                }),
            });
            publish_collector(outbound, status);
        }
        _ => {}
    }
}

fn publish_collector(outbound: &OutboundSender, status: &CollectorStatus) {
    let _ = outbound.send(Outbound::Publish {
        dataset: "player",
        source: "collector",
        data: collector_snapshot(status),
    });
}

fn collector_snapshot(status: &CollectorStatus) -> serde_json::Value {
    json!({
            "companion_pid": std::process::id(),
            "version": env!("WFCLI_VERSION"),
            "game_pid": status.game_pid,
            "session_generation": status.session_generation,
            "session_error": status.session_error,
            "incident_log": incident::log_path(),
            "debug_output_lines_observed": status.debug_lines,
            "inventory_updates_observed": status.inventory_updates,
            "account_updates_observed": status.account_updates,
            "game_metadata_updates_observed": status.metadata_updates,
            "debug_output_active": status.debug_output_active,
            "inventory_active": status.inventory_active,
            "game_metadata_active": status.metadata_active,
            "debug_output_error": status.debug_output_error,
            "debug_output_queue": status.debug_output_queue,
            "inventory_error": status.inventory_error,
            "inventory_pipeline": status.inventory_pipeline,
            "game_metadata_error": status.metadata_error,
            "inventory_received_at": status.inventory_received_at,
            "game_metadata_received_at": status.metadata_received_at,
            "game_metadata_source": status.metadata_source,
            "last_observed_at": unix_time_millis(),
    })
}

fn handle_relic_observation(
    observation: Option<DebugOutputEvent>,
    session: &Session,
    relic: &crate::relic::Sender,
    last_ui_console_open: &mut Option<Instant>,
    observed_at: Instant,
    observed_at_unix_ms: u128,
) {
    match observation {
        Some(DebugOutputEvent::RelicRewards) => {
            let _ = relic.send(RelicTrigger::Rewards {
                session: session.clone(),
                observed_at,
                observed_at_unix_ms,
            });
        }
        Some(DebugOutputEvent::RelicSuggestions) => {
            let blocked = last_ui_console_open.is_some_and(|seen| {
                observed_at.saturating_duration_since(seen) < UI_CONSOLE_OPEN_GUARD
            });
            if !blocked {
                let _ = relic.send(RelicTrigger::Suggestions {
                    session: session.clone(),
                    observed_at,
                });
            }
        }
        Some(DebugOutputEvent::CloseRelicSuggestions) => {
            let _ = relic.send(RelicTrigger::CloseSuggestions);
        }
        Some(DebugOutputEvent::UiConsoleOpen) => {
            *last_ui_console_open = Some(observed_at);
        }
        None => {}
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

    #[test]
    fn stopped_observer_can_be_joined() {
        let (outbound, _receiver) = crate::daemon::outbound_channel();
        let (relic, _relic_receiver) = crate::relic::channel();
        let stopping = Arc::new(AtomicBool::new(true));
        let (_, requests) = diagnostics::channel();
        spawn(outbound, relic, requests, stopping, None)
            .join()
            .unwrap();
    }

    #[test]
    fn ui_console_open_suppresses_immediate_suggestion_trigger() {
        let (sender, receiver) = crate::relic::channel();
        let mut last = None;
        handle_relic_observation(
            Some(DebugOutputEvent::UiConsoleOpen),
            &Session::for_test(10),
            &sender,
            &mut last,
            Instant::now(),
            unix_time_millis(),
        );
        handle_relic_observation(
            Some(DebugOutputEvent::RelicSuggestions),
            &Session::for_test(10),
            &sender,
            &mut last,
            Instant::now(),
            unix_time_millis(),
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn suggestion_trigger_carries_game_process_and_time() {
        let (sender, receiver) = crate::relic::channel();
        let mut last = None;
        handle_relic_observation(
            Some(DebugOutputEvent::RelicSuggestions),
            &Session::for_test(42),
            &sender,
            &mut last,
            Instant::now(),
            unix_time_millis(),
        );
        assert!(matches!(
            receiver.recv().unwrap(),
            RelicTrigger::Suggestions { session, .. } if session.pid() == 42
        ));
    }

    #[test]
    fn collector_report_includes_receipts_errors_and_owner() {
        let (outbound, mut receiver) = crate::daemon::outbound_channel();
        let status = CollectorStatus {
            game_pid: Some(42),
            inventory_received_at: Some(1000),
            metadata_error: Some("missing StoreManifest".to_owned()),
            ..CollectorStatus::default()
        };
        publish_collector(&outbound, &status);
        let Outbound::Publish {
            dataset,
            source,
            data,
        } = receiver.try_recv().unwrap()
        else {
            panic!("expected collector report")
        };
        assert_eq!((dataset, source), ("player", "collector"));
        assert_eq!(data["companion_pid"], std::process::id());
        assert_eq!(data["game_pid"], 42);
        assert_eq!(data["inventory_received_at"], 1000);
        assert_eq!(data["game_metadata_error"], "missing StoreManifest");
        assert!(data["game_metadata_received_at"].is_null());
    }

    #[test]
    fn unsupported_executable_reports_identity_with_discovery_details() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let data =
            unsupported_game_metadata(&format!("unsupported Warframe executable {hash}")).unwrap();
        assert_eq!(data["schema"], 2);
        assert_eq!(data["executable"]["sha256"], hash);
        assert_eq!(data["unavailable"]["reason"], "unsupported_executable");
        assert_eq!(
            unsupported_game_metadata(&format!(
                "unsupported Warframe executable {hash} (game registry signature not found)"
            )),
            Some(data)
        );
        assert!(unsupported_game_metadata("unsupported Warframe executable unknown").is_none());
        assert!(unsupported_game_metadata("VariantManifest is not loaded").is_none());
    }
}
