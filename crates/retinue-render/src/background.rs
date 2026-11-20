use super::{PaintProgram, ProgramId, program::AnalysisTarget, replay_index::ReplayIndex};
use crate::work::{Executor, Graph, Output, Priority, Submission};
use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

pub(super) use crate::work::Pause;

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
    pub participants: usize,
    pub submitted: u64,
    pub published: u64,
    pub discarded: u64,
    pub failures: u64,
}

struct Pending {
    target: AnalysisTarget,
    output: Output<Option<ReplayIndex>>,
    submission: Submission,
    bytes: usize,
}

pub(super) struct Background {
    policy: AnalysisPolicy,
    executor: Arc<Executor>,
    targets: HashMap<ProgramId, Pending>,
    stats: AnalysisStats,
}

impl Background {
    pub fn new(policy: AnalysisPolicy, executor: Arc<Executor>) -> Self {
        Self {
            policy,
            executor,
            targets: HashMap::new(),
            stats: AnalysisStats::default(),
        }
    }

    pub fn pause(&self) -> Pause {
        self.executor.pause_background()
    }

    pub fn running(&self) -> usize {
        self.executor.stats().background_participants
    }

    pub fn stats(&self) -> AnalysisStats {
        let participants = self.running();
        AnalysisStats {
            jobs: self.targets.len(),
            running: participants != 0,
            participants,
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
        let Some(bytes) = ReplayIndex::reservation(count) else {
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
        let target = program.analysis_target();
        let input = target.clone();
        let mut graph = Graph::default();
        let output = graph.add(&[], NonZeroUsize::MIN, move |cancel| {
            Ok(input.build(|| cancel.is_cancelled()))
        });
        let submission = match self.executor.submit(graph, Priority::Background, || {}) {
            Ok(submission) => submission,
            Err(crate::work::SubmitError::Full) => return false,
            Err(_) => {
                self.stats.failures += 1;
                return true;
            }
        };
        self.targets.insert(
            program.id(),
            Pending {
                target,
                output,
                submission,
                bytes,
            },
        );
        self.stats.reserved_bytes += bytes;
        self.stats.submitted += 1;
        true
    }

    pub fn collect(&mut self) {
        let completed: Vec<_> = self
            .targets
            .iter()
            .filter_map(|(&id, pending)| {
                pending
                    .submission
                    .try_complete()
                    .ok()
                    .map(|report| (id, report))
            })
            .collect();
        for (id, report) in completed {
            let pending = self.targets.remove(&id).unwrap();
            self.stats.reserved_bytes -= pending.bytes;
            self.stats.failures += report.failed as u64;
            let index = pending
                .output
                .into_result()
                .ok()
                .and_then(Result::ok)
                .flatten();
            if index.is_some_and(|index| pending.target.publish(id, index)) {
                self.stats.published += 1;
            } else {
                self.stats.discarded += 1;
            }
        }
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        for pending in self.targets.values() {
            pending.submission.cancel();
        }
    }
}

#[cfg(test)]
mod tests;
