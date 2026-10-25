use std::{
    fmt, io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

pub struct PreparationToken<'a> {
    revision: &'a AtomicU64,
    expected: u64,
}

impl PreparationToken<'_> {
    pub fn is_current(&self) -> bool {
        self.revision.load(Ordering::Acquire) == self.expected
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationFailed;

impl fmt::Display for PreparationFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("resource preparation worker stopped unexpectedly")
    }
}

impl std::error::Error for PreparationFailed {}

struct Mailbox<I, O> {
    pending: Option<(u64, I)>,
    ready: Option<O>,
    stopped: bool,
    failed: bool,
}

struct Shared<I, O> {
    mailbox: Mutex<Mailbox<I, O>>,
    changed: Condvar,
    revision: AtomicU64,
}

impl<I, O> Shared<I, O> {
    fn lock(&self) -> MutexGuard<'_, Mailbox<I, O>> {
        self.mailbox.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One running job, one latest pending job and one latest result. No idle polling.
///
/// Callers bound input/output sizes and check the token between bounded operations.
/// Dropping the worker cancels outstanding work and joins before returning.
pub struct Preparation<I, O> {
    shared: Arc<Shared<I, O>>,
    worker: Option<JoinHandle<()>>,
}

impl<I: Send + 'static, O: Send + 'static> Preparation<I, O> {
    /// `notify` wakes the consumer; it must be brief and must not panic.
    pub fn spawn(
        name: &str,
        mut prepare: impl FnMut(I, &PreparationToken<'_>) -> O + Send + 'static,
        notify: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        Self::spawn_streaming(
            name,
            move |input, token, publish| publish(prepare(input, token)),
            |previous, next| *previous = next,
            notify,
        )
    }

    /// Partial outputs share one mailbox. `merge` must keep that mailbox bounded.
    pub fn spawn_streaming(
        name: &str,
        mut prepare: impl FnMut(I, &PreparationToken<'_>, &mut dyn FnMut(O)) + Send + 'static,
        merge: impl Fn(&mut O, O) + Send + 'static,
        notify: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            mailbox: Mutex::new(Mailbox {
                pending: None,
                ready: None,
                stopped: false,
                failed: false,
            }),
            changed: Condvar::new(),
            revision: AtomicU64::new(0),
        });
        let state = shared.clone();
        let worker = thread::Builder::new().name(name.into()).spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                loop {
                    let (revision, input) = {
                        let mut mailbox = state
                            .changed
                            .wait_while(state.lock(), |m| m.pending.is_none() && !m.stopped)
                            .unwrap_or_else(|e| e.into_inner());
                        if mailbox.stopped {
                            break;
                        }
                        mailbox.pending.take().unwrap()
                    };
                    let token = PreparationToken {
                        revision: &state.revision,
                        expected: revision,
                    };
                    if !token.is_current() {
                        continue;
                    }
                    prepare(input, &token, &mut |output| {
                        let mut mailbox = state.lock();
                        if mailbox.stopped || !token.is_current() {
                            return;
                        }
                        let wake = mailbox.ready.is_none();
                        if let Some(previous) = &mut mailbox.ready {
                            merge(previous, output);
                        } else {
                            mailbox.ready = Some(output);
                        }
                        drop(mailbox);
                        if wake {
                            notify();
                        }
                    });
                }
            }));
            if result.is_err() {
                // Do not reuse preparation state after unwinding through it.
                let mut mailbox = state.lock();
                mailbox.stopped = true;
                mailbox.failed = true;
                let pending = mailbox.pending.take();
                let ready = mailbox.ready.take();
                drop(mailbox);
                drop((pending, ready));
                notify();
            }
        })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn request(&mut self, input: I) -> Result<(), PreparationFailed> {
        let mut mailbox = self.shared.lock();
        if mailbox.failed {
            return Err(PreparationFailed);
        }
        let revision = self.shared.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let pending = mailbox.pending.replace((revision, input));
        let ready = mailbox.ready.take();
        drop(mailbox);
        self.shared.changed.notify_one();
        drop((pending, ready));
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<O>, PreparationFailed> {
        let mut mailbox = self.shared.lock();
        if mailbox.failed {
            Err(PreparationFailed)
        } else {
            Ok(mailbox.ready.take())
        }
    }
}

