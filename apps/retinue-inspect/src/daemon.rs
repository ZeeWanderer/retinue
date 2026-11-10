use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use retinue_client::{local_protocol, transport::Channel};
use tokio::{net::UnixStream, runtime::Runtime, time::timeout};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    Warframe,
    Poe2,
}

impl Game {
    fn name(self) -> &'static str {
        match self {
            Self::Warframe => "warframe",
            Self::Poe2 => "poe2",
        }
    }

    fn interfaces(self) -> BTreeMap<&'static str, u32> {
        match self {
            Self::Warframe => local_protocol::interfaces(),
            Self::Poe2 => BTreeMap::from([("native", 1), ("session", 2)]),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HandshakeReport {
    pub game: Game,
    pub socket: PathBuf,
    pub round_trip_ms: u128,
    pub required_interfaces: Value,
    pub response: Value,
}

#[derive(Debug, Serialize)]
pub struct DatasetReport {
    pub handshake: HandshakeReport,
    pub request_ms: u128,
    pub response: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubscriptionItem {
    Initial {
        message: Value,
    },
    Update {
        observed_after_ms: u128,
        message: Value,
    },
}

#[derive(Debug, Serialize)]
pub struct SubscriptionSummary {
    pub handshake: HandshakeReport,
    pub duration_ms: u128,
    pub events: usize,
}

pub fn handshake(game: Game) -> Result<HandshakeReport, String> {
    let mut connection = Connection::connect()?;
    connection.handshake(game)
}

pub fn dataset(game: Game, name: &str) -> Result<DatasetReport, String> {
    let mut connection = Connection::connect()?;
    let handshake = connection.handshake(game)?;
    require_compatible(&handshake.response, game)?;
    let started = Instant::now();
    connection.send(&json!({"op": "get", "id": 2, "game": game, "dataset": name}))?;
    let response = connection.receive(REQUEST_TIMEOUT)?;
    Ok(DatasetReport {
        handshake,
        request_ms: started.elapsed().as_millis(),
        response,
    })
}

pub fn subscribe_stream(
    game: Game,
    name: &str,
    view: Option<&str>,
    duration: Duration,
    limit: usize,
    mut emit: impl FnMut(&SubscriptionItem) -> Result<(), String>,
) -> Result<SubscriptionSummary, String> {
    validate_bounds(duration, limit)?;
    if game != Game::Warframe && view.is_some() {
        return Err("dataset projections are only available for Warframe".into());
    }
    let mut connection = Connection::connect()?;
    let handshake = connection.handshake(game)?;
    require_compatible(&handshake.response, game)?;
    let mut request = json!({
        "op": "subscribe",
        "id": 2,
        "game": game,
        "dataset": name,
    });
    if let Some(view) = view {
        request["view"] = view.into();
    } else if game == Game::Warframe {
        request["include_data"] = false.into();
    }
    connection.send(&request)?;
    let initial = connection.receive(REQUEST_TIMEOUT)?;
    require_ok(&initial, "subscription")?;
    emit(&SubscriptionItem::Initial { message: initial })?;

    let started = Instant::now();
    let deadline = started + duration;
    let mut events = 0;
    while Instant::now() < deadline && events < limit {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match connection.receive(remaining) {
            Ok(message) => {
                let item = SubscriptionItem::Update {
                    observed_after_ms: started.elapsed().as_millis(),
                    message,
                };
                emit(&item)?;
                events += 1;
            }
            Err(error) if error == "daemon read timed out" => break,
            Err(error) => return Err(error),
        }
    }
    let _ = connection.send(&json!({
        "op": "unsubscribe",
        "id": 3,
        "game": game,
        "subscription": 2,
    }));
    Ok(SubscriptionSummary {
        handshake,
        duration_ms: started.elapsed().as_millis(),
        events,
    })
}

struct Connection {
    socket: PathBuf,
    channel: Channel,
    runtime: Runtime,
}

impl Connection {
    fn connect() -> Result<Self, String> {
        let socket = local_protocol::socket_path();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let stream = runtime
            .block_on(UnixStream::connect(&socket))
            .map_err(|error| format!("could not connect to {}: {error}", socket.display()))?;
        Ok(Self {
            socket,
            channel: Channel::new(stream, MAX_FRAME_BYTES),
            runtime,
        })
    }

    fn handshake(&mut self, game: Game) -> Result<HandshakeReport, String> {
        let request = hello_request(game);
        let required_interfaces = request["interfaces"].clone();
        let started = Instant::now();
        self.send(&request)?;
        let response = self.receive(HANDSHAKE_TIMEOUT)?;
        if require_compatible(&response, game).is_ok() {
            self.channel
                .enable_protobuf(game.name())
                .map_err(|error| error.to_string())?;
        }
        Ok(HandshakeReport {
            game,
            socket: self.socket.clone(),
            round_trip_ms: started.elapsed().as_millis(),
            required_interfaces,
            response,
        })
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        self.runtime
            .block_on(async { timeout(REQUEST_TIMEOUT, self.channel.send(message)).await })
            .map_err(|_| "daemon write timed out".to_owned())?
            .map_err(|error| format!("could not write daemon request: {error}"))
    }

    fn receive(&mut self, duration: Duration) -> Result<Value, String> {
        self.runtime
            .block_on(async { timeout(duration, self.channel.receive()).await })
            .map_err(|_| "daemon read timed out".to_owned())?
            .map_err(|error| format!("could not read daemon response: {error}"))
    }
}

fn hello_request(game: Game) -> Value {
    json!({
        "op": "hello",
        "id": 1,
        "envelope": local_protocol::ENVELOPE_VERSION,
        "transport": "protobuf",
        "game": game,
        "interfaces": game.interfaces(),
        "features": [],
        "client": "retinue-inspect",
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "mode": "diagnostic",
    })
}

fn require_compatible(response: &Value, game: Game) -> Result<(), String> {
    require_ok(response, "handshake")?;
    if response.get("compatible").and_then(Value::as_bool) == Some(true)
        && response["transport"] == "protobuf"
        && response["envelope"] == local_protocol::ENVELOPE_VERSION
        && response["game"] == json!(game)
        && game.interfaces().iter().all(|(name, version)| {
            response["interfaces"][*name].as_u64() == Some(u64::from(*version))
        })
    {
        Ok(())
    } else {
        Err(format!("daemon contract mismatch: {response}"))
    }
}

fn require_ok(response: &Value, operation: &str) -> Result<(), String> {
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(format!("daemon {operation} failed: {response}"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warframe_hello_requires_complete_local_contract() {
        let hello = hello_request(Game::Warframe);
        assert_eq!(hello["envelope"], 2);
        assert_eq!(hello["game"], "warframe");
        assert_eq!(hello["interfaces"], json!(local_protocol::interfaces()));
        assert_eq!(hello["interfaces"]["worldstate"], 1);
        assert_eq!(hello["interfaces"]["builds"], 1);
        assert_eq!(hello["client"], "retinue-inspect");
    }

    #[test]
    fn poe2_hello_requires_native_session_contract() {
        let hello = hello_request(Game::Poe2);
        assert_eq!(hello["envelope"], 2);
        assert_eq!(hello["game"], "poe2");
        assert_eq!(hello["interfaces"], json!({"native": 1, "session": 2}));
        assert_eq!(hello["client"], "retinue-inspect");
    }

    #[test]
    fn connection_switches_to_native_frames_after_hello() {
        use retinue_protocol::warframe_native::{RelicRecommendations, RelicSuggestion};
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        server.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut channel =
                    Channel::new(UnixStream::from_std(server).unwrap(), MAX_FRAME_BYTES);
                let hello = channel.receive_request().await.unwrap();
                assert_eq!(hello["transport"], "protobuf");
                channel
                    .send(&json!({"id": 1, "ok": true, "compatible": true,
                    "envelope": local_protocol::ENVELOPE_VERSION, "game": "warframe",
                    "interfaces": Game::Warframe.interfaces(), "transport": "protobuf"}))
                    .await
                    .unwrap();
                channel.enable_protobuf("warframe").unwrap();
                assert_eq!(
                    channel.receive_request().await.unwrap()["op"],
                    "relic_recommendations"
                );
                channel
                    .send_data(
                        2,
                        &RelicRecommendations {
                            trace_count: 10,
                            items: vec![RelicSuggestion {
                                name: "Neo A1".into(),
                                favorite: true,
                                expected_ducats: 58.0,
                                ..Default::default()
                            }],
                        },
                    )
                    .await
                    .unwrap();
            });
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let channel = {
            let _guard = runtime.enter();
            Channel::new(UnixStream::from_std(client).unwrap(), MAX_FRAME_BYTES)
        };
        let mut connection = Connection {
            socket: PathBuf::new(),
            channel,
            runtime,
        };
        let report = connection.handshake(Game::Warframe).unwrap();
        require_compatible(&report.response, Game::Warframe).unwrap();
        connection
            .send(&json!({"id": 2, "op": "relic_recommendations", "era": "neo"}))
            .unwrap();
        let response = connection.receive(REQUEST_TIMEOUT).unwrap();
        assert_eq!(response["data"]["items"][0]["expected_ducats"], 58.0);
        assert_eq!(response["data"]["items"][0]["favorite"], true);
        worker.join().unwrap();
    }

    #[test]
    fn requests_require_current_envelope_game_and_interface_versions() {
        for game in [Game::Warframe, Game::Poe2] {
            let response = json!({
                "ok": true, "compatible": true, "envelope": 2,
                "game": game, "interfaces": game.interfaces(), "transport": "protobuf",
            });
            assert!(require_compatible(&response, game).is_ok());
            for (field, value) in [
                ("envelope", json!(1)),
                ("game", json!("arc")),
                ("interfaces", json!({})),
                ("compatible", json!(false)),
                ("transport", json!("json")),
            ] {
                let mut changed = response.clone();
                changed[field] = value;
                assert!(require_compatible(&changed, game).is_err());
            }
        }
        assert!(
            require_compatible(&json!({"ok": true, "compatible": true}), Game::Warframe).is_err()
        );
        assert!(
            require_compatible(
                &json!({
                    "ok": true, "compatible": true, "envelope": 2,
                    "game": "poe2", "interfaces": {"session": 3},
                }),
                Game::Poe2
            )
            .is_err()
        );
    }

    #[test]
    fn subscription_is_bounded() {
        assert!(validate_bounds(Duration::ZERO, 1).is_err());
        assert!(validate_bounds(Duration::from_secs(301), 1).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 0).is_err());
        assert!(validate_bounds(Duration::from_secs(1), 1001).is_err());
    }
}
