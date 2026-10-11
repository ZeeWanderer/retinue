use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub duration: Duration,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Budget(Arc<State>);

#[derive(Debug)]
struct State {
    started: Instant,
    limits: Limits,
    cancelled: AtomicBool,
    exhausted: AtomicU8,
    read: AtomicU64,
    written: AtomicU64,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub elapsed_ms: u128,
    pub timeout_ms: u128,
    pub read_bytes_reserved: u64,
    pub write_bytes_reserved: u64,
    pub max_read_bytes: u64,
    pub max_write_bytes: u64,
}

impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self(Arc::new(State {
            started: Instant::now(),
            limits,
            cancelled: AtomicBool::new(false),
            exhausted: AtomicU8::new(0),
            read: AtomicU64::new(0),
            written: AtomicU64::new(0),
        }))
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> io::Result<()> {
        let (kind, reason) = if self.cancelled() {
            (io::ErrorKind::ConnectionAborted, "capture cancelled")
        } else if self.0.started.elapsed() >= self.0.limits.duration {
            (io::ErrorKind::TimedOut, "capture deadline exceeded")
        } else {
            match self.0.exhausted.load(Ordering::Relaxed) {
                1 => (io::ErrorKind::Other, "capture read budget exceeded"),
                2 => (io::ErrorKind::Other, "capture output budget exceeded"),
                _ => return Ok(()),
            }
        };
        Err(io::Error::new(kind, reason))
    }

    pub fn remaining(&self) -> io::Result<Duration> {
        self.check()?;
        Ok(self
            .0
            .limits
            .duration
            .saturating_sub(self.0.started.elapsed()))
    }

    pub fn read(&self, bytes: usize) -> io::Result<()> {
        self.reserve(&self.0.read, self.0.limits.read_bytes, bytes, 1)
    }

    pub fn write(&self, bytes: usize) -> io::Result<()> {
        self.reserve(&self.0.written, self.0.limits.write_bytes, bytes, 2)
    }

    fn reserve(
        &self,
        counter: &AtomicU64,
        limit: u64,
        bytes: usize,
        exhausted: u8,
    ) -> io::Result<()> {
        self.check()?;
        if counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes as u64).filter(|next| *next <= limit)
            })
            .is_err()
        {
            self.0.exhausted.store(exhausted, Ordering::Relaxed);
            return self.check();
        }
        Ok(())
    }

    pub fn usage(&self) -> Usage {
        Usage {
            elapsed_ms: self.0.started.elapsed().as_millis(),
            timeout_ms: self.0.limits.duration.as_millis(),
            read_bytes_reserved: self.0.read.load(Ordering::Relaxed),
            write_bytes_reserved: self.0.written.load(Ordering::Relaxed),
            max_read_bytes: self.0.limits.read_bytes,
            max_write_bytes: self.0.limits.write_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            duration: Duration::from_secs(30),
            read_bytes: 8,
            write_bytes: 4,
        }
    }

    #[test]
    fn clones_share_limits_and_exhaustion_cannot_be_ignored() {
        let budget = Budget::new(limits());
        budget.read(4).unwrap();
        budget.clone().read(4).unwrap();
        assert!(budget.read(1).is_err());
        assert!(budget.check().is_err());
        assert!(budget.read(0).is_err());
        assert_eq!(budget.usage().read_bytes_reserved, 8);
        let budget = Budget::new(limits());
        budget.write(4).unwrap();
        assert!(budget.write(1).is_err());
        assert_eq!(budget.usage().write_bytes_reserved, 4);
    }

    #[test]
    fn cancellation_and_deadline_stop_work() {
        let budget = Budget::new(limits());
        budget.clone().cancel();
        assert_eq!(
            budget.check().unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        let budget = Budget::new(Limits {
            duration: Duration::ZERO,
            ..limits()
        });
        assert_eq!(budget.read(1).unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(budget.usage().read_bytes_reserved, 0);
    }
}
