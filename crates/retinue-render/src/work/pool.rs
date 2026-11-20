use super::graph::{Cancellation, Failure, Graph, Job};
use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, Weak, mpsc},
    thread::JoinHandle,
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub participants: NonZeroUsize,
    /// Admitted nodes, including completed nodes retained by unfinished graphs.
    pub tasks: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    Foreground,
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Stopped,
    Participants,
    Unavailable(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub retained: usize,
    pub queued: usize,
    pub running: usize,
    pub participants: usize,
    pub background_participants: usize,
}

pub struct Submission {
    cancellation: Cancellation,
    completed: mpsc::Receiver<Report>,
}

impl Submission {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn try_complete(&self) -> Result<Report, mpsc::TryRecvError> {
        self.completed.try_recv()
    }

    /// For synchronous consumers outside the pool. Event loops should poll and wake instead.
    pub fn wait(&self) -> Result<Report, mpsc::RecvError> {
        self.completed.recv()
    }
}

type Complete = Box<dyn FnOnce(Report) + Send>;

struct Active {
    graph: Graph,
    priority: Priority,
    left: usize,
    report: Report,
    complete: Complete,
}

#[derive(Default)]
struct State {
    graphs: HashMap<u64, Active>,
    ready: [VecDeque<(u64, usize)>; 2],
    stats: Stats,
    paused: usize,
    stopped: bool,
}

struct Shared {
    state: Mutex<State>,
    idle: Condvar,
    pool: Mutex<Weak<rayon::ThreadPool>>,
    limits: Limits,
}

/// One bounded pool per host/process, not one pool per frame or surface.
/// Keep ownership outside its jobs; owner teardown joins the worker threads.
pub struct Executor {
    shared: Arc<Shared>,
    pool: Mutex<Option<Arc<rayon::ThreadPool>>>,
    threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

pub struct Pause {
    shared: Arc<Shared>,
    pub participants: usize,
}

impl Drop for Pause {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().paused -= 1;
        Shared::dispatch(&self.shared);
    }
}

impl Executor {
    pub fn new(limits: Limits) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                idle: Condvar::new(),
                pool: Mutex::new(Weak::new()),
                limits,
            }),
            pool: Mutex::new(None),
            threads: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start(&self) -> Result<(), SubmitError> {
        let mut target = self.pool.lock().unwrap();
        if target.is_some() {
            return Ok(());
        }
        let worker_handles = self.threads.clone();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.shared.limits.participants.get())
            .spawn_handler(move |worker| {
                let handle = std::thread::Builder::new()
                    .name(format!("retinue-cpu-{}", worker.index()))
                    .spawn(move || worker.run())?;
                worker_handles.lock().unwrap().push(handle);
                Ok(())
            })
            .build();
        let pool = match pool {
            Ok(pool) => Arc::new(pool),
            Err(error) => {
                for thread in self.threads.lock().unwrap().drain(..) {
                    let _ = thread.join();
                }
                return Err(SubmitError::Unavailable(error.to_string()));
            }
        };
        *self.shared.pool.lock().unwrap() = Arc::downgrade(&pool);
        *target = Some(pool);
        Ok(())
    }

    pub fn stats(&self) -> Stats {
        self.shared.state.lock().unwrap().stats
    }

    pub(super) fn is_worker(&self) -> bool {
        self.pool
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|pool| pool.current_thread_index().is_some())
    }

    /// Pause new background work without waiting for an already executing job.
    pub fn pause_background(&self) -> Pause {
        let mut state = self.shared.state.lock().unwrap();
        state.paused += 1;
        Pause {
            shared: self.shared.clone(),
            participants: state.stats.background_participants,
        }
    }

    pub fn submit(
        &self,
        mut graph: Graph,
        priority: Priority,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<Submission, SubmitError> {
        if !graph.nodes.is_empty()
            && let Err(error) = self.start()
        {
            graph.reject();
            return Err(error);
        }
        let (send, completed) = mpsc::channel();
        let submission = Submission {
            cancellation: graph.cancellation(),
            completed,
        };
        let complete: Complete = Box::new(move |report| {
            let _ = send.send(report);
            wake();
        });
        let mut state = self.shared.state.lock().unwrap();
        let capacity = self.shared.limits.participants.get();
        let max_width = match priority {
            Priority::Foreground => capacity,
            Priority::Background => capacity.saturating_sub(1),
        };
        let error = if state.stopped {
            Some(SubmitError::Stopped)
        } else if graph.nodes.iter().any(|node| node.participants > max_width) {
            Some(SubmitError::Participants)
        } else if graph.nodes.len()
            > self
                .shared
                .limits
                .tasks
                .saturating_sub(state.stats.retained)
        {
            Some(SubmitError::Full)
        } else {
            None
        };
        if let Some(error) = error {
            drop(state);
            graph.reject();
            return Err(error);
        }
        if graph.nodes.is_empty() {
            drop(state);
            complete(Report::default());
            return Ok(submission);
        }
        for (index, node) in graph.nodes.iter().enumerate() {
            if node.remaining == 0 {
                state.ready[priority as usize].push_back((graph.id, index));
            }
        }
        let left = graph.nodes.len();
        state.stats.retained += left;
        state.stats.queued += left;
        state.graphs.insert(
            graph.id,
            Active {
                graph,
                priority,
                left,
                report: Report::default(),
                complete,
            },
        );
        drop(state);
        Shared::dispatch(&self.shared);
        Ok(submission)
    }
}

