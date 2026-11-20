use super::{Executor, Graph, Priority};
use std::{num::NonZeroUsize, sync::Arc};

/// CPU stages invoked by a blocking I/O coordinator, never by a UI or pool task.
/// The default executes inline for standalone tools without a host executor.
#[derive(Clone, Default)]
pub struct CpuStage(Option<Arc<Executor>>);

impl CpuStage {
    pub fn new(executor: Arc<Executor>) -> Self {
        Self(Some(executor))
    }

    pub fn run<T: Send + 'static>(
        &self,
        run: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let Some(executor) = &self.0 else {
            return run();
        };
        assert!(
            !executor.is_worker(),
            "CPU tasks must express dependencies instead of waiting"
        );
        let mut graph = Graph::default();
        let mut output = graph.add_owned(&[], NonZeroUsize::MIN, move |_| Ok(run()));
        let submission = executor
            .submit(graph, Priority::Foreground, || {})
            .map_err(|error| format!("CPU stage admission: {error:?}"))?;
        submission
            .wait()
            .map_err(|_| "CPU stage stopped".to_owned())?;
        output
            .take()
            .ok_or_else(|| "CPU stage result missing".to_owned())?
            .map_err(|error| format!("CPU stage failed: {error:?}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Limits;

    #[test]
    fn io_coordinator_moves_work_to_the_host_pool_and_reports_failure() {
        let executor = Arc::new(Executor::new(Limits {
            participants: NonZeroUsize::MIN,
            tasks: 8,
        }));
        let cpu = CpuStage::new(executor.clone());
        let caller = std::thread::current().id();
        let worker = cpu.run(|| Ok(std::thread::current().id())).unwrap();
        assert_ne!(caller, worker);
        assert_eq!(
            cpu.run(|| Err::<(), _>("invalid image".into())),
            Err("invalid image".into())
        );
        assert!(
            cpu.run(|| -> Result<(), String> { panic!("decoder panic") })
                .unwrap_err()
                .contains("Panicked")
        );
        assert_eq!(cpu.run(|| Ok(42)), Ok(42));
        assert_eq!(cpu.run(|| Ok(std::cell::Cell::new(7))).unwrap().get(), 7);
        assert_eq!(executor.stats().participants, 0);
    }

    #[test]
    fn pool_task_cannot_block_waiting_for_another_cpu_stage() {
        let executor = Arc::new(Executor::new(Limits {
            participants: NonZeroUsize::MIN,
            tasks: 8,
        }));
        let nested = CpuStage::new(executor.clone());
        let outer = CpuStage::new(executor);
        assert!(
            outer
                .run(move || nested.run(|| Ok(())))
                .unwrap_err()
                .contains("Panicked")
        );
    }
}
