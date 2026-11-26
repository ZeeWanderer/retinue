//! Isolated benchmark census. Join all participating layout jobs before ending a capture.
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    thread::{self, ThreadId},
    time::Instant,
};
use taffy::{AvailableSpace, LayoutInput, RunMode};

type Group = (&'static str, &'static str, u8, bool, &'static str);
pub type Rows = BTreeMap<Group, [u64; 12]>;
pub const COLUMNS: [&str; 12] = [
    "queries",
    "misses",
    "measure_callbacks",
    "worker_queries",
    "hit_self_ns",
    "miss_self_ns",
    "leaf_adapter_ns",
    "measure_self_ns",
    "place_measure_queries",
    "same_measure_queries",
    "same_measure_misses",
    "same_measure_inclusive_ns",
];
static MODE: AtomicU8 = AtomicU8::new(0);
static RESULT: Mutex<(Option<ThreadId>, Rows)> = Mutex::new((None, BTreeMap::new()));

#[derive(Default)]
struct Local {
    depth: usize,
    children_ns: u64,
    rows: Rows,
}
thread_local! { static LOCAL: RefCell<Local> = RefCell::default(); }

pub fn capture<T>(timed: bool, f: impl FnOnce() -> T) -> (T, Rows) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let mut result = RESULT.lock().unwrap();
            MODE.store(0, Ordering::Release);
            result.0 = None;
            result.1.clear();
        }
    }
    {
        let mut result = RESULT.lock().unwrap();
        if result.0.is_some() {
            drop(result);
            panic!("overlapping solver census");
        }
        result.0 = Some(thread::current().id());
        MODE.store(if timed { 2 } else { 1 }, Ordering::Release);
    }
    let _reset = Reset;
    let value = f();
    let rows = std::mem::take(&mut RESULT.lock().unwrap().1);
    (value, rows)
}

pub(crate) struct Visit {
    group: Group,
    start: Option<Instant>,
    parent_children: u64,
    pub miss: bool,
    measured: bool,
    pub adapter_ns: u64,
    measure_ns: u64,
    pub placement: [u64; 4],
}

/// Matches only within one custom layout invocation, never across frames.
#[derive(Default)]
pub(crate) struct MeasurePairs {
    measured: Vec<(usize, crate::layout::Proposal)>,
    pub totals: [u64; 4],
    pub timed: bool,
}

impl MeasurePairs {
    pub fn new(timed: bool) -> Self {
        Self {
            timed,
            ..Self::default()
        }
    }
    pub fn measured(&mut self, child: usize, proposal: crate::layout::Proposal) {
        self.measured.push((child, proposal));
    }
    pub fn matches(&mut self, child: usize, proposal: crate::layout::Proposal) -> bool {
        self.totals[0] += 1;
        let matched = self.measured.contains(&(child, proposal));
        self.totals[1] += u64::from(matched);
        matched
    }
}

impl Visit {
    pub fn begin(input: LayoutInput, kind: impl FnOnce() -> &'static str) -> Option<Self> {
        let mode = MODE.load(Ordering::Acquire);
        if mode == 0 {
            return None;
        }
        let known = input.known_dimensions;
        let group = (
            kind(),
            match input.run_mode {
                RunMode::ComputeSize => "measure",
                RunMode::PerformLayout => "place",
                RunMode::PerformHiddenLayout => "hidden",
            },
            u8::from(known.width.is_some()) + u8::from(known.height.is_some()),
            matches!(input.available_space.width, AvailableSpace::Definite(_))
                && matches!(input.available_space.height, AvailableSpace::Definite(_)),
            "",
        );
        let parent_children = LOCAL.with_borrow_mut(|l| {
            l.depth += 1;
            std::mem::take(&mut l.children_ns)
        });
        Some(Self {
            group,
            start: (mode == 2).then(Instant::now),
            parent_children,
            miss: false,
            measured: false,
            adapter_ns: 0,
            measure_ns: 0,
            placement: [0; 4],
        })
    }
    pub fn clock(&self) -> Option<Instant> {
        self.start.map(|_| Instant::now())
    }
    pub fn algorithm(&mut self, name: &'static str) {
        self.group.4 = name;
    }
    pub fn child_ns() -> u64 {
        LOCAL.with_borrow(|l| l.children_ns)
    }
    pub fn measured(&mut self, start: Option<(Instant, u64)>) -> u64 {
        self.measured = true;
        start.map_or(0, |(t, before)| {
            let ns = t.elapsed().as_nanos() as u64;
            self.measure_ns = ns.saturating_sub(Self::child_ns() - before);
            ns
        })
    }
}

