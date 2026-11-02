use super::{
    PaintProgram, ProgramId, paint::Pixels, program::AnalysisTarget, replay_index::ReplayIndex,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Copy, Debug)]
pub struct AnalysisPolicy {
    pub jobs: usize,
    pub bytes: usize,
    pub min_commands: usize,
    pub max_commands: usize,
}

impl AnalysisPolicy {
    pub const fn desktop() -> Self {
        Self {
            jobs: 8,
            bytes: 2 * 1024 * 1024,
            min_commands: 128,
            max_commands: 4096,
        }
    }

    pub const fn overlay() -> Self {
        Self {
            jobs: 2,
            bytes: 256 * 1024,
            min_commands: 128,
            max_commands: 1024,
        }
    }

    pub const fn disabled() -> Self {
        Self {
            jobs: 0,
            bytes: 0,
            min_commands: 0,
            max_commands: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AnalysisStats {
    pub jobs: usize,
    pub reserved_bytes: usize,
    pub running: bool,
    pub submitted: u64,
    pub published: u64,
    pub discarded: u64,
    pub failures: u64,
}

struct Job {
    id: ProgramId,
    bounds: Vec<Option<Pixels>>,
    alive: Weak<()>,
    bytes: usize,
}

struct Completed {
    id: ProgramId,
    index: Option<ReplayIndex>,
    bytes: usize,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    completed: VecDeque<Completed>,
    paused: usize,
    running: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    stopping: AtomicBool,
}

pub(super) struct Pause {
    shared: Arc<Shared>,
    pub running: bool,
}

impl Drop for Pause {
    fn drop(&mut self) {
        let mut queue = self.shared.queue.lock().unwrap();
        queue.paused -= 1;
        if queue.paused == 0 {
            self.shared.wake.notify_one();
        }
    }
}

pub(super) struct Background {
    policy: AnalysisPolicy,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    targets: HashMap<ProgramId, AnalysisTarget>,
    stats: AnalysisStats,
}

impl Background {
    pub fn new(policy: AnalysisPolicy) -> Self {
        Self {
            policy,
            shared: Arc::default(),
            worker: None,
            targets: HashMap::new(),
            stats: AnalysisStats::default(),
        }
    }

    pub fn pause(&self) -> Pause {
        let mut queue = self.shared.queue.lock().unwrap();
        queue.paused += 1;
        Pause {
            shared: self.shared.clone(),
            running: queue.running,
        }
    }

    pub fn running(&self) -> bool {
        self.shared.queue.lock().unwrap().running
    }

    pub fn stats(&self) -> AnalysisStats {
        AnalysisStats {
            jobs: self.targets.len(),
            running: self.running(),
            ..self.stats
        }
    }

    pub fn submit(&mut self, program: &PaintProgram) -> bool {
        let count = program.optimized().len();
        if self.stats.failures != 0
            || program.has_replay_index()
            || count < self.policy.min_commands
            || count > self.policy.max_commands
            || self.policy.jobs == 0
            || self.targets.contains_key(&program.id())
        {
            return true;
        }
        let Some(bytes) = ReplayIndex::reservation(count).and_then(|output| {
            count
                .checked_mul(size_of::<Option<Pixels>>())?
                .checked_add(output)
        }) else {
            return true;
        };
        if bytes > self.policy.bytes {
            return true;
        }
        if self.targets.len() >= self.policy.jobs
            || bytes > self.policy.bytes.saturating_sub(self.stats.reserved_bytes)
        {
            return false;
        }
        let bounds: Vec<_> = program
            .optimized()
            .iter()
            .map(|command| command.cull_bounds(program.scale()))
            .collect();
        if bounds.iter().all(Option::is_none) {
            return true;
        }
        if self.worker.is_none() {
            let shared = self.shared.clone();
            match thread::Builder::new()
                .name("render-analysis".into())
                .spawn(move || run(shared))
            {
                Ok(worker) => self.worker = Some(worker),
                Err(_) => {
                    self.stats.failures += 1;
                    return true;
                }
            }
        }
        let (target, alive) = program.analysis_target();
        self.targets.insert(program.id(), target);
        self.stats.reserved_bytes += bytes;
        self.stats.submitted += 1;
        self.shared.queue.lock().unwrap().jobs.push_back(Job {
            id: program.id(),
            bounds,
            alive,
            bytes,
        });
        self.shared.wake.notify_one();
        true
    }

    pub fn collect(&mut self) {
        let completed = std::mem::take(&mut self.shared.queue.lock().unwrap().completed);
        for result in completed {
            self.stats.reserved_bytes -= result.bytes;
            let target = self.targets.remove(&result.id).unwrap();
            if result
                .index
                .is_some_and(|index| target.publish(result.id, index))
            {
                self.stats.published += 1;
            } else {
                self.stats.discarded += 1;
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let failed = self.worker.take().unwrap().join().is_err();
            self.stats.failures += u64::from(failed);
            self.stats.discarded += self.targets.len() as u64;
            self.targets.clear();
            self.stats.reserved_bytes = 0;
            let mut queue = self.shared.queue.lock().unwrap();
            queue.running = false;
            queue.jobs.clear();
            queue.completed.clear();
        }
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        {
            let mut queue = self.shared.queue.lock().unwrap();
            self.shared.stopping.store(true, Ordering::Relaxed);
            queue.jobs.clear();
            self.shared.wake.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().unwrap();
            while !shared.stopping.load(Ordering::Relaxed)
                && (queue.paused != 0 || queue.jobs.is_empty())
            {
                queue = shared.wake.wait(queue).unwrap();
            }
            if shared.stopping.load(Ordering::Relaxed) {
                return;
            }
            queue.running = true;
            queue.jobs.pop_front().unwrap()
        };
        let index = ReplayIndex::build(&job.bounds, || {
            job.alive.strong_count() == 0 || shared.stopping.load(Ordering::Relaxed)
        });
        let mut queue = shared.queue.lock().unwrap();
        queue.running = false;
        if shared.stopping.load(Ordering::Relaxed) {
            return;
        }
        queue.completed.push_back(Completed {
            id: job.id,
            index,
            bytes: job.bytes,
        });
    }
}

#[cfg(test)]
mod tests;
