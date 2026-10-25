use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

pub(crate) trait Message {
    fn bytes(&self) -> usize;
    fn replaces(&self, _queued: &Self) -> bool {
        false
    }
    fn control(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Limit {
    pub(crate) items: usize,
    pub(crate) bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct Stats {
    pub(crate) items: usize,
    pub(crate) bytes: usize,
    pub(crate) peak_items: usize,
    pub(crate) peak_bytes: usize,
    pub(crate) coalesced: u64,
    pub(crate) rejected: u64,
}

pub(crate) struct Sender<T>(Arc<Shared<T>>);
pub(crate) struct Receiver<T>(Arc<Shared<T>>);

struct Shared<T> {
    name: &'static str,
    limits: [Limit; 2],
    state: Mutex<State<T>>,
    ready: Condvar,
}

struct State<T> {
    messages: VecDeque<(T, usize, usize)>,
    used: [Limit; 2],
    stats: Stats,
    senders: usize,
    closed: bool,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

pub(crate) fn channel<T>(
    name: &'static str,
    data: Limit,
    control: Limit,
) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        name,
        limits: [data, control],
        state: Mutex::new(State {
            messages: VecDeque::new(),
            used: [Limit { items: 0, bytes: 0 }; 2],
            stats: Stats::default(),
            senders: 1,
            closed: false,
            wake: None,
        }),
        ready: Condvar::new(),
    });
    (Sender(shared.clone()), Receiver(shared))
}

impl<T: Message> Sender<T> {
    pub(crate) fn send(&self, message: T) -> Result<(), &'static str> {
        let size = message.bytes();
        let class = usize::from(message.control());
        let limit = self.0.limits[class];
        let mut state = self.0.state.lock().unwrap();
        if state.closed {
            return Err("inbox closed");
        }
        let replacement = state
            .messages
            .iter()
            .position(|(old, _, old_class)| *old_class == class && message.replaces(old));
        let replaced_bytes = replacement.map_or(0, |index| state.messages[index].1);
        let used = state.used[class];
        if used.items - usize::from(replacement.is_some()) >= limit.items
            || size > limit.bytes.saturating_sub(used.bytes - replaced_bytes)
        {
            state.stats.rejected += 1;
            drop(state);
            crate::incident::warn("runtime.inbox_full", self.0.name);
            return Err("inbox capacity exceeded");
        }
        // Append replacements: retaining the old position would reorder observations.
        let replaced = replacement.and_then(|index| state.messages.remove(index));
        if replaced.is_some() {
            state.stats.coalesced += 1;
        }
        state.used[class].items = used.items + 1 - usize::from(replaced.is_some());
        state.used[class].bytes = used.bytes - replaced_bytes + size;
        state.messages.push_back((message, size, class));
        state.stats.items = state.messages.len();
        state.stats.bytes = state.stats.bytes - replaced_bytes + size;
        state.stats.peak_items = state.stats.peak_items.max(state.stats.items);
        state.stats.peak_bytes = state.stats.peak_bytes.max(state.stats.bytes);
        let wake = state.wake.clone();
        drop(state);
        self.0.ready.notify_one();
        if let Some(wake) = wake {
            wake();
        }
        drop(replaced);
        Ok(())
    }
}

impl<T> Sender<T> {
    pub(crate) fn stats(&self) -> Stats {
        self.0.state.lock().unwrap().stats
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.0.state.lock().unwrap().senders += 1;
        Self(self.0.clone())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.senders -= 1;
        let wake = state.wake.clone().filter(|_| state.senders == 0);
        drop(state);
        self.0.ready.notify_one();
        if let Some(wake) = wake {
            wake();
        }
    }
}

impl<T> State<T> {
    fn pop(&mut self) -> Option<T> {
        let (message, bytes, class) = self.messages.pop_front()?;
        self.used[class].items -= 1;
        self.used[class].bytes -= bytes;
        self.stats.items -= 1;
        self.stats.bytes -= bytes;
        Some(message)
    }
}

impl<T> Receiver<T> {
    pub(crate) fn set_waker(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        let mut state = self.0.state.lock().unwrap();
        state.wake = wake.clone();
        let ready = !state.messages.is_empty() || state.senders == 0;
        drop(state);
        if ready && let Some(wake) = wake {
            wake();
        }
    }

    pub(crate) fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        let mut state = self.0.state.lock().unwrap();
        state.pop().ok_or(if state.senders == 0 {
            mpsc::TryRecvError::Disconnected
        } else {
            mpsc::TryRecvError::Empty
        })
    }

    pub(crate) fn recv_timeout(&self, timeout: Duration) -> Result<T, mpsc::RecvTimeoutError> {
        let state = self.0.state.lock().unwrap();
        let (mut state, _) = self
            .0
            .ready
            .wait_timeout_while(state, timeout, |state| {
                state.messages.is_empty() && state.senders != 0
            })
            .unwrap();
        state.pop().ok_or(if state.senders == 0 {
            mpsc::RecvTimeoutError::Disconnected
        } else {
            mpsc::RecvTimeoutError::Timeout
        })
    }

    pub(crate) fn drain(&self, max: usize) -> Vec<T> {
        let mut state = self.0.state.lock().unwrap();
        let messages = (0..max).map_while(|_| state.pop()).collect();
        let wake = state.wake.clone().filter(|_| !state.messages.is_empty());
        drop(state);
        if let Some(wake) = wake {
            wake();
        }
        messages
    }

    #[cfg(test)]
    pub(crate) fn recv(&self) -> Result<T, mpsc::RecvTimeoutError> {
        self.recv_timeout(Duration::from_secs(5))
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.closed = true;
        state.stats.items = 0;
        state.stats.bytes = 0;
        let messages = std::mem::take(&mut state.messages);
        drop(state);
        drop(messages);
    }
}