impl Drop for Visit {
    fn drop(&mut self) {
        let inclusive = self.start.map_or(0, |s| s.elapsed().as_nanos() as u64);
        LOCAL.with_borrow_mut(|l| {
            let own = inclusive.saturating_sub(l.children_ns);
            let row = l.rows.entry(self.group).or_default();
            row[0] += 1;
            row[1] += u64::from(self.miss);
            row[2] += u64::from(self.measured);
            row[4 + usize::from(self.miss)] += own;
            row[6] += self.adapter_ns;
            row[7] += self.measure_ns;
            for (dest, count) in row[8..].iter_mut().zip(self.placement) {
                *dest += count;
            }
            l.depth -= 1;
            if l.depth == 0 {
                l.children_ns = 0;
                let mut result = RESULT.lock().unwrap();
                let worker = result.0 != Some(thread::current().id());
                for (group, mut row) in std::mem::take(&mut l.rows) {
                    if worker {
                        row[3] = row[0];
                    }
                    let dest = result.1.entry(group).or_default();
                    for (a, b) in dest.iter_mut().zip(row) {
                        *a += b;
                    }
                }
            } else {
                // Charge a parent only for its own work, not nested query reporting.
                l.children_ns =
                    self.parent_children + self.start.map_or(0, |s| s.elapsed().as_nanos() as u64);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::Size;

    #[test]
    fn placement_pairs_require_the_same_child_and_proposal_in_this_invocation() {
        use crate::layout::Proposal;
        let mut visit = MeasurePairs::default();
        let p = Proposal::bounded(40., 20.);
        visit.measured(1, p);
        assert!(!visit.matches(0, p));
        assert!(!visit.matches(1, Proposal::IDEAL));
        assert!(visit.matches(1, p));
        assert_eq!(visit.totals, [3, 1, 0, 0]);
        assert!(!MeasurePairs::default().matches(1, p));
    }

    #[test]
    fn bounded_is_not_fixed_and_nested_and_worker_queries_count_once() {
        let bounded = LayoutInput {
            run_mode: RunMode::ComputeSize,
            available_space: Size {
                width: AvailableSpace::Definite(40.),
                height: AvailableSpace::Definite(40.),
            },
            ..LayoutInput::HIDDEN
        };
        let fixed = LayoutInput {
            known_dimensions: Size {
                width: Some(40.),
                height: Some(40.),
            },
            run_mode: RunMode::PerformLayout,
            ..bounded
        };
        let (_, rows) = capture(false, || {
            let mut parent = Visit::begin(fixed, || "test_parent").unwrap();
            parent.miss = true;
            drop(Visit::begin(bounded, || "test_leaf"));
            thread::spawn(move || {
                let mut child = Visit::begin(fixed, || "test_leaf").unwrap();
                child.miss = true;
                child.measured(None);
                child.algorithm("example");
                child.placement = [3, 1, 0, 0];
            })
            .join()
            .unwrap();
        });
        assert_eq!(
            rows[&("test_parent", "place", 2, true, "")],
            [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            rows[&("test_leaf", "measure", 0, true, "")],
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            rows[&("test_leaf", "place", 2, true, "example")],
            [1, 1, 1, 1, 0, 0, 0, 0, 3, 1, 0, 0]
        );
    }
}
