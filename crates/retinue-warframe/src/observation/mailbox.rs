use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, mpsc::RecvTimeoutError};
use std::time::Duration;

use serde::Serialize;

#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct Stats {
    pub items: usize,
    pub bytes: usize,
    pub peak_items: usize,
    pub peak_bytes: usize,
    pub dropped_items: u64,
    pub dropped_bytes: u64,
}

pub enum SendError {
    Closed,
    Oversized,
}

pub struct Mailbox<T> {
    max_items: usize,
    max_bytes: usize,
    state: Mutex<State<T>>,
    ready: Condvar,
}

struct State<T> {
    items: VecDeque<(T, usize)>,
    stats: Stats,
    closed: bool,
}

impl<T> Mailbox<T> {
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        assert!(max_items > 0 && max_bytes > 0);
        Self {
            max_items,
            max_bytes,
            state: Mutex::new(State {
                items: VecDeque::new(),
                stats: Stats::default(),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    // Never wait for consumer capacity. Retain recent entries and account for gaps.
    pub fn send(&self, item: T, bytes: usize) -> Result<usize, SendError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(SendError::Closed);
        }
        if bytes > self.max_bytes {
            state.stats.dropped_items += 1;
            state.stats.dropped_bytes += bytes as u64;
            return Err(SendError::Oversized);
        }
        let mut evicted = Vec::new();
        while state.items.len() >= self.max_items || state.stats.bytes > self.max_bytes - bytes {
            let (item, size) = state.items.pop_front().unwrap();
            state.stats.bytes -= size;
            state.stats.dropped_items += 1;
            state.stats.dropped_bytes += size as u64;
            evicted.push(item);
        }
        state.items.push_back((item, bytes));
        state.stats.bytes += bytes;
        state.stats.items = state.items.len();
        state.stats.peak_items = state.stats.peak_items.max(state.stats.items);
        state.stats.peak_bytes = state.stats.peak_bytes.max(state.stats.bytes);
        drop(state);
        self.ready.notify_one();
        Ok(evicted.len())
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let state = self.state.lock().unwrap();
        let (mut state, _) = self
            .ready
            .wait_timeout_while(state, timeout, |state| {
                state.items.is_empty() && !state.closed
            })
            .unwrap();
        if let Some((item, bytes)) = state.items.pop_front() {
            state.stats.items = state.items.len();
            state.stats.bytes -= bytes;
            Ok(item)
        } else if state.closed {
            Err(RecvTimeoutError::Disconnected)
        } else {
            Err(RecvTimeoutError::Timeout)
        }
    }

    pub fn stats(&self) -> Stats {
        self.state.lock().unwrap().stats
    }

    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn limits_count_and_bytes_without_reordering_retained_entries() {
        let queue = Mailbox::new(2, 5);
        assert!(matches!(queue.send(1, 2), Ok(0)));
        assert!(matches!(queue.send(2, 2), Ok(0)));
        assert!(matches!(queue.send(3, 2), Ok(1)));
        assert!(matches!(queue.send(4, 4), Ok(2)));
        assert_eq!(queue.recv_timeout(Duration::ZERO), Ok(4));
        let stats = queue.stats();
        assert_eq!((stats.items, stats.bytes), (0, 0));
        assert_eq!((stats.peak_items, stats.peak_bytes), (2, 4));
        assert_eq!((stats.dropped_items, stats.dropped_bytes), (3, 6));
    }

    #[test]
    fn oversized_entries_do_not_destroy_the_existing_queue() {
        let queue = Mailbox::new(2, 5);
        assert!(queue.send(1, 2).is_ok());
        assert!(matches!(queue.send(2, 6), Err(SendError::Oversized)));
        assert_eq!(queue.recv_timeout(Duration::ZERO), Ok(1));
        assert_eq!(queue.stats().dropped_items, 1);
    }

    #[test]
    fn close_wakes_consumer_and_drains_accepted_work() {
        let queue = Arc::new(Mailbox::new(2, 5));
        assert!(queue.send(1, 2).is_ok());
        queue.close();
        assert!(matches!(queue.send(2, 2), Err(SendError::Closed)));
        assert_eq!(queue.recv_timeout(Duration::ZERO), Ok(1));
        assert_eq!(
            queue.recv_timeout(Duration::ZERO),
            Err(RecvTimeoutError::Disconnected)
        );
        let queue = Arc::new(Mailbox::<u32>::new(2, 5));
        let reader = queue.clone();
        let worker = std::thread::spawn(move || reader.recv_timeout(Duration::from_secs(30)));
        queue.close();
        assert_eq!(worker.join().unwrap(), Err(RecvTimeoutError::Disconnected));
    }
}