// Estimated retained-value accounting, not a process RSS limit.
pub(crate) fn value_bytes(value: &Value) -> usize {
    size_of::<Value>()
        + match value {
            Value::String(text) => text.capacity(),
            Value::Array(items) => {
                items.capacity() * size_of::<Value>() + items.iter().map(value_bytes).sum::<usize>()
            }
            Value::Object(items) => items
                .iter()
                .map(|(key, value)| {
                    key.capacity()
                        + size_of::<String>()
                        + 4 * size_of::<usize>()
                        + value_bytes(value)
                })
                .sum(),
            _ => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Entry(u32, usize, bool);

    impl Message for Entry {
        fn bytes(&self) -> usize {
            self.1
        }
        fn control(&self) -> bool {
            self.2
        }
        fn replaces(&self, queued: &Self) -> bool {
            !self.2 && self.0 == queued.0
        }
    }

    fn test_channel() -> (Sender<Entry>, Receiver<Entry>) {
        channel(
            "test",
            Limit { items: 2, bytes: 8 },
            Limit { items: 2, bytes: 4 },
        )
    }

    #[test]
    fn waker_covers_existing_messages_partial_drains_and_disconnect() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (sender, receiver) = test_channel();
        sender.send(Entry(1, 1, false)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        receiver.set_waker(Some(Arc::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
        })));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        sender.send(Entry(2, 1, false)).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(receiver.drain(1), vec![Entry(1, 1, false)]);
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert_eq!(receiver.drain(1), vec![Entry(2, 1, false)]);
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        drop(sender);
        assert_eq!(calls.load(Ordering::Relaxed), 4);
        receiver.set_waker(None);
        assert!(receiver.drain(1).is_empty());
        assert_eq!(calls.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn replacement_appends_without_reordering_controls() {
        let (sender, receiver) = test_channel();
        sender.send(Entry(1, 4, false)).unwrap();
        sender.send(Entry(2, 1, true)).unwrap();
        for _ in 0..1000 {
            sender.send(Entry(1, 3, false)).unwrap();
        }
        assert_eq!(sender.stats().items, 2);
        assert_eq!(sender.stats().coalesced, 1000);
        assert_eq!(receiver.drain(10), [Entry(2, 1, true), Entry(1, 3, false)]);
        assert_eq!(sender.stats().bytes, 0);
    }

    #[test]
    fn separate_budgets_reserve_controls_and_preserve_oversized_replacement() {
        let (sender, receiver) = test_channel();
        sender.send(Entry(1, 8, false)).unwrap();
        assert!(sender.send(Entry(1, 9, false)).is_err());
        assert!(sender.send(Entry(2, 1, false)).is_err());
        sender.send(Entry(2, 1, true)).unwrap();
        sender.send(Entry(3, 1, true)).unwrap();
        assert!(sender.send(Entry(4, 1, true)).is_err());
        assert_eq!(sender.stats().rejected, 3);
        assert_eq!(sender.stats().bytes, 10);
        assert_eq!(receiver.drain(1), [Entry(1, 8, false)]);
        sender.send(Entry(4, 1, false)).unwrap();
        assert_eq!(
            receiver.drain(10),
            [Entry(2, 1, true), Entry(3, 1, true), Entry(4, 1, false)]
        );
    }

    #[test]
    fn close_is_nonblocking_and_last_sender_wakes_receiver() {
        let (sender, receiver) = test_channel();
        let other = sender.clone();
        drop(sender);
        other.send(Entry(1, 1, false)).unwrap();
        drop(other);
        assert_eq!(receiver.recv().unwrap(), Entry(1, 1, false));
        assert_eq!(receiver.recv(), Err(mpsc::RecvTimeoutError::Disconnected));
        let (sender, receiver) = test_channel();
        let worker = std::thread::spawn(move || receiver.recv_timeout(Duration::from_secs(30)));
        drop(sender);
        assert_eq!(
            worker.join().unwrap(),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
        let (sender, receiver) = test_channel();
        drop(receiver);
        assert_eq!(sender.send(Entry(1, 1, false)), Err("inbox closed"));
    }

    #[test]
    fn count_limit_and_concurrent_publishers_keep_controls_ordered() {
        let (sender, receiver) = test_channel();
        sender.send(Entry(1, 1, false)).unwrap();
        sender.send(Entry(2, 1, false)).unwrap();
        assert!(sender.send(Entry(3, 1, false)).is_err());
        let workers: Vec<_> = (1..=2)
            .map(|id| {
                let sender = sender.clone();
                std::thread::spawn(move || {
                    for _ in 0..1000 {
                        sender.send(Entry(id, 1, false)).unwrap();
                    }
                })
            })
            .collect();
        sender.send(Entry(3, 1, true)).unwrap();
        sender.send(Entry(4, 1, true)).unwrap();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(sender.stats().items, 4);
        assert_eq!(sender.stats().coalesced, 2000);
        let controls: Vec<_> = receiver
            .drain(10)
            .into_iter()
            .filter(|event| event.2)
            .collect();
        assert_eq!(controls, [Entry(3, 1, true), Entry(4, 1, true)]);
    }
}
