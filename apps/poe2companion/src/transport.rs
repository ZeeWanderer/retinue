use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use retinue_client::transport::Channel;
use serde_json::{Value, json};
use tokio::net::UnixStream;

use crate::Sample;

const MAX_FRAME: usize = 128 * 1024;

pub struct Connection {
    channel: Channel,
    prediction: bool,
}

impl Connection {
    pub async fn connect(path: &Path, mode: &str) -> io::Result<Self> {
        Self::connect_interfaces(path, mode, false).await
    }

    async fn connect_interfaces(path: &Path, mode: &str, trade: bool) -> io::Result<Self> {
        let stream =
            tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(path)).await??;
        let mut connection = Self {
            channel: Channel::new(stream, MAX_FRAME),
            prediction: false,
        };
        let mut hello = json!({"op":"hello", "id":1, "game":"poe2", "envelope":2,
            "transport":"protobuf",
            "interfaces":{"session":2,"native":1}, "features":[], "client":"poe2companion",
            "version":env!("CARGO_PKG_VERSION"), "pid":std::process::id(), "mode":mode});
        if trade {
            hello["interfaces"]["trade"] = json!(5);
            hello["features"] = json!(["trade.prediction"]);
        }
        let reply =
            tokio::time::timeout(Duration::from_secs(3), connection.request(&hello)).await??;
        if reply["ok"] != true
            || reply["compatible"] != true
            || reply["envelope"] != 2
            || reply["interfaces"]["session"] != 2
            || reply["interfaces"]["native"] != 1
            || reply["transport"] != "protobuf"
            || reply["game"] != "poe2"
            || (trade && reply["interfaces"]["trade"] != 5)
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                if trade {
                    "daemon contract mismatch: need envelope 2, native 1, poe2 session 2 and trade 5"
                } else {
                    "daemon contract mismatch: need envelope 2, native 1 and poe2 session 2"
                },
            ));
        }
        connection.prediction = reply["features"]
            .as_array()
            .is_some_and(|features| features.iter().any(|f| f == "trade.prediction"));
        connection.channel.enable_protobuf("poe2")?;
        Ok(connection)
    }

    pub async fn trade(path: &Path, mut request: Value) -> io::Result<Value> {
        let dataset = trade_dataset(&request)?.to_owned();
        let mut connection = Self::connect_interfaces(path, "overlay", true).await?;
        if request["op"] == "trade_predict" && !connection.prediction {
            return Ok(json!({"ok":true,"dataset":"trade_predict","data":{"estimate":null}}));
        }
        request["id"] = json!(2);
        let reply =
            tokio::time::timeout(Duration::from_secs(45), connection.request(&request)).await??;
        if reply["ok"] == true && reply["dataset"] != dataset {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid trade dataset",
            ));
        }
        Ok(reply)
    }

    async fn request(&mut self, request: &Value) -> io::Result<Value> {
        self.channel.request(request).await
    }

    pub async fn publish(&mut self, sample: &Sample) -> io::Result<()> {
        let id = sample
            .sequence
            .checked_add(1)
            .filter(|_| sample.sequence > 0)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid observation sequence")
            })?;
        let request = json!({"op":"publish", "id":id, "dataset":"session", "data":sample});
        let reply = tokio::time::timeout(Duration::from_secs(30), self.request(&request)).await??;
        if reply["ok"] != true {
            let kind = if matches!(
                reply["error"].as_str(),
                Some(
                    "invalid_session_sample"
                        | "unsupported_poe2_request"
                        | "unauthorized"
                        | "observer_required"
                        | "session_sequence_conflict"
                        | "stale_session_sample"
                )
            ) {
                io::ErrorKind::InvalidInput
            } else {
                io::ErrorKind::Other
            };
            return Err(io::Error::new(
                kind,
                "daemon rejected native session observation",
            ));
        }
        if reply["dataset"] != "session_published" || reply["data"]["revision"].as_u64().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid session acknowledgement",
            ));
        }
        Ok(())
    }
}

fn trade_dataset(request: &Value) -> io::Result<&str> {
    match request["op"].as_str() {
        Some(
            op @ ("trade_item" | "trade_search" | "trade_fetch" | "trade_restore" | "trade_leagues"
            | "trade_predict" | "currency_asset"),
        ) => Ok(op),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid trade operation",
        )),
    }
}

pub struct Publisher {
    socket: PathBuf,
    mode: String,
    connection: Option<Connection>,
    ensured: bool,
}

impl Publisher {
    pub fn new(socket: PathBuf, mode: &str) -> Self {
        Self {
            socket,
            mode: mode.into(),
            connection: None,
            ensured: false,
        }
    }

