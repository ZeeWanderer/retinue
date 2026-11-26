use super::{Input, Kind, State, Tree, solver::Computation};
use retinue_render::{
    RenderHost, Wake,
    work::{Executor, Failure, Graph, OwnedOutput, Priority, Submission},
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};
use taffy::Size;

const MINIMUM_NODES: usize = 32;

#[cfg(test)]
mod tests;

/// Layout uses the host's existing CPU budget and completion wake-up.
#[derive(Clone)]
pub struct LayoutExecution {
    executor: Arc<Executor>,
    participants: NonZeroUsize,
    wake: Wake,
}

impl LayoutExecution {
    pub fn new(host: &RenderHost, wake: Wake) -> Self {
        Self {
            executor: host.executor(),
            participants: NonZeroUsize::new(host.task_threads(NonZeroU32::MAX).get() as usize)
                .unwrap(),
            wake,
        }
    }
}

struct Storage {
    #[cfg(feature = "layout-profiling")]
    job: Option<crate::profiling::layout::Job>,
    inputs: Vec<Option<Input>>,
    state: State,
    #[cfg(feature = "profiling")]
    elapsed: std::time::Duration,
}

pub(in crate::scene) struct Pending {
    storage: Arc<Mutex<Option<Storage>>>,
    output: OwnedOutput<()>,
    submission: Option<Submission>,
    #[cfg(feature = "layout-profiling")]
    pub(in crate::scene) worker_window: Option<(std::time::Instant, std::time::Instant)>,
    #[cfg(feature = "profiling")]
    pub(in crate::scene) elapsed: std::time::Duration,
}

impl Pending {
    pub(in crate::scene) fn poll(&mut self, tree: &mut Tree) -> Option<Result<(), &'static str>> {
        let result = self.output.take()?;
        let storage = self.storage.lock().unwrap().take().unwrap();
        #[cfg(feature = "layout-profiling")]
        if let Some(job) = storage.job {
            self.worker_window = Some((job.started, job.finished));
            job.collected(std::time::Instant::now());
        }
        #[cfg(feature = "profiling")]
        {
            self.elapsed = storage.elapsed;
        }
        tree.inputs = storage.inputs;
        tree.state = storage.state;
        self.submission.take();
        Some(result.map_err(|error| match error {
            Failure::Panicked => "layout computation panicked",
            Failure::Cancelled => "layout computation cancelled",
            _ => "custom layout failed",
        }))
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(submission) = &self.submission {
            submission.cancel();
        }
    }
}

impl Tree {
    pub(in crate::scene) fn begin_layout(
        &mut self,
        root: usize,
        size: Size<f32>,
        execution: &LayoutExecution,
    ) -> Option<Pending> {
        let mut count = 0;
        for input in self.inputs.iter().flatten() {
            if input.kind == Kind::Owner {
                return None;
            }
            count += 1;
        }
        if count < MINIMUM_NODES {
            return None;
        }
        let storage = Arc::new(Mutex::new(Some(Storage {
            #[cfg(feature = "layout-profiling")]
            job: None,
            inputs: std::mem::take(&mut self.inputs),
            state: std::mem::take(&mut self.state),
            #[cfg(feature = "profiling")]
            elapsed: std::time::Duration::ZERO,
        })));
        let worker = storage.clone();
        let mut graph = Graph::default();
        #[cfg(feature = "layout-profiling")]
        let submitted = std::time::Instant::now();
        let output = graph.add_parallel_owned(&[], execution.participants, move |_, parallel| {
            // The slot only hands ownership over. Computation holds no storage lock.
            let mut storage = worker.lock().unwrap().take().unwrap();
            #[cfg(feature = "profiling")]
            let started = std::time::Instant::now();
            #[cfg(feature = "layout-profiling")]
            let cpu = {
                storage.state.batch_cpu = Default::default();
                crate::profiling::layout::thread_cpu_ns()
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                Computation::new(&storage.inputs, &mut storage.state, None)
                    .batched(parallel, MINIMUM_NODES)
                    .compute(root, size)
            }));
            let result = match result {
                Ok(result) => result.map_err(|error| Failure::Failed(error.into())),
                Err(_) => {
                    storage.state.invalidate_caches();
                    Err(Failure::Panicked)
                }
            };
            #[cfg(feature = "profiling")]
            {
                storage.elapsed = started.elapsed();
            }
            #[cfg(feature = "layout-profiling")]
            {
                let measured = &storage.state.batch_cpu;
                storage.job = Some(crate::profiling::layout::Job {
                    submitted,
                    started,
                    finished: std::time::Instant::now(),
                    serial_cpu: crate::profiling::layout::thread_cpu_ns() - cpu - measured.scope,
                    computed: storage.state.computed,
                    batches: *measured,
                });
            }
            *worker.lock().unwrap() = Some(storage);
            result
        });
        let wake = execution.wake.clone();
        match execution
            .executor
            .submit(graph, Priority::Foreground, move || wake())
        {
            Ok(submission) => Some(Pending {
                storage,
                output,
                submission: Some(submission),
                #[cfg(feature = "layout-profiling")]
                worker_window: None,
                #[cfg(feature = "profiling")]
                elapsed: std::time::Duration::ZERO,
            }),
            Err(_) => {
                let storage = storage.lock().unwrap().take().unwrap();
                self.inputs = storage.inputs;
                self.state = storage.state;
                None
            }
        }
    }
}
