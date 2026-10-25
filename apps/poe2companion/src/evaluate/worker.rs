use super::model::{Estimate, Evaluation, ListingPage, Restoration, Results};
use retinue_client::paths::Paths;
use serde_json::Value;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::{Notify, mpsc, watch};

pub enum Job {
    Copy(u64, Option<u32>),
    Evaluate(u64, String),
    Search(u64, Value),
    Restore(u64, Value, bool),
    Fetch(u64, Value),
    Predict(u64, Value),
    Open(String),
}
pub enum Reply {
    Currencies(
        super::currency::Request,
        std::collections::BTreeMap<String, super::currency::Icon>,
    ),
    Copied(u64, String),
    Evaluated(u64, Evaluation),
    Searched(u64, Results),
    Restored(u64, Restoration, bool),
    Fetched(u64, ListingPage),
    Predicted(u64, Result<Option<Estimate>, String>),
    Error(u64, String, u64),
}
pub struct Worker {
    send: mpsc::Sender<Job>,
    capture: mpsc::Sender<Job>,
    prediction: watch::Sender<Option<(u64, Value)>>,
    restoration: watch::Sender<Option<(u64, Value, bool)>>,
    pub budget: watch::Receiver<Option<super::budget::Snapshot>>,
    pub currencies: watch::Sender<super::currency::Request>,
    pub receive: mpsc::Receiver<Reply>,
    stop: Arc<AtomicBool>,
    stopped: Arc<Notify>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(wake: retinue_ui::schedule::Ping) -> Result<Self, String> {
        let (send, mut jobs) = mpsc::channel(4);
        let (capture, mut copies) = mpsc::channel(1);
        let (prediction, mut predictions) = watch::channel::<Option<(u64, Value)>>(None);
        let (restoration, mut restorations) = watch::channel::<Option<(u64, Value, bool)>>(None);
        let (budget_updates, budget) = watch::channel(None);
        let (currencies, currency_requests) = watch::channel(super::currency::Request::default());
        let (replies, receive) = mpsc::channel(4);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let stopped = Arc::new(Notify::new());
        let stop_notice = stopped.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let thread = thread::Builder::new()
            .name("poe2-evaluate-requests".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let currency_task = tokio::spawn(super::currency::run(currency_requests, replies.clone(), wake.clone()));
                    let mut tasks = tokio::task::JoinSet::new();
                    let mut captures = tokio::task::JoinSet::new();
                    let mut estimates = tokio::task::JoinSet::new();
                    let mut lookups = tokio::task::JoinSet::new();
                    loop {
                        tokio::select! {
                            _ = stop_notice.notified() => break,
                            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                            Some(_) = captures.join_next(), if !captures.is_empty() => {},
                            Some(_) = estimates.join_next(), if !estimates.is_empty() => {},
                            Some(_) = lookups.join_next(), if !lookups.is_empty() => {},
                            changed = restorations.changed() => {
                                if changed.is_err() { break; }
                                lookups.abort_all();
                                let latest = restorations.borrow_and_update().clone();
                                if let Some((id, request, requested)) = latest {
                                    lookups.spawn(respond(Job::Restore(id, request, requested), stopping.clone(), replies.clone(), budget_updates.clone(), wake.clone()));
                                }
                            }
                            changed = predictions.changed() => {
                                if changed.is_err() { break; }
                                estimates.abort_all();
                                let latest = predictions.borrow_and_update().clone();
                                if let Some((id, request)) = latest {
                                    estimates.spawn(respond(Job::Predict(id, request), stopping.clone(), replies.clone(), budget_updates.clone(), wake.clone()));
                                }
                            }
                            job = copies.recv(), if captures.is_empty() => {
                                let Some(job) = job else { break; };
                                captures.spawn(respond(job, stopping.clone(), replies.clone(), budget_updates.clone(), wake.clone()));
                            }
                            job = jobs.recv(), if tasks.len() < 2 => {
                                let Some(job) = job else { break; };
                                tasks.spawn(respond(job, stopping.clone(), replies.clone(), budget_updates.clone(), wake.clone()));
                            }
                        }
                    }
                    tasks.abort_all();
                    currency_task.abort();
                    let _ = currency_task.await;
                    estimates.abort_all();
                    lookups.abort_all();
                    while tasks.join_next().await.is_some() {}
                    while estimates.join_next().await.is_some() {}
                    while lookups.join_next().await.is_some() {}
                    while captures.join_next().await.is_some() {}
                });
                runtime.shutdown_timeout(Duration::from_secs(3));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            send,
            capture,
            prediction,
            restoration,
            budget,
            currencies,
            receive,
            stop,
            stopped,
            thread: Some(thread),
        })
    }
    pub fn submit(&self, job: Job) -> Result<(), String> {
        if let Job::Restore(id, request, requested) = job {
            return self
                .restoration
                .send(Some((id, request, requested)))
                .map_err(|_| "Evaluate worker stopped".into());
        }
        if let Job::Predict(id, request) = job {
            return self
                .prediction
                .send(Some((id, request)))
                .map_err(|_| "Evaluate worker stopped".into());
        }
        let send = if matches!(job, Job::Copy(..)) {
            &self.capture
        } else {
            &self.send
        };
        send.try_send(job)
            .map_err(|_| "Evaluate worker is busy; try again".into())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.stopped.notify_one();
        self.receive.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn respond(
    job: Job,
    cancel: Arc<AtomicBool>,
    replies: mpsc::Sender<Reply>,
    budget: watch::Sender<Option<super::budget::Snapshot>>,
    wake: retinue_ui::schedule::Ping,
) {
    if let Some(reply) = perform(job, cancel, &budget).await {
        if replies.send(reply).await.is_ok() {
            wake.ping();
        }
    }
}

async fn perform(
    job: Job,
    cancel: Arc<AtomicBool>,
    budget: &watch::Sender<Option<super::budget::Snapshot>>,
) -> Option<Reply> {
    let requested = matches!(&job, Job::Restore(_, _, true));
    if let Job::Predict(id, request) = job {
        let path = Paths::from_env().daemon_socket().to_owned();
        let result = match trade_request(&path, request).await {
            Ok(reply) if reply["ok"] == true => {
                serde_json::from_value(reply["data"]["estimate"].clone())
                    .map_err(|e| format!("Invalid prediction response: {e}"))
            }
            Ok(reply) => Err(error_message(&reply)),
            Err(error) => Err(error.to_string()),
        };
        return Some(Reply::Predicted(id, result));
    }
    let (id, request, operation) = match job {
        Job::Copy(id, pid) => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            return Some(
                match tokio::task::spawn_blocking(move || {
                    retinue_ui::clipboard::copy_from_game("steam_app_2694490", pid, &cancel)
                        .and_then(super::model::validate_item_text)
                })
                .await
                {
                    Ok(Ok(text)) => Reply::Copied(id, text),
                    Ok(Err(error)) => Reply::Error(id, error, 0),
                    Err(error) => Reply::Error(id, error.to_string(), 0),
                },
            );
        }
        Job::Evaluate(id, text) => (
            id,
            serde_json::json!({"op":"trade_item","text":text}),
            "item",
        ),
        Job::Search(id, request) => (id, request, "search"),
        Job::Restore(id, request, _) => (id, request, "restore"),
        Job::Fetch(id, request) => (id, request, "fetch"),
        Job::Predict(..) => unreachable!(),
        Job::Open(url) => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if url.starts_with("https://www.pathofexile.com/trade2/search/poe2/") {
                let mut command = tokio::process::Command::new("xdg-open");
                command
                    .arg(url)
                    .kill_on_drop(true)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                if let Ok(mut child) = command.spawn() {
                    if tokio::time::timeout(Duration::from_secs(15), child.wait())
                        .await
                        .is_err()
                    {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                }
            }
            return None;
        }
    };
    let path = Paths::from_env().daemon_socket().to_owned();
    let reply = trade_request(&path, request).await;
    if let Ok(reply) = &reply {
        let key = if reply["ok"] == true { "data" } else { "error" };
        if let Ok(snapshot) =
            serde_json::from_value::<super::budget::Snapshot>(reply[key]["rate_budget"].clone())
        {
            budget.send_if_modified(|current| {
                if current
                    .as_ref()
                    .is_some_and(|old| old.observed_at_ms > snapshot.observed_at_ms)
                {
                    return false;
                }
                *current = Some(snapshot);
                true
            });
        }
    }
    Some(match reply {
        Err(error) => Reply::Error(id, error.to_string(), 0),
        Ok(reply) if reply["ok"] != true => Reply::Error(
            id,
            error_message(&reply),
            reply["error"]["retry_after_seconds"].as_u64().unwrap_or(0),
        ),
        Ok(reply) => {
            let data = reply["data"].clone();
            match operation {
                "item" => serde_json::from_value(data).map(|e| Reply::Evaluated(id, e)),
                "fetch" => serde_json::from_value(data).map(|r| Reply::Fetched(id, r)),
                "restore" => {
                    serde_json::from_value(data).map(|r| Reply::Restored(id, r, requested))
                }
                _ => serde_json::from_value(data).map(|r| Reply::Searched(id, r)),
            }
            .unwrap_or_else(|e| Reply::Error(id, format!("Invalid trade response: {e}"), 0))
        }
    })
}

