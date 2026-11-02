use crate::{data::foundry::Asset, data::foundry::Descriptor, data::foundry::Snapshot};
use retinue_client::transport::Channel;
use serde_json::{Value, json};
use std::{collections::HashSet, thread, time::Duration};
use tokio::{sync::oneshot, sync::watch};

pub enum Update {
    Snapshot(Snapshot),
    Assets(Vec<Descriptor>),
    AssetErrors(Vec<(String, String)>),
    Error(String),
}

#[derive(Clone, Default)]
pub struct Desired {
    pub assets: Vec<Asset>,
    pub refresh: u64,
}

pub struct Worker {
    desired: watch::Sender<Desired>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub fn new(send: impl Fn(Update) -> bool + Send + 'static) -> Result<Self, String> {
        let (desired, mut wanted) = watch::channel(Desired::default());
        let (stop, mut stopped) = oneshot::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let thread = thread::Builder::new().name("wfgui-foundry".into()).spawn(move || {
            runtime.block_on(async {
                loop {
                    let result = tokio::select! {
                        _ = &mut stopped => break,
                        result = session(&mut wanted, &send) => result,
                    };
                    if let Err(error) = result { if !send(Update::Error(error)) { break; } }
                    tokio::select! { _ = &mut stopped => break, _ = tokio::time::sleep(Duration::from_secs(2)) => () }
                }
            });
        }).map_err(|e| e.to_string())?;
        Ok(Self {
            desired,
            stop: Some(stop),
            thread: Some(thread),
        })
    }
    pub fn set(&self, desired: Desired) {
        self.desired.send_replace(desired);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub async fn connect() -> Result<Channel, String> {
    crate::domain::session::connect().await
}

async fn session(
    wanted: &mut watch::Receiver<Desired>,
    send: &impl Fn(Update) -> bool,
) -> Result<(), String> {
    run(connect().await?, wanted, send).await
}

async fn run(
    mut channel: Channel,
    wanted: &mut watch::Receiver<Desired>,
    send: &impl Fn(Update) -> bool,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    let mut assets_pending = false;
    let mut refresh = wanted.borrow().refresh;
    let mut refresh_needed = true;
    let mut view_pending = false;
    let mut view_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut asset_deadline = view_deadline;
    channel
        .send(&json!({"op":"subscribe","id":3,"dataset":"player","include_data":false}))
        .await
        .map_err(|e| e.to_string())?;
    loop {
        if refresh_needed && !view_pending {
            refresh_needed = false;
            view_pending = true;
            view_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            channel
                .send(&json!({"op":"foundry_view","id":2}))
                .await
                .map_err(|e| e.to_string())?;
        }
        if !assets_pending {
            let assets = wanted
                .borrow()
                .assets
                .iter()
                .filter(|a| !seen.contains(*a))
                .take(32)
                .cloned()
                .collect::<Vec<_>>();
            if !assets.is_empty() {
                seen.extend(assets.iter().cloned());
                channel
                    .send(&json!({"op":"asset_resolve","id":4,"assets":assets}))
                    .await
                    .map_err(|e| e.to_string())?;
                assets_pending = true;
                asset_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            }
        }
        let deadline = match (view_pending, assets_pending) {
            (true, true) => view_deadline.min(asset_deadline),
            (true, false) => view_deadline,
            _ => asset_deadline,
        };
        let message = tokio::select! {
            changed = wanted.changed() => {
                if changed.is_err() { return Ok(()); }
                let next = wanted.borrow_and_update().refresh;
                if next != refresh { refresh = next; refresh_needed = true; seen.clear(); }
                continue;
            }
            _ = tokio::time::sleep_until(deadline), if view_pending || assets_pending => return Err("Foundry request timed out".into()),
            message = channel.receive_packet() => message.map_err(|e| e.to_string())?,
        };
        if message.id() == Some(2) {
            view_pending = false;
            let snapshot = message
                .decode()
                .map_err(|e| format!("Invalid Foundry data: {e}"))?;
            if !send(Update::Snapshot(snapshot)) {
                return Ok(());
            }
            continue;
        }
        let message = message.json().map_err(|error| error.to_string())?;
        if message["event"] == "asset" {
            if let Some(asset) = descriptor(&message["data"]) {
                if !send(Update::Assets(vec![asset])) {
                    return Ok(());
                }
            }
        } else if message["event"] == "dataset" || message["event"] == "update" {
            if message["dataset"] == "player" {
                refresh_needed = true;
            }
        } else if message["id"] == 4 {
            assets_pending = false;
            if message["ok"] != true {
                return Err(error(&message));
            }
            let values = message["data"]["assets"]
                .as_array()
                .ok_or("Invalid asset response")?;
            let errors = asset_errors(values);
            if !errors.is_empty() && !send(Update::AssetErrors(errors)) {
                return Ok(());
            }
            let assets = message["data"]["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(descriptor)
                .collect();
            if !send(Update::Assets(assets)) {
                return Ok(());
            }
        } else if message["id"] == 3 && message["ok"] != true {
            return Err(error(&message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_protocol::{
        prost::Message, prost::Name, warframe_native::AssetDescriptor, warframe_native::Assets,
        warframe_native::PlayerSnapshot, wire,
    };

    fn event<T: Message + Name>(name: &str, data: &T) -> wire::Packet {
        wire::Packet {
            body: Some(wire::packet::Body::Event(wire::Event {
                event: name.into(),
                dataset: Some("player".into()),
                source: Some("inventory_native".into()),
                type_name: T::full_name(),
                payload: data.encode_to_vec().into(),
                ..Default::default()
            })),
        }
    }
    #[tokio::test]
    async fn events_interleave_with_requests_and_scrolling_replaces_unsent_assets() {
        let (client, server) = tokio::net::UnixStream::pair().unwrap();
        let asset = |i| Asset {
            id: format!("asset-{i}"),
            source: "wfcd".into(),
            image_name: format!("{i}.png"),
        };
        let (desired, mut wanted) = watch::channel(Desired {
            assets: (0..70).map(asset).collect(),
            refresh: 0,
        });
        let updates = std::sync::Mutex::new(Vec::new());
        let send = |update| {
            updates.lock().unwrap().push(update);
            true
        };
        let mut client = Channel::new(client, 65536);
        client.enable_protobuf("warframe").unwrap();
        let client = run(client, &mut wanted, &send);
        let server = async {
            let mut server = Channel::new(server, 65536);
            server.enable_protobuf("warframe").unwrap();
            assert_eq!(server.receive_request().await.unwrap()["op"], "subscribe");
            assert_eq!(
                server.receive_request().await.unwrap()["op"],
                "foundry_view"
            );
            assert_eq!(
                server.receive_request().await.unwrap()["assets"]
                    .as_array()
                    .unwrap()
                    .len(),
                32
            );
            desired.send_replace(Desired {
                assets: vec![asset(80)],
                refresh: 0,
            });
            server
                .send_packet(&event(
                    "asset",
                    &AssetDescriptor {
                        ok: Some(true),
                        id: Some("a".into()),
                        path: Some("a.png".into()),
                        digest: Some("11".repeat(32)),
                        ..Default::default()
                    },
                ))
                .await
                .unwrap();
            server.send(&json!({"id":3,"ok":true})).await.unwrap();
            server
                .send_data(
                    2,
                    &Snapshot {
                        revision: 1,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            server.send_data(4, &Assets::default()).await.unwrap();
            let batch = server.receive_request().await.unwrap();
            assert_eq!(batch["assets"], json!([asset(80)]));
            server.send_data(4, &Assets::default()).await.unwrap();
            server
                .send_packet(&event("dataset", &PlayerSnapshot::default()))
                .await
                .unwrap();
            assert_eq!(
                server.receive_request().await.unwrap()["op"],
                "foundry_view"
            );
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(client, server)
        })
        .await
        .unwrap();
        assert!(result.is_err());
        let updates = updates.into_inner().unwrap();
        assert!(updates.iter().any(|v| matches!(v, Update::Snapshot(_))));
        assert!(
            updates
                .iter()
                .any(|v| matches!(v,Update::Assets(a) if a.iter().any(|a|a.id=="a")))
        );
    }
}

pub fn descriptor(value: &Value) -> Option<Descriptor> {
    (value["ok"] == true)
        .then(|| serde_json::from_value(value.clone()).ok())
        .flatten()
}

pub fn asset_errors(values: &[Value]) -> Vec<(String, String)> {
    values
        .iter()
        .filter(|v| descriptor(v).is_none())
        .filter_map(|v| {
            Some((
                v["id"].as_str()?.to_owned(),
                v["error"]
                    .as_str()
                    .unwrap_or("Invalid asset descriptor")
                    .to_owned(),
            ))
        })
        .collect()
}

fn error(message: &Value) -> String {
    message["error"]
        .as_str()
        .unwrap_or("Foundry request failed")
        .replace('_', " ")
}

pub async fn snapshot(channel: &mut Channel) -> Result<Snapshot, String> {
    let reply = tokio::time::timeout(
        Duration::from_secs(30),
        channel.request_packet(&json!({"op":"foundry_view","id":2})),
    )
    .await
    .map_err(|_| "Foundry request timed out".to_owned())?
    .map_err(|e| e.to_string())?;
    reply.decode().map_err(|e| e.to_string())
}
