use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use retinue_client::transport::JsonChannel;
use serde_json::{Value, json};
use tokio::net::UnixStream;

use crate::Sample;

const MAX_FRAME: usize = 128 * 1024;

pub struct Connection {
    channel: JsonChannel,
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
            channel: JsonChannel::new(stream, MAX_FRAME),
            prediction: false,
        };
        let mut hello = json!({"op":"hello", "id":1, "game":"poe2", "envelope":2,
            "interfaces":{"session":2}, "features":[], "client":"poe2companion",
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
            || reply["game"] != "poe2"
            || (trade && reply["interfaces"]["trade"] != 5)
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                if trade {
                    "daemon contract mismatch: need envelope 2, poe2 session 2 and trade 5"
                } else {
                    "daemon contract mismatch: need envelope 2, poe2 session 2"
                },
            ));
        }
        connection.prediction = reply["features"]
            .as_array()
            .is_some_and(|features| features.iter().any(|f| f == "trade.prediction"));
        Ok(connection)
    }

    pub async fn trade(path: &Path, mut request: Value) -> io::Result<Value> {
        let mut connection = Self::connect_interfaces(path, "overlay", true).await?;
        if request["op"] == "trade_predict" && !connection.prediction {
            return Ok(json!({"ok":true,"dataset":"trade","data":{"estimate":null}}));
        }
        request["id"] = json!(2);
        let reply =
            tokio::time::timeout(Duration::from_secs(45), connection.request(&request)).await??;
        if reply["ok"] == true && reply["dataset"] != "trade" {
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
        if reply["dataset"] != "session" || reply["data"]["revision"].as_u64().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid session acknowledgement",
            ));
        }
        Ok(())
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
