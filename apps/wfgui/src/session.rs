use retinue_client::{local_protocol as protocol, paths::Paths, transport::JsonChannel};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    thread,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug)]
pub struct Request {
    pub key: String,
    pub body: Value,
}

pub enum Update {
    Connected,
    Disconnected(String),
    Reply {
        request: Request,
        result: Result<Value, String>,
    },
    Event(Value),
}

pub struct Worker {
    requests: mpsc::Sender<Request>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

pub async fn connect() -> Result<JsonChannel, String> {
    tokio::time::timeout(Duration::from_secs(8), async {
        let stream = tokio::net::UnixStream::connect(Paths::from_env().daemon_socket())
            .await
            .map_err(|e| format!("Daemon unavailable: {e}"))?;
        let mut channel = JsonChannel::new(stream, 32 * 1024 * 1024);
        let interfaces = protocol::interfaces();
        let reply = channel
            .request(&json!({"op":"hello", "id":1, "game":"warframe",
            "envelope":protocol::ENVELOPE_VERSION, "interfaces":interfaces, "features":["diagnostics.report"],
            "client":"wfgui", "version":env!("CARGO_PKG_VERSION"),
            "pid":std::process::id(), "mode":"desktop"}))
            .await
            .map_err(|e| e.to_string())?;
        if reply["ok"] != true
            || reply["compatible"] != true
            || reply["game"] != "warframe"
            || reply["envelope"] != protocol::ENVELOPE_VERSION
            || interfaces
                .iter()
                .any(|(name, version)| reply["interfaces"][name] != *version)
        {
            return Err("Daemon contract mismatch; rebuild Retinue together".into());
        }
        Ok(channel)
    })
    .await
    .map_err(|_| "Daemon connection timed out".to_owned())?
}

impl Worker {
    pub fn new(send: impl Fn(Update) -> bool + Send + 'static) -> Result<Self, String> {
        let (requests, mut receiver) = mpsc::channel(128);
        let (stop, mut stopped) = oneshot::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let thread = thread::Builder::new()
            .name("wfgui-session".into())
            .spawn(move || {
                runtime.block_on(async {
                    loop {
                        let result = tokio::select! {
                            _ = &mut stopped => break,
                            result = async {
                                let channel = connect().await?;
                                if !send(Update::Connected) { return Ok(()); }
                                run(channel, &mut receiver, &send).await
                            } => result,
                        };
                        match result {
                            Ok(()) => break,
                            Err(error) => {
                                if !send(Update::Disconnected(error)) {
                                    break;
                                }
                            }
                        }
                        while let Ok(request) = receiver.try_recv() {
                            send(Update::Reply {
                                request,
                                result: Err("Disconnected; request was not sent".into()),
                            });
                        }
                        tokio::select! { _ = &mut stopped => break,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => () }
                    }
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            requests,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    pub fn submit(&self, request: Request) -> Result<(), String> {
        self.requests
            .try_send(request)
            .map_err(|_| "Request queue is full; try again".into())
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

async fn run(
    mut channel: JsonChannel,
    receiver: &mut mpsc::Receiver<Request>,
    send: &impl Fn(Update) -> bool,
) -> Result<(), String> {
    channel
        .send(&json!({"op":"subscribe", "id":2, "dataset":"player", "include_data":false}))
        .await
        .map_err(|e| e.to_string())?;
    let mut sequence = 10u64;
    let mut pending = HashMap::<u64, (Request, tokio::time::Instant)>::new();
    let mut queued = VecDeque::<Request>::new();
    let mut subscriptions = std::collections::HashSet::from([2u64]);
    let subscribe_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let result = async {
        loop {
            while pending.len() < 8 {
                let Some(request) = queued.pop_front() else { break; };
                sequence += 1;
                let mut body = request.body.clone();
                body["id"] = json!(sequence);
                channel.send(&body).await.map_err(|e| e.to_string())?;
                pending.insert(sequence, (request, tokio::time::Instant::now() + Duration::from_secs(90)));
            }
            let deadline = pending.values().map(|(_, t)| *t)
                .chain((!subscriptions.is_empty()).then_some(subscribe_deadline)).min()
                .unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(90));
            tokio::select! {
                request = receiver.recv(), if queued.len() < 128 => {
                    match request { Some(request) => queued.push_back(request), None => return Ok(()) }
                }
                _ = tokio::time::sleep_until(deadline), if !pending.is_empty() || !subscriptions.is_empty() => {
                    let now = tokio::time::Instant::now();
                    if !subscriptions.is_empty()&&now>=subscribe_deadline{return Err("Daemon update subscription timed out".into());}
                    let expired = pending.iter().filter(|(_, (_, t))| *t <= now).map(|(id,_)|*id).collect::<Vec<_>>();
                    for id in expired {
                        let (request, _) = pending.remove(&id).unwrap();
                        if !send(Update::Reply { request, result: Err("Request timed out; check its state before retrying changes".into()) }) { return Ok(()); }
                    }
                }
                message = channel.receive() => {
                    let message = message.map_err(|e| e.to_string())?;
                    if message["id"].as_u64().is_some_and(|id|subscriptions.remove(&id)){
                        response(&message).map_err(|e|format!("Daemon update subscription failed: {e}"))?;
                        continue;
                    }
                    if let Some((request, _)) = message["id"].as_u64().and_then(|id| pending.remove(&id)) {
                        let result = response(&message);
                        if !send(Update::Reply { request, result }) { return Ok(()); }
                    } else if message.get("event").is_some() && !send(Update::Event(message)) { return Ok(()); }
                }
            }
        }
    }.await;
    for (_, (request, _)) in pending {
        send(Update::Reply {
            request,
            result: Err("Connection lost; verify state before retrying changes".into()),
        });
    }
    for request in queued {
        send(Update::Reply {
            request,
            result: Err("Disconnected; request was not sent".into()),
        });
    }
    result
}

pub fn response(reply: &Value) -> Result<Value, String> {
    if reply["ok"] == true {
        Ok(reply["data"].clone())
    } else {
        Err(reply["error"]
            .as_str()
            .unwrap_or("Request failed")
            .replace('_', " "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn interleaved_events_and_replies_keep_request_identity() {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let (sender, mut receiver) = mpsc::channel(4);
        sender
            .send(Request {
                key: "first".into(),
                body: json!({"op":"one"}),
            })
            .await
            .unwrap();
        sender
            .send(Request {
                key: "second".into(),
                body: json!({"op":"two"}),
            })
            .await
            .unwrap();
        let updates = std::sync::Mutex::new(Vec::new());
        let receive = |v| {
            updates.lock().unwrap().push(v);
            true
        };
        let server = async {
            let mut channel = JsonChannel::new(b, 65536);
            for _ in 0..1 {
                let request = channel.receive().await.unwrap();
                assert_eq!(request["op"], "subscribe");
                channel
                    .send(&json!({"id":request["id"],"ok":true}))
                    .await
                    .unwrap();
            }
            let first = channel.receive().await.unwrap();
            let second = channel.receive().await.unwrap();
            channel
                .send(&json!({"event":"dataset","dataset":"player"}))
                .await
                .unwrap();
            channel
                .send(&json!({"id":second["id"],"ok":true,"data":2}))
                .await
                .unwrap();
            channel
                .send(&json!({"id":first["id"],"ok":true,"data":1}))
                .await
                .unwrap();
        };
        let _ = tokio::join!(
            run(JsonChannel::new(a, 65536), &mut receiver, &receive),
            server
        );
        let values = updates.into_inner().unwrap();
        assert!(matches!(&values[0], Update::Event(_)));
        assert!(
            matches!(&values[1],Update::Reply{request,result:Ok(v)} if request.key=="second" && v==2)
        );
        assert!(
            matches!(&values[2],Update::Reply{request,result:Ok(v)} if request.key=="first" && v==1)
        );
    }

    #[tokio::test]
    async fn a_rejected_subscription_is_not_silently_treated_as_connected() {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let (_sender, mut receiver) = mpsc::channel(4);
        let server = async {
            let mut channel = JsonChannel::new(b, 65536);
            let request = channel.receive().await.unwrap();
            channel
                .send(&json!({"id":request["id"],"ok":false,"error":"unsupported_dataset"}))
                .await
                .unwrap();
        };
        let (result, _) = tokio::join!(
            run(JsonChannel::new(a, 65536), &mut receiver, &|_| true),
            server
        );
        assert!(result.unwrap_err().contains("subscription failed"));
    }
}
#[tokio::test]
#[ignore = "requires a running local daemon"]
async fn live_daemon_accepts_subscription_and_activity_query() {
    let channel = connect().await.unwrap();
    let (sender, mut receiver) = mpsc::channel(4);
    sender
        .send(Request {
            key: "activity".into(),
            body: json!({"op":"activity_view"}),
        })
        .await
        .unwrap();
    let replies = std::sync::Mutex::new(Vec::new());
    let receive = |update| {
        if let Update::Reply { result, .. } = update {
            replies.lock().unwrap().push(result);
            return false;
        }
        true
    };
    tokio::time::timeout(
        Duration::from_secs(10),
        run(channel, &mut receiver, &receive),
    )
    .await
    .unwrap()
    .unwrap();
    let replies = replies.into_inner().unwrap();
    assert_eq!(replies.len(), 1);
    assert!(replies[0].as_ref().unwrap()["fissures"].is_array());
}
