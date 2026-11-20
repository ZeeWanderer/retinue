use crate::layout::{Proposal, alignment::AlignmentGuide};
use taffy::{LayoutInput, LayoutOutput, RunMode};

const MAX_QUERIES: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct Measurement {
    pub input: LayoutInput,
    pub output: LayoutOutput,
}

#[derive(Clone, Copy)]
pub(super) struct Alignment {
    pub guide: AlignmentGuide,
    pub proposal: Proposal,
    pub input: LayoutInput,
    pub value: f32,
}

#[derive(Default)]
pub(super) struct Dependencies {
    pub measurements: Vec<Measurement>,
    pub placement: Option<(LayoutInput, LayoutOutput)>,
    pub alignments: Vec<Alignment>,
    pub overflowed: bool,
}

impl Dependencies {
    pub fn observe(&mut self, input: LayoutInput, output: LayoutOutput) {
        match input.run_mode {
            RunMode::ComputeSize => {
                if let Some(query) = self.measurements.iter_mut().find(|q| q.input == input) {
                    query.output = output;
                } else if self.measurements.len() < MAX_QUERIES {
                    self.measurements.push(Measurement { input, output });
                } else {
                    // An unusual custom layout can issue arbitrarily many proposals.
                    // Forgetting one must never turn an unknown dependency into a hit.
                    self.overflowed = true;
                }
            }
            RunMode::PerformLayout => self.placement = Some((input, output)),
            RunMode::PerformHiddenLayout => {
                self.reset_queries();
                self.placement = None;
            }
        }
    }

    pub fn observe_alignment(&mut self, query: Alignment) {
        if let Some(old) = self.alignments.iter_mut().find(|old| {
            old.guide == query.guide && old.proposal == query.proposal && old.input == query.input
        }) {
            old.value = query.value;
        } else if self.alignments.len() < MAX_QUERIES {
            self.alignments.push(query);
        } else {
            self.overflowed = true;
        }
    }

    pub fn reset_queries(&mut self) {
        self.measurements.clear();
        self.alignments.clear();
        self.overflowed = false;
    }
}
