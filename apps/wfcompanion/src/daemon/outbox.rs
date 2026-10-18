use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc::TryRecvError};

use serde_json::Value;
use tokio::sync::Notify;

use super::{Outbound, RequestReply};
use crate::runtime::inbox::value_bytes;

pub(super) const MAX_MESSAGES: usize = 64;
pub(super) const MAX_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct Sender(Arc<Shared>);
pub(crate) struct Receiver(Arc<Shared>);

struct Shared {
    state: Mutex<State>,
    ready: Notify,
    cancelled: Arc<AtomicBool>,
}

struct State {
    messages: VecDeque<(Outbound, usize)>,
    bytes: usize,
    senders: usize,
    closed: bool,
}

pub(crate) fn channel() -> (Sender, Receiver) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            messages: VecDeque::new(),
            bytes: 0,
            senders: 1,
            closed: false,
        }),
        ready: Notify::new(),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    (Sender(shared.clone()), Receiver(shared))
}

impl Sender {
    pub(crate) fn send(&self, message: Outbound) -> Result<(), &'static str> {
        let size = memory_cost(&message);
        let mut state = self.0.state.lock().unwrap();
        let replacement = publication_key(&message).and_then(|key| {
            state
                .messages
                .iter()
                .position(|(old, _)| publication_key(old) == Some(key))
        });
        let replaced_bytes = replacement.map_or(0, |index| state.messages[index].1);
        let failure = if state.closed {
            Some("daemon connection worker stopped")
        } else if message.reply().is_some() && self.0.cancelled.load(Ordering::Acquire) {
            Some("companion stopping")
        } else if state.messages.len() - usize::from(replacement.is_some()) >= MAX_MESSAGES
            || size > MAX_BYTES.saturating_sub(state.bytes - replaced_bytes)
        {
            Some("daemon outbound queue is full")
        } else {
            None
        };
        if let Some(reason) = failure {
            drop(state);
            reject(message, reason);
            return Err(reason);
        }
        let replaced = replacement.and_then(|index| state.messages.remove(index));
        state.bytes = state.bytes - replaced_bytes + size;
        state.messages.push_back((message, size));
        drop(state);
        drop(replaced);
        self.0.ready.notify_one();
        Ok(())
    }

    pub(crate) fn cancel_requests(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        self.0.ready.notify_one();
    }

    pub(super) fn cancellation(&self) -> Arc<AtomicBool> {
        self.0.cancelled.clone()
    }

    #[cfg(test)]
    pub(crate) fn is_closed(&self) -> bool {
        self.0.state.lock().unwrap().closed
    }
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        self.0.state.lock().unwrap().senders += 1;
        Self(self.0.clone())
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().senders -= 1;
        self.0.ready.notify_one();
    }
}