async fn trade_request(path: &std::path::Path, request: Value) -> std::io::Result<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        let reply =
            tokio::time::timeout_at(deadline, crate::Connection::trade(path, request.clone()))
                .await??;
        if reply["error"]["reason"] != "trade_busy" && reply["error"]["reason"] != "prediction_busy"
        {
            return Ok(reply);
        }
        tokio::time::timeout_at(deadline, tokio::time::sleep(Duration::from_secs(1))).await?;
    }
}

fn error_message(reply: &Value) -> String {
    let error = &reply["error"];
    error
        .as_str()
        .or_else(|| error["message"].as_str())
        .or_else(|| error["reason"].as_str())
        .unwrap_or("Trade request failed")
        .replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn busy_retries_but_upstream_cooldowns_are_returned() {
        for reason in [
            "trade_busy",
            "prediction_busy",
            "rate_limited",
            "upstream_unavailable",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("daemon.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let server = tokio::spawn(async move {
                let attempts = if reason.ends_with("_busy") { 2 } else { 1 };
                for attempt in 0..attempts {
                    let (socket, _) = listener.accept().await.unwrap();
                    let (reader, mut writer) = socket.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut line = String::new();
                    reader.read_line(&mut line).await.unwrap();
                    let hello = json!({"id":1,"ok":true,"compatible":true,"envelope":2,
                        "game":"poe2","interfaces":{"session":2,"trade":5}});
                    writer
                        .write_all(format!("{hello}\n").as_bytes())
                        .await
                        .unwrap();
                    line.clear();
                    reader.read_line(&mut line).await.unwrap();
                    let request: Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(request["op"], "trade_leagues");
                    let reply = if attempt == 0 {
                        json!({"id":2,"ok":false,"error":{"reason":reason,"retry_after_seconds":30}})
                    } else {
                        json!({"id":2,"ok":true,"dataset":"trade","data":{"leagues":[]}})
                    };
                    writer
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            });
            let reply = tokio::time::timeout(
                Duration::from_secs(5),
                trade_request(&path, json!({"op":"trade_leagues"})),
            )
            .await
            .unwrap()
            .unwrap();
            if reason.ends_with("_busy") {
                assert_eq!(reply["ok"], true);
            } else {
                assert_eq!(reply["error"]["reason"], reason);
                assert_eq!(reply["error"]["retry_after_seconds"], 30);
            }
            server.await.unwrap();
        }
    }

    #[test]
    fn shutdown_cancels_copy_and_joins_before_returning() {
        let (wake, _source) = retinue_ui::schedule::make_ping().unwrap();
        let worker = Worker::start(wake).unwrap();
        worker.submit(Job::Copy(1, None)).unwrap();
        let start = std::time::Instant::now();
        drop(worker);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn cancelled_copy_does_not_read_the_desktop() {
        let (budget, _) = watch::channel(None);
        let reply = perform(Job::Copy(7, None), Arc::new(AtomicBool::new(true)), &budget).await;
        assert!(
            matches!(reply, Some(Reply::Error(7, message, 0)) if message == "item copy cancelled")
        );
    }
}