struct Ready {
    graph: u64,
    index: usize,
    participants: usize,
    priority: Priority,
    cancellation: Cancellation,
    skip: Option<Failure>,
    job: Job,
}

impl Shared {
    fn dispatch(shared: &Arc<Self>) {
        let Some(pool) = shared.pool.lock().unwrap().upgrade() else {
            return;
        };
        let mut state = shared.state.lock().unwrap();
        let mut ready = Vec::new();
        'priority: for queue in 0..2 {
            if queue == 1 && state.paused != 0 && !state.stopped {
                break;
            }
            let capacity = shared
                .limits
                .participants
                .get()
                .saturating_sub(usize::from(queue == 1));
            loop {
                let available = capacity.saturating_sub(state.stats.participants);
                let fits = |&(id, index): &(u64, usize)| {
                    let active = &state.graphs[&id];
                    active.graph.cancellation.is_cancelled()
                        || active.graph.nodes[index].participants <= available
                };
                let candidate = if queue == 0 {
                    state.ready[queue]
                        .front()
                        .filter(|entry| fits(entry))
                        .map(|_| 0)
                } else {
                    state.ready[queue].iter().position(fits)
                };
                let Some(candidate) = candidate else {
                    if queue == 0 && !state.ready[queue].is_empty() {
                        break 'priority;
                    }
                    break;
                };
                let (id, index) = state.ready[queue].remove(candidate).unwrap();
                let active = state.graphs.get_mut(&id).unwrap();
                let cancellation = active.graph.cancellation();
                let node = &mut active.graph.nodes[index];
                let skip = if cancellation.is_cancelled() {
                    Some(Failure::Cancelled)
                } else if node.blocked {
                    Some(Failure::Dependency)
                } else {
                    None
                };
                let participants = if skip.is_some() { 0 } else { node.participants };
                ready.push(Ready {
                    graph: id,
                    index,
                    participants,
                    priority: active.priority,
                    cancellation,
                    skip,
                    job: node.job.take().unwrap(),
                });
                state.stats.queued -= 1;
                state.stats.running += 1;
                state.stats.participants += participants;
                if queue == 1 {
                    state.stats.background_participants += participants;
                }
            }
        }
        drop(state);
        for ready in ready {
            let shared = shared.clone();
            pool.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (ready.job)(ready.skip, &ready.cancellation)
                }))
                .unwrap_or(Err(Failure::Panicked));
                let mut state = shared.state.lock().unwrap();
                state.stats.running -= 1;
                state.stats.participants -= ready.participants;
                if ready.priority == Priority::Background {
                    state.stats.background_participants -= ready.participants;
                }
                let active = state.graphs.get_mut(&ready.graph).unwrap();
                match result {
                    Ok(()) => active.report.completed += 1,
                    Err(Failure::Cancelled) => active.report.cancelled += 1,
                    Err(_) => active.report.failed += 1,
                }
                active.left -= 1;
                let successors = std::mem::take(&mut active.graph.nodes[ready.index].successors);
                let mut new_ready = Vec::new();
                for successor in successors {
                    let node = &mut active.graph.nodes[successor];
                    node.remaining -= 1;
                    node.blocked |= result.is_err();
                    if node.remaining == 0 {
                        new_ready.push((ready.graph, successor));
                    }
                }
                let finished = active.left == 0;
                state.ready[ready.priority as usize].extend(new_ready);
                let completed = finished.then(|| {
                    let active = state.graphs.remove(&ready.graph).unwrap();
                    state.stats.retained -= active.graph.nodes.len();
                    active
                });
                drop(state);
                if let Some(active) = completed {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (active.complete)(active.report)
                    }));
                }
                Shared::dispatch(&shared);
                shared.idle.notify_all();
            });
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.stopped = true;
        for active in state.graphs.values() {
            active.graph.cancellation.cancel();
        }
        drop(state);
        Shared::dispatch(&self.shared);
        let mut state = self.shared.state.lock().unwrap();
        while !state.graphs.is_empty() {
            state = self.shared.idle.wait(state).unwrap();
        }
        drop(state);
        self.pool.get_mut().unwrap().take();
        for thread in self.threads.lock().unwrap().drain(..) {
            let _ = thread.join();
        }
    }
}