impl<I, O> Drop for Preparation<I, O> {
    fn drop(&mut self) {
        let mut mailbox = self.shared.lock();
        mailbox.stopped = true;
        self.shared.revision.fetch_add(1, Ordering::AcqRel);
        let pending = mailbox.pending.take();
        let ready = mailbox.ready.take();
        drop(mailbox);
        self.shared.changed.notify_one();
        drop((pending, ready));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn publishes_before_completion_coalesces_wakes_and_discards_stale_progress() {
        let (release, gate) = mpsc::channel();
        let (wake, ready) = mpsc::channel();
        let (progress, reached) = mpsc::channel();
        let mut worker = Preparation::spawn_streaming(
            "stream-test",
            move |job: u32, _, publish| {
                publish(vec![job * 10]);
                if job == 1 {
                    gate.recv_timeout(TIMEOUT).unwrap();
                    publish(vec![11]);
                    publish(vec![12]);
                    progress.send(()).unwrap();
                    gate.recv_timeout(TIMEOUT).unwrap();
                    publish(vec![13]);
                }
            },
            |pending: &mut Vec<u32>, next| pending.extend(next),
            move || wake.send(()).unwrap(),
        )
        .unwrap();
        worker.request(1).unwrap();
        ready.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(worker.poll().unwrap(), Some(vec![10]));
        release.send(()).unwrap();
        reached.recv_timeout(TIMEOUT).unwrap();
        ready.recv_timeout(TIMEOUT).unwrap();
        assert!(ready.try_recv().is_err());
        assert_eq!(worker.poll().unwrap(), Some(vec![11, 12]));
        worker.request(2).unwrap();
        release.send(()).unwrap();
        ready.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(worker.poll().unwrap(), Some(vec![20]));
    }

    #[test]
    fn coalesces_pending_work_and_discards_running_stale_output() {
        let (started, starts) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let (ready, results) = mpsc::channel();
        let mut worker = Preparation::spawn(
            "prepare-test",
            move |input, token| {
                started.send(input).unwrap();
                if input == 1 {
                    gate.recv_timeout(TIMEOUT).unwrap();
                    assert!(!token.is_current());
                }
                input
            },
            move || ready.send(()).unwrap(),
        )
        .unwrap();
        worker.request(1).unwrap();
        assert_eq!(starts.recv_timeout(TIMEOUT).unwrap(), 1);
        worker.request(2).unwrap();
        worker.request(3).unwrap();
        release.send(()).unwrap();
        assert_eq!(starts.recv_timeout(TIMEOUT).unwrap(), 3);
        results.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(worker.poll().unwrap(), Some(3));
        assert!(starts.try_recv().is_err());
        assert!(results.try_recv().is_err());
    }

    #[test]
    fn unread_completion_does_not_block_new_work() {
        let (ready, results) = mpsc::channel();
        let mut worker = Preparation::spawn(
            "prepare-test",
            |input, _| input,
            move || ready.send(()).unwrap(),
        )
        .unwrap();
        for input in 0..10 {
            worker.request(input).unwrap();
            results.recv_timeout(TIMEOUT).unwrap();
        }
        assert_eq!(worker.poll().unwrap(), Some(9));
        assert_eq!(worker.poll().unwrap(), None);
    }

    #[test]
    fn new_request_removes_an_already_ready_stale_result() {
        let (ready, results) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut worker = Preparation::spawn(
            "prepare-test",
            move |input, _| {
                if input == 2 {
                    gate.recv_timeout(TIMEOUT).unwrap();
                }
                input
            },
            move || ready.send(()).unwrap(),
        )
        .unwrap();
        worker.request(1).unwrap();
        results.recv_timeout(TIMEOUT).unwrap();
        worker.request(2).unwrap();
        assert_eq!(worker.poll().unwrap(), None);
        release.send(()).unwrap();
        results.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(worker.poll().unwrap(), Some(2));
    }

    #[test]
    fn shutdown_cancels_running_work_drops_pending_and_joins() {
        let (started, starts) = mpsc::channel();
        let (finished, finishes) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut worker = Preparation::spawn(
            "prepare-test",
            move |input, token| {
                started.send(input).unwrap();
                gate.recv_timeout(TIMEOUT).unwrap();
                assert!(!token.is_current());
                finished.send(()).unwrap();
                input
            },
            || panic!("cancelled work must not notify"),
        )
        .unwrap();
        worker.request(1).unwrap();
        starts.recv_timeout(TIMEOUT).unwrap();
        worker.request(2).unwrap();
        let state = worker.shared.clone();
        let (stopped, stops) = mpsc::channel();
        let shutdown = thread::spawn(move || {
            drop(worker);
            stopped.send(()).unwrap();
        });
        let deadline = std::time::Instant::now() + TIMEOUT;
        while !state.lock().stopped {
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(stops.try_recv().is_err());
        release.send(()).unwrap();
        stops.recv_timeout(TIMEOUT).unwrap();
        shutdown.join().unwrap();
        finishes.recv_timeout(TIMEOUT).unwrap();
        assert!(starts.try_recv().is_err());
        assert!(!state.lock().failed);
    }

    #[test]
    fn idle_worker_shuts_down_and_releases_its_state() {
        let state = Arc::new(());
        let held = state.clone();
        let worker = Preparation::spawn(
            "prepare-test",
            move |(): (), _| {
                let _ = &held;
            },
            || {},
        )
        .unwrap();
        drop(worker);
        assert_eq!(Arc::strong_count(&state), 1);
    }

    #[test]
    fn panic_wakes_consumer_and_rejects_further_work() {
        let (ready, results) = mpsc::channel();
        let mut worker: Preparation<(), ()> = Preparation::spawn(
            "prepare-test",
            |(), _| panic!("decode failed"),
            move || ready.send(()).unwrap(),
        )
        .unwrap();
        worker.request(()).unwrap();
        results.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(worker.poll(), Err(PreparationFailed));
        assert_eq!(worker.request(()), Err(PreparationFailed));
    }
}
