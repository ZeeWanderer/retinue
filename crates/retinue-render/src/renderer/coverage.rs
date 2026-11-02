use crate::{Rect, coverage};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

#[derive(Default)]
pub(super) struct SpatialIndex {
    spans: BTreeMap<usize, (usize, Vec<Rect>)>,
}

impl SpatialIndex {
    pub fn clear(&mut self) {
        self.spans.clear();
    }

    pub fn update(&mut self, regions: impl IntoIterator<Item = (Range<usize>, Vec<Rect>)>) {
        self.spans.clear();
        let regions: Vec<_> = regions.into_iter().collect();
        let mut events = BTreeMap::<usize, Vec<(bool, usize)>>::new();
        for (id, (range, _)) in regions.iter().enumerate() {
            if range.is_empty() {
                continue;
            }
            events.entry(range.start).or_default().push((true, id));
            events.entry(range.end).or_default().push((false, id));
        }
        let mut active = BTreeSet::new();
        let mut start = 0;
        for (end, changes) in events {
            if start < end && !active.is_empty() {
                let clips = active
                    .iter()
                    .flat_map(|&id: &usize| regions[id].1.iter().copied())
                    .collect();
                self.spans.insert(start, (end, clips));
            }
            for (added, id) in changes {
                if added {
                    active.insert(id);
                } else {
                    active.remove(&id);
                }
            }
            start = end;
        }
    }

    pub fn remaining(&self, index: usize, bounds: Rect) -> Vec<Rect> {
        let covered = self
            .spans
            .range(..=index)
            .next_back()
            .filter(|(_, (end, _))| index < *end)
            .into_iter()
            .flat_map(|(_, (_, clips))| clips.iter().copied());
        coverage::remaining(bounds, covered)
    }

    pub fn covers(&self, index: usize, bounds: Rect) -> bool {
        self.remaining(index, bounds).is_empty()
    }
}