    pub async fn publish(&mut self, sample: &Sample, stop: &AtomicBool) -> io::Result<()> {
        let mut delay = Duration::from_millis(500);
        loop {
            let result = tokio::select! {
                result = async {
                    if self.connection.is_none() { self.connection = Some(Connection::connect(&self.socket, &self.mode).await?); }
                    self.connection.as_mut().unwrap().publish(sample).await
                } => result,
                _ = cancelled(stop) => return Err(io::Error::new(io::ErrorKind::Interrupted, "publisher stopped")),
            };
            match result {
                Ok(()) => {
                    self.ensured = false;
                    return Ok(());
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Unsupported
                            | io::ErrorKind::InvalidData
                            | io::ErrorKind::InvalidInput
                    ) =>
                {
                    return Err(error);
                }
                Err(error) => {
                    self.connection = None;
                    eprintln!(
                        "poe2companion: {error}; retrying observation {}",
                        sample.sequence
                    );
                    if !self.ensured
                        && matches!(
                            error.kind(),
                            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                        )
                    {
                        match ensure_daemon(stop).await {
                            Ok(()) => self.ensured = true,
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                                return Err(error);
                            }
                            Err(error) => eprintln!("poe2companion: daemon ensure failed: {error}"),
                        }
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {},
                        _ = cancelled(stop) => return Err(io::Error::new(io::ErrorKind::Interrupted, "publisher stopped")),
                    }
                    delay = (delay * 2).min(Duration::from_secs(10));
                }
            }
        }
    }
}

async fn cancelled(stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn ensure_daemon(stop: &AtomicBool) -> io::Result<()> {
    let command = std::env::var_os("RETINUE_COMMAND")
        .map(PathBuf::from)
        .or_else(|| {
            retinue_client::executable_path().and_then(|exe| {
                exe.ancestors()
                    .map(|p| p.join("retinue"))
                    .find(|p| p.is_file())
            })
        })
        .unwrap_or_else(|| PathBuf::from("retinue"));
    let mut process = tokio::process::Command::new(command);
    process
        .args(["daemon", "ensure"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for key in [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "STEAM_RUNTIME",
        "STEAM_RUNTIME_LIBRARY_PATH",
    ] {
        process.env_remove(key);
    }
    let mut child = process.spawn()?;
    let result = tokio::select! {
        status = child.wait() => status,
        _ = tokio::time::sleep(Duration::from_secs(30)) => Err(io::Error::new(io::ErrorKind::TimedOut, "daemon ensure timed out")),
        _ = cancelled(stop) => Err(io::Error::new(io::ErrorKind::Interrupted, "publisher stopped")),
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    let status = result?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "retinue daemon ensure exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    async fn bootstrap(listener: UnixListener, native: bool) -> Channel {
        let (socket, _) = listener.accept().await.unwrap();
        let mut channel = Channel::new(socket, MAX_FRAME);
        let hello = channel.receive_request().await.unwrap();
        assert_eq!(hello["interfaces"]["native"], 1);
        assert_eq!(hello["game"], "poe2");
        assert_eq!(hello["transport"], "protobuf");
        let mut interfaces = json!({"session":2,"trade":5});
        if native {
            interfaces["native"] = 1.into();
        }
        channel
            .send(&json!({"id":1,"ok":true,"compatible":true,"envelope":2,
                "game":"poe2","transport":"protobuf","interfaces":interfaces,"features":[]}))
            .await
            .unwrap();
        if native {
            channel.enable_protobuf("poe2").unwrap();
        }
        channel
    }

    #[tokio::test]
    async fn native_interface_is_mandatory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move { bootstrap(listener, false).await });
        let result = Connection::connect(&path, "watch").await;
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::Unsupported));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn json_transport_downgrade_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut channel = Channel::new(socket, MAX_FRAME);
            channel.receive_request().await.unwrap();
            channel
                .send(&json!({"id":1,"ok":true,"compatible":true,"envelope":2,
                "game":"poe2","transport":"json","interfaces":{"session":2,"native":1}}))
                .await
                .unwrap();
        });
        let result = Connection::connect(&path, "watch").await;
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::Unsupported));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn hello_switches_to_typed_publication_and_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let mut channel = bootstrap(listener, true).await;
            let request = tokio::time::timeout(Duration::from_secs(3), channel.receive_request())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(request["op"], "publish");
            assert_eq!(request["dataset"], "session");
            assert_eq!(request["id"], 2);
            assert_eq!(request["data"]["snapshot"], Value::Null);
            assert_eq!(request["data"]["error"], Value::Null);
            channel
                .send(&json!({"id":2,"ok":false,"error":"invalid_session_sample"}))
                .await
                .unwrap();
        });
        let mut connection = Connection::connect(&path, "watch").await.unwrap();
        let sample = Sample {
            stream: "test".into(),
            sequence: 1,
            observed_at: 1000,
            observation: crate::Observation::default(),
        };
        let error = connection.publish(&sample).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unnegotiated_prediction_keeps_typed_dataset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move { bootstrap(listener, true).await });
        let reply = Connection::trade(&path, json!({"op":"trade_predict"}))
            .await
            .unwrap();
        assert_eq!(reply["dataset"], "trade_predict");
        assert_eq!(reply["data"]["estimate"], Value::Null);
        server.await.unwrap();
    }

    #[test]
    fn trade_dataset_matches_operation() {
        for op in [
            "trade_item",
            "trade_search",
            "trade_fetch",
            "trade_restore",
            "trade_leagues",
            "trade_predict",
            "currency_asset",
        ] {
            assert_eq!(trade_dataset(&json!({"op":op})).unwrap(), op);
        }
        assert!(trade_dataset(&json!({"op":"get"})).is_err());
    }
}
