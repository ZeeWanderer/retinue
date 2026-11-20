use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Cancelled,
    Dependency,
    Panicked,
    Rejected,
    Failed(Arc<str>),
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId {
    pub(super) graph: u64,
    pub(super) index: usize,
}

/// An immutable result lease. `read` never blocks; dependencies make it ready.
pub struct Output<T> {
    id: TaskId,
    value: Arc<OnceLock<Result<T, Failure>>>,
}

impl<T> Clone for Output<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
        }
    }
}

impl<T> Output<T> {
    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn read(&self) -> Option<Result<&T, &Failure>> {
        self.value.get().map(Result::as_ref)
    }
    pub fn into_result(self) -> Result<Result<T, Failure>, Self> {
        match Arc::try_unwrap(self.value) {
            Ok(value) => match value.into_inner() {
                Some(result) => Ok(result),
                None => Err(Self {
                    id: self.id,
                    value: Arc::new(OnceLock::new()),
                }),
            },
            Err(value) => Err(Self { id: self.id, value }),
        }
    }
}

/// A single-consumer result. Mutable data moves between jobs; it is never shared
/// while being computed. Only publication and collection use the handoff lock.
pub struct OwnedOutput<T> {
    id: TaskId,
    value: Arc<Mutex<Option<Result<T, Failure>>>>,
}

impl<T> OwnedOutput<T> {
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Does not wait for the job. A successful take consumes its result.
    pub fn take(&mut self) -> Option<Result<T, Failure>> {
        self.value.lock().unwrap().take()
    }
}

pub(super) type Job = Box<dyn FnOnce(Option<Failure>, &Cancellation) -> Result<(), Failure> + Send>;

pub(super) struct Node {
    pub job: Option<Job>,
    pub successors: Vec<usize>,
    pub remaining: usize,
    pub blocked: bool,
    pub participants: usize,
}

/// Edges refer to preceding nodes in this graph, so cycles are unrepresentable.
pub struct Graph {
    pub(super) id: u64,
    pub(super) nodes: Vec<Node>,
    pub(super) cancellation: Cancellation,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            id: NEXT_GRAPH.fetch_add(1, Ordering::Relaxed),
            nodes: Vec::new(),
            cancellation: Cancellation::default(),
        }
    }
}

impl Graph {
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    pub fn add<T: Send + Sync + 'static>(
        &mut self,
        dependencies: &[TaskId],
        participants: std::num::NonZeroUsize,
        run: impl FnOnce(&Cancellation) -> Result<T, Failure> + Send + 'static,
    ) -> Output<T> {
        let value = Arc::new(OnceLock::new());
        let published = value.clone();
        let id = self.insert(dependencies, participants, run, move |result| {
            let _ = published.set(result);
        });
        Output { id, value }
    }

    pub fn add_owned<T: Send + 'static>(
        &mut self,
        dependencies: &[TaskId],
        participants: std::num::NonZeroUsize,
        run: impl FnOnce(&Cancellation) -> Result<T, Failure> + Send + 'static,
    ) -> OwnedOutput<T> {
        let value = Arc::new(Mutex::new(None));
        let published = value.clone();
        let id = self.insert(dependencies, participants, run, move |result| {
            *published.lock().unwrap() = Some(result);
        });
        OwnedOutput { id, value }
    }

    /// Reserves the complete width before the task can split ready work. Its
    /// non-transferable scope prevents nested batches from multiplying the budget.
    pub fn add_parallel_owned<T: Send + 'static>(
        &mut self,
        dependencies: &[TaskId],
        participants: std::num::NonZeroUsize,
        run: impl FnOnce(&Cancellation, &mut super::Parallelism) -> Result<T, Failure> + Send + 'static,
    ) -> OwnedOutput<T> {
        self.add_owned(dependencies, participants, move |cancellation| {
            run(cancellation, &mut super::Parallelism::new(participants))
        })
    }

    fn insert<T: Send + 'static>(
        &mut self,
        dependencies: &[TaskId],
        participants: std::num::NonZeroUsize,
        run: impl FnOnce(&Cancellation) -> Result<T, Failure> + Send + 'static,
        publish: impl FnOnce(Result<T, Failure>) + Send + 'static,
    ) -> TaskId {
        let id = TaskId {
            graph: self.id,
            index: self.nodes.len(),
        };
        let mut dependencies = dependencies.to_vec();
        dependencies.sort_unstable_by_key(|task| task.index);
        dependencies.dedup();
        for dependency in &dependencies {
            assert_eq!(
                dependency.graph, self.id,
                "dependency belongs to another graph"
            );
            assert!(
                dependency.index < id.index,
                "dependency must precede consumer"
            );
        }
        for dependency in &dependencies {
            self.nodes[dependency.index].successors.push(id.index);
        }
        self.nodes.push(Node {
            job: Some(Box::new(move |skip, cancellation| {
                let result = match skip {
                    Some(failure) => Err(failure),
                    None if cancellation.is_cancelled() => Err(Failure::Cancelled),
                    None => {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(cancellation)))
                            .unwrap_or(Err(Failure::Panicked))
                    }
                };
                let status = result.as_ref().map(|_| ()).map_err(Clone::clone);
                publish(result);
                status
            })),
            successors: Vec::new(),
            remaining: dependencies.len(),
            blocked: false,
            participants: participants.get(),
        });
        id
    }

    pub(super) fn reject(&mut self) {
        for node in &mut self.nodes {
            if let Some(job) = node.job.take() {
                job(Some(Failure::Rejected), &self.cancellation).ok();
            }
        }
    }
}

impl Drop for Graph {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            if let Some(job) = node.job.take() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(Some(Failure::Cancelled), &self.cancellation)
                }));
            }
        }
    }
}
