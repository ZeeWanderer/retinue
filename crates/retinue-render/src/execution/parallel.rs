use super::*;
use crate::work::{Graph, Output, Priority, Submission};
use std::{
    num::NonZeroUsize,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

enum Dependency {
    Ready(Option<Arc<RasterImage>>),
    Pending(Output<Outcome>),
}

struct Dependencies(HashMap<TaskId, Dependency>);

impl Inputs for Dependencies {
    fn image(&self, task: TaskId) -> Option<&Arc<RasterImage>> {
        match &self.0[&task] {
            Dependency::Ready(image) => image.as_ref(),
            Dependency::Pending(output) => output
                .read()
                .expect("declared raster dependency is ready")
                .ok()?
                .result()
                .map(|result| &result.image),
        }
    }
}

enum Slot {
    Ready(Outcome),
    Pending(Output<Outcome>),
}

impl Slot {
    fn dependency(&self) -> Dependency {
        match self {
            Self::Ready(outcome) => {
                Dependency::Ready(outcome.result().map(|result| result.image.clone()))
            }
            Self::Pending(output) => Dependency::Pending(output.clone()),
        }
    }
}

pub(crate) struct Running {
    outputs: Vec<Slot>,
    submission: Option<Submission>,
    edges: usize,
    budget: Arc<Budget>,
    ready: bool,
    _executor: Arc<crate::work::Executor>,
}

impl Running {
    pub(crate) fn cancel_and_poll(&mut self) -> bool {
        if !self.ready
            && let Some(submission) = &self.submission
        {
            submission.cancel();
        }
        self.ready()
    }

    pub fn ready(&mut self) -> bool {
        if !self.ready {
            self.ready = self.submission.as_ref().is_none_or(|submission| {
                !matches!(
                    submission.try_complete(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                )
            });
        }
        self.ready
    }

    pub fn wait(mut self, host: &RenderHost) -> Completion {
        if !self.ready
            && let Some(submission) = &self.submission
        {
            let _ = submission.wait();
        }
        self.ready = true;
        if let Some(started) = self.budget.started.get() {
            host.charge_interval(
                *started,
                Duration::from_nanos(self.budget.elapsed.load(Ordering::Acquire)),
            );
        }
        Completion {
            outputs: std::mem::take(&mut self.outputs)
                .into_iter()
                .map(|output| match output {
                    Slot::Ready(outcome) => outcome,
                    Slot::Pending(output) => match output.into_result() {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(
                            crate::work::Failure::Cancelled | crate::work::Failure::Rejected,
                        )) => Outcome::Deferred,
                        _ => Outcome::Failed,
                    },
                })
                .collect(),
            edges: self.edges,
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.ready
            && let Some(submission) = &self.submission
        {
            submission.cancel();
            let _ = submission.wait();
        }
    }
}

struct Budget {
    started: OnceLock<Instant>,
    duration: Duration,
    elapsed: AtomicU64,
}

impl Budget {
    fn admit(&self) -> bool {
        let mut first = false;
        let started = self.started.get_or_init(|| {
            first = true;
            Instant::now()
        });
        first || started.elapsed() < self.duration
    }

    fn completed(&self) {
        self.elapsed.fetch_max(
            self.started
                .get()
                .unwrap()
                .elapsed()
                .as_nanos()
                .min(u64::MAX as u128) as u64,
            Ordering::Release,
        );
    }
}

impl Preparation {
    pub(crate) fn dispatch(
        &self,
        host: &RenderHost,
        fonts: &Arc<crate::text::preparation::PreparedFonts>,
        wake: impl FnOnce() + Send + 'static,
    ) -> Option<Running> {
        let (executor, duration) = host.preparation_executor()?;
        let budget = Arc::new(Budget {
            started: OnceLock::new(),
            duration,
            elapsed: AtomicU64::new(0),
        });
        let mut graph = Graph::default();
        let mut outputs: Vec<Slot> = Vec::with_capacity(self.tasks.len());
        for task in &self.tasks {
            let pixels = task.pixels();
            if pixels != 0 && host.admit(pixels, 1).is_none() {
                outputs.push(Slot::Ready(Outcome::Deferred));
                continue;
            }
            let inputs = task
                .fragments
                .values()
                .chain(
                    task.substitutes
                        .iter()
                        .flat_map(|substitute| &substitute.regions)
                        .map(|source| &source.source),
                )
                .filter_map(|input| match input {
                    Input::Task(id) => Some((*id, outputs[id.0].dependency())),
                    Input::Ready(_) => None,
                })
                .collect::<HashMap<_, _>>();
            let edges = inputs
                .values()
                .filter_map(|input| match input {
                    Dependency::Pending(output) => Some(output.id()),
                    Dependency::Ready(_) => None,
                })
                .collect::<Vec<_>>();
            let inputs = Dependencies(inputs);
            if pixels == 0 {
                outputs.push(Slot::Ready(task.run_with(&inputs, 1, &mut |task, _, _| {
                    Some(Rendered {
                        image: task.previous.as_ref().unwrap().image.clone(),
                        reads: HashSet::new(),
                        blits: 0,
                        substitutions: 0,
                    })
                })));
                continue;
            }
            let task = task.clone();
            let fonts = fonts.clone();
            let budget = budget.clone();
            outputs.push(Slot::Pending(graph.add(
                &edges,
                NonZeroUsize::MIN,
                move |_| {
                    if !budget.admit() {
                        return Ok(Outcome::Deferred);
                    }
                    let outcome = task.run_with(&inputs, 1, &mut |task, inputs, _| {
                        task.render_with_threads(inputs, NonZeroU32::MIN, &fonts)
                            .ok()
                    });
                    budget.completed();
                    Ok(outcome)
                },
            )));
        }
        let submission = executor.submit(graph, Priority::Foreground, wake).ok();
        Some(Running {
            outputs,
            submission,
            edges: self.edges,
            budget,
            ready: false,
            _executor: executor,
        })
    }
}
