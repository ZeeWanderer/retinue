use super::ViewId;
use crate::timeline::Deadlines;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    time::Instant,
};

#[derive(Default)]
pub(super) struct Timing {
    scheduled: BTreeSet<(Instant, ViewId)>,
    entries: HashMap<ViewId, Option<Instant>>,
    wall: HashSet<ViewId>,
}

impl Timing {
    pub fn replace(&mut self, id: ViewId, deadlines: &Deadlines) {
        self.remove(id);
        if let Some(next) = deadlines.next {
            self.scheduled.insert((next, id));
        }
        if deadlines.wall_clock {
            self.wall.insert(id);
        }
        if deadlines.next.is_some() || deadlines.wall_clock {
            self.entries.insert(id, deadlines.next);
        }
    }

    pub fn remove(&mut self, id: ViewId) {
        if let Some(Some(next)) = self.entries.remove(&id) {
            self.scheduled.remove(&(next, id));
        }
        self.wall.remove(&id);
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.scheduled.first().map(|(time, _)| *time)
    }

    pub fn take_due(&mut self, now: Instant, clock_changed: bool) -> Vec<ViewId> {
        let mut due = Vec::new();
        while self.scheduled.first().is_some_and(|(at, _)| *at <= now) {
            due.push(self.scheduled.pop_first().unwrap().1);
        }
        if clock_changed {
            due.extend(self.wall.iter().copied());
        }
        due
    }
}
