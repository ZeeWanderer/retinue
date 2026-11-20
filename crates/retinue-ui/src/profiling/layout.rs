use rustix::time::{ClockId, clock_gettime};
use std::time::Instant;

pub const PHASES: [&str; 14] = [
    "layout_dispatch",
    "layout_worker_wall",
    "layout_pickup",
    "layout_serial_cpu",
    "layout_batch_cpu",
    "layout_wait_solving",
    "layout_wait_dispatch",
    "layout_wait_pickup",
    "layout_wait_other",
    "layout_split_cpu",
    "layout_merge_cpu",
    "layout_batch_wall",
    "layout_slot_map_cpu",
    "layout_gather_cpu",
];

const SOLVING: usize = 5;
const DISPATCH: usize = 6;
const PICKUP: usize = 7;
const OTHER: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub ns: [u64; PHASES.len()],
    pub jobs: u32,
    pub groups: u32,
    pub slots: usize,
    pub computed: usize,
}

pub(crate) fn thread_cpu_ns() -> u64 {
    let t = clock_gettime(ClockId::ThreadCPUTime);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

#[derive(Clone, Copy, Default)]
pub(crate) struct BatchCpu {
    pub scope: u64,
    pub groups: u32,
    pub work: u64,
    pub split: u64,
    pub mapping: u64,
    pub gather: u64,
    pub merge: u64,
    pub wall: u64,
    pub slots: usize,
}

pub(crate) struct Job {
    pub submitted: Instant,
    pub started: Instant,
    pub finished: Instant,
    pub serial_cpu: u64,
    pub computed: usize,
    pub batches: BatchCpu,
}

#[derive(Default)]
pub(super) struct Trace {
    sample: Sample,
    jobs: Vec<[u64; 4]>,
    waits: Vec<(u64, u64)>,
}

impl Job {
    pub(crate) fn collected(self, now: Instant) {
        super::ACTIVE.with_borrow_mut(|active| {
            let Some(active) = active else { return };
            let trace = &mut active.accounting.layout;
            trace.sample.jobs += 1;
            trace.sample.groups += self.batches.groups;
            trace.sample.slots += self.batches.slots;
            trace.sample.computed += self.computed;
            for (dst, elapsed) in trace.sample.ns.iter_mut().zip([
                self.started - self.submitted,
                self.finished - self.started,
                now - self.finished,
            ]) {
                *dst += elapsed.as_nanos() as u64;
            }
            trace.sample.ns[3] += self.serial_cpu;
            trace.sample.ns[4] += self.batches.work;
            trace.sample.ns[9] += self.batches.split;
            trace.sample.ns[10] += self.batches.merge;
            trace.sample.ns[11] += self.batches.wall;
            trace.sample.ns[12] += self.batches.mapping;
            trace.sample.ns[13] += self.batches.gather;
            trace.jobs.push(
                [self.submitted, self.started, self.finished, now]
                    .map(|t| t.saturating_duration_since(active.started).as_nanos() as u64),
            );
        });
    }
}

impl Trace {
    pub(super) fn wait(&mut self, start: u64, end: u64) {
        self.waits.push((start, end));
    }

    pub(super) fn finish(mut self) -> Sample {
        // Classify the union, not the sum: different roots can be in flight together.
        for (mut t, end) in self.waits {
            while t < end {
                let mut next = end;
                let mut phase = OTHER;
                for &[submitted, started, finished, collected] in &self.jobs {
                    let (boundary, category) = if t < submitted {
                        (submitted, OTHER)
                    } else if t < started {
                        (started, DISPATCH)
                    } else if t < finished {
                        (finished, SOLVING)
                    } else if t < collected {
                        (collected, PICKUP)
                    } else {
                        continue;
                    };
                    phase = phase.min(category);
                    next = next.min(boundary);
                }
                self.sample.ns[phase] += next - t;
                t = next;
            }
        }
        self.sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_jobs_partition_only_the_actual_owner_wait() {
        let trace = Trace {
            jobs: vec![[1, 4, 8, 14], [6, 9, 12, 15]],
            waits: vec![(0, 7), (8, 17)],
            ..Trace::default()
        };
        assert_eq!(trace.finish().ns[SOLVING..=OTHER], [6, 4, 3, 3]);
    }
}