impl Receiver {
    pub(crate) fn try_recv(&mut self) -> Result<Outbound, TryRecvError> {
        let mut state = self.0.state.lock().unwrap();
        if let Some((message, size)) = state.messages.pop_front() {
            state.bytes -= size;
            Ok(message)
        } else if state.closed || state.senders == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    pub(super) async fn recv(&mut self) -> Option<Outbound> {
        loop {
            match self.try_recv() {
                Ok(message) => return Some(message),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => self.0.ready.notified().await,
            }
        }
    }

    pub(super) fn close(&mut self) {
        self.0.state.lock().unwrap().closed = true;
        self.0.ready.notify_one();
    }

    #[cfg(test)]
    pub(crate) fn blocking_recv(&mut self) -> Option<Outbound> {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(self.recv())
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.closed = true;
        state.bytes = 0;
        let messages = std::mem::take(&mut state.messages);
        drop(state);
        drop(messages);
    }
}

fn publication_key(message: &Outbound) -> Option<super::PublicationKey> {
    match message {
        Outbound::Publish {
            dataset, source, ..
        } if snapshot(dataset, source) => Some((dataset, source)),
        _ => None,
    }
}

pub(super) fn snapshot(dataset: &str, source: &str) -> bool {
    snapshot_key(dataset, source).is_some()
}

pub(super) fn snapshot_key(dataset: &str, source: &str) -> Option<super::PublicationKey> {
    [
        ("player", "game"),
        ("player", "inventory_http"),
        ("player", "inventory_native"),
        ("player", "account"),
        ("player", "collector"),
        ("player", "capture"),
        ("game_metadata", "warframe"),
        ("checks", "adapters"),
    ]
    .into_iter()
    .find(|key| *key == (dataset, source))
}

pub(super) fn reject(message: Outbound, reason: &'static str) {
    if let Some(reply) = message.reply() {
        reply.send(Err(reason.to_owned()));
    } else {
        crate::incident::warn("daemon.outbound_rejected", reason);
    }
}

impl Outbound {
    pub(super) fn reply(&self) -> Option<&RequestReply> {
        match self {
            Self::DatasetGet { reply, .. }
            | Self::DucatInventory { reply }
            | Self::MarketQuote { reply, .. }
            | Self::MarketResolve { reply, .. }
            | Self::AssetResolve { reply, .. }
            | Self::RelicContext { reply, .. }
            | Self::RelicRecommendations { reply, .. } => Some(reply),
            _ => None,
        }
    }
}

pub(super) fn memory_cost(message: &Outbound) -> usize {
    size_of::<Outbound>()
        + match message {
            Outbound::Publish { data, .. } => value_bytes(data),
            Outbound::DiagnosticsReport { issues }
            | Outbound::AssetResolve { assets: issues, .. } => {
                issues.capacity() * size_of::<Value>()
                    + issues.iter().map(value_bytes).sum::<usize>()
            }
            Outbound::MarketResolve { labels, .. }
            | Outbound::MarketQuote { items: labels, .. }
            | Outbound::RelicContext { items: labels, .. } => {
                labels.capacity() * size_of::<String>()
                    + labels.iter().map(String::capacity).sum::<usize>()
            }
            Outbound::RelicRecommendations { era, .. } => era.capacity(),
            Outbound::DatasetGet { .. } | Outbound::DucatInventory { .. } => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish(source: &'static str, data: Value) -> Outbound {
        Outbound::Publish {
            dataset: "player",
            source,
            data,
        }
    }

    #[tokio::test]
    async fn coalesces_state_but_preserves_terminal_outcomes_and_close_drains() {
        let (sender, mut receiver) = channel();
        for count in 0..1000 {
            sender
                .send(publish("inventory_http", Value::from(count)))
                .unwrap();
        }
        for count in 0..2 {
            sender
                .send(publish("capture_result", Value::from(count)))
                .unwrap();
        }
        assert_eq!(sender.0.state.lock().unwrap().messages.len(), 3);
        drop(sender);
        for expected in [999, 0, 1] {
            let Outbound::Publish { data, .. } = receiver.recv().await.unwrap() else {
                panic!()
            };
            assert_eq!(data, expected);
        }
        assert!(receiver.recv().await.is_none());
    }

    #[test]
    fn oversized_replacement_does_not_discard_accepted_state() {
        let (sender, mut receiver) = channel();
        sender
            .send(publish("inventory_http", Value::from(1)))
            .unwrap();
        assert!(
            sender
                .send(publish(
                    "inventory_http",
                    Value::String("x".repeat(MAX_BYTES))
                ))
                .is_err()
        );
        let Outbound::Publish { data, .. } = receiver.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(data, 1);
    }

    #[test]
    fn full_queue_fails_request_without_blocking() {
        let (sender, _receiver) = channel();
        for count in 0..MAX_MESSAGES {
            sender
                .send(publish("capture_result", Value::from(count)))
                .unwrap();
        }
        let (reply, result) = std::sync::mpsc::channel();
        assert!(
            sender
                .send(Outbound::DatasetGet {
                    dataset: "player",
                    reply: RequestReply::new(reply)
                })
                .is_err()
        );
        assert_eq!(
            result.try_recv().unwrap(),
            Err("daemon outbound queue is full".into())
        );
    }
}
