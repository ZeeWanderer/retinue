use super::{Materialization, ProgramId, TaskId};
use crate::PaintStats;
use std::collections::HashSet;

#[derive(Default)]
pub(crate) enum Outcome {
    #[default]
    Pending,
    Deferred,
    Failed,
    Finished {
        result: Option<Materialization>,
        elapsed: u64,
        reads: HashSet<ProgramId>,
        blits: usize,
        substitutions: usize,
        repaired_pixels: usize,
        copied_pixels: usize,
    },
}

impl Outcome {
    pub fn result(&self) -> Option<&Materialization> {
        match self {
            Self::Pending => panic!("materialization dependency has not completed"),
            Self::Deferred | Self::Failed => None,
            Self::Finished { result, .. } => result.as_ref(),
        }
    }

    pub fn elapsed(&self) -> u64 {
        match self {
            Self::Pending => panic!("materialization task has not completed"),
            Self::Deferred | Self::Failed => 0,
            Self::Finished { elapsed, .. } => *elapsed,
        }
    }
}

#[derive(Default)]
pub(crate) struct Completion {
    pub(super) outputs: Vec<Outcome>,
    pub(super) edges: usize,
}

impl Completion {
    pub fn new(tasks: usize) -> Self {
        Self {
            outputs: (0..tasks).map(|_| Outcome::Pending).collect(),
            edges: 0,
        }
    }

    pub fn result(&self, task: TaskId) -> Option<&Materialization> {
        self.outputs[task.0].result()
    }

    pub fn elapsed(&self, task: TaskId) -> u64 {
        self.outputs[task.0].elapsed()
    }

    pub fn deferred(&self, task: TaskId) -> bool {
        matches!(self.outputs[task.0], Outcome::Deferred)
    }

    pub fn has_deferred(&self) -> bool {
        self.outputs
            .iter()
            .any(|outcome| matches!(outcome, Outcome::Deferred))
    }

    pub fn used_fragments(&self) -> impl Iterator<Item = &ProgramId> {
        self.outputs
            .iter()
            .flat_map(|outcome| match outcome {
                Outcome::Finished { reads, .. } => Some(reads.iter()),
                _ => None,
            })
            .flatten()
    }

    pub fn accumulate(&self, stats: &mut PaintStats) {
        stats.preparation_tasks += self.outputs.len();
        stats.preparation_edges += self.edges;
        for outcome in &self.outputs {
            match outcome {
                Outcome::Pending => panic!("materialization task has not completed"),
                Outcome::Deferred => stats.deferred_preparations += 1,
                Outcome::Failed => stats.failed_preparations += 1,
                Outcome::Finished {
                    result,
                    blits,
                    substitutions,
                    repaired_pixels,
                    copied_pixels,
                    ..
                } => {
                    stats.failed_preparations += usize::from(result.is_none());
                    stats.fragment_blits += blits;
                    stats.preparation_blits += substitutions;
                    stats.repaired_pixels += repaired_pixels;
                    stats.copied_pixels += copied_pixels;
                    #[cfg(any(test, feature = "benchmark"))]
                    {
                        stats.materialization_ns += outcome.elapsed();
                    }
                }
            }
        }
    }
}
