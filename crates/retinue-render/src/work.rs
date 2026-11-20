//! Bounded CPU work with explicit dependencies and completion-only publication.
//!
//! Jobs own their inputs and outputs. They never wait for another queued job;
//! only ready nodes enter the pool. Native work reserves all its participants,
//! including the submitting worker, before any commands can trigger a flush.

mod cpu_stage;
mod graph;
mod parallel;
mod pool;

pub use cpu_stage::CpuStage;
pub use graph::{Cancellation, Failure, Graph, Output, OwnedOutput, TaskId};
pub use parallel::Parallelism;
pub use pool::{Executor, Limits, Pause, Priority, Report, Stats, Submission, SubmitError};

#[cfg(test)]
mod tests;
