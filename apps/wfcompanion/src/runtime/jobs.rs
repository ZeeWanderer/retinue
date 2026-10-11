use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::join_worker;
use retinue_native::work::Budget;

pub(crate) struct Jobs {
    spawner: Spawner,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    state: Arc<Mutex<State>>,
    name: &'static str,
    limit: usize,
}

#[derive(Default)]
struct State {
    closed: bool,
    workers: Vec<(JoinHandle<()>, Budget)>,
}

impl Jobs {
    pub(crate) fn new(name: &'static str, limit: usize) -> Self {
        assert!(limit > 0);
        Self {
            spawner: Spawner {
                state: Arc::new(Mutex::new(State::default())),
                name,
                limit,
            },
        }
    }

    pub(crate) fn spawner(&self) -> Spawner {
        self.spawner.clone()
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.spawner
            .state
            .lock()
            .unwrap()
            .workers
            .iter()
            .all(|(worker, _)| worker.is_finished())
    }

    pub(crate) fn shutdown(&mut self) {
        let workers = {
            let mut state = self.spawner.state.lock().unwrap();
            state.closed = true;
            for (_, budget) in &state.workers {
                budget.cancel();
            }
            std::mem::take(&mut state.workers)
        };
        for (worker, _) in workers {
            join_worker(self.spawner.name, worker);
        }
    }

    pub(crate) fn cancel_all(&self) {
        for (_, budget) in &self.spawner.state.lock().unwrap().workers {
            budget.cancel();
        }
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Spawner {
    pub(crate) fn spawn(
        &self,
        budget: Budget,
        work: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(format!("{} jobs are stopping", self.name));
        }
        let mut index = 0;
        while index < state.workers.len() {
            if state.workers[index].0.is_finished() {
                join_worker(self.name, state.workers.swap_remove(index).0);
            } else {
                index += 1;
            }
        }
        if state.workers.len() >= self.limit {
            return Err(format!("{} job limit reached ({})", self.name, self.limit));
        }
        let worker = thread::Builder::new()
            .name(format!("wfcompanion-{}", self.name))
            .spawn(work)
            .map_err(|error| format!("could not start {} job: {error}", self.name))?;
        state.workers.push((worker, budget));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    fn budget() -> Budget {
        Budget::new(retinue_warframe::observation::ui_capture::limits())
    }

    #[test]
    fn jobs_are_bounded_and_shutdown_joins_accepted_work() {
        let mut jobs = Jobs::new("test", 1);
        assert!(jobs.is_idle());
        let spawner = jobs.spawner();
        let (release, wait) = mpsc::channel();
        let (done, result) = mpsc::channel();
        spawner
            .spawn(budget(), move || {
                wait.recv().unwrap();
                done.send(()).unwrap();
            })
            .unwrap();
        assert!(!jobs.is_idle());
        assert!(
            spawner
                .spawn(budget(), || {})
                .unwrap_err()
                .contains("limit reached")
        );
        release.send(()).unwrap();
        jobs.shutdown();
        assert!(jobs.is_idle());
        assert!(result.try_recv().is_ok());
        assert!(
            spawner
                .spawn(budget(), || {})
                .unwrap_err()
                .contains("stopping")
        );
        jobs.shutdown();
    }

    #[test]
    fn shutdown_closes_admission_before_joining_workers() {
        let jobs = Jobs::new("test", 2);
        let spawner = jobs.spawner();
        let (started, starting) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        spawner
            .spawn(budget(), move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
            })
            .unwrap();
        starting.recv_timeout(Duration::from_secs(5)).unwrap();
        let closer = thread::spawn(move || drop(jobs));
        loop {
            if spawner.state.lock().unwrap().closed {
                break;
            }
            thread::yield_now();
        }
        let denied = spawner.spawn(budget(), || {});
        release.send(()).unwrap();
        closer.join().unwrap();
        assert!(denied.unwrap_err().contains("stopping"));
    }

    #[test]
    fn completed_and_panicked_jobs_release_capacity() {
        let mut jobs = Jobs::new("test", 1);
        let spawner = jobs.spawner();
        spawner
            .spawn(budget(), || panic!("failed capture"))
            .unwrap();
        while !spawner.state.lock().unwrap().workers[0].0.is_finished() {
            thread::yield_now();
        }
        spawner.spawn(budget(), || {}).unwrap();
        jobs.shutdown();
    }

    #[test]
    fn shutdown_cancels_cooperative_work_before_joining() {
        let mut jobs = Jobs::new("test", 1);
        let control = budget();
        let worker = control.clone();
        jobs.spawner()
            .spawn(control.clone(), move || {
                while worker.check().is_ok() {
                    thread::yield_now();
                }
            })
            .unwrap();
        jobs.shutdown();
        assert!(control.cancelled());
    }

    #[test]
    fn cancellation_does_not_close_future_admission() {
        let mut jobs = Jobs::new("test", 1);
        let control = budget();
        let worker = control.clone();
        jobs.spawner()
            .spawn(control.clone(), move || {
                while worker.check().is_ok() {
                    thread::yield_now();
                }
            })
            .unwrap();
        jobs.cancel_all();
        while !jobs.spawner.state.lock().unwrap().workers[0]
            .0
            .is_finished()
        {
            thread::yield_now();
        }
        let next = budget();
        jobs.spawner().spawn(next.clone(), || {}).unwrap();
        assert!(!next.cancelled());
        jobs.shutdown();
    }
}
