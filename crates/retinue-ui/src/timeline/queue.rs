use super::Deadlines;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    hash::Hash,
    time::Instant,
};

pub(crate) struct Timing<K> {
    scheduled: BTreeSet<(Instant, K)>,
    entries: HashMap<K, Option<Instant>>,
    wall: HashSet<K>,
    motion: HashSet<K>,
}

impl<K> Default for Timing<K> {
    fn default() -> Self {
        Self {
            scheduled: BTreeSet::new(),
            entries: HashMap::new(),
            wall: HashSet::new(),
            motion: HashSet::new(),
        }
    }
}

impl<K: Copy + Hash + Ord> Timing<K> {
    pub fn replace(&mut self, id: K, deadlines: &Deadlines) {
        self.remove(id);
        if let Some(next) = deadlines.next {
            self.scheduled.insert((next, id));
        }
        if deadlines.wall_clock {
            self.wall.insert(id);
        }
        if deadlines.motion {
            self.motion.insert(id);
        }
        if deadlines.next.is_some() || deadlines.wall_clock {
            self.entries.insert(id, deadlines.next);
        }
    }

    pub fn remove(&mut self, id: K) {
        if let Some(Some(next)) = self.entries.remove(&id) {
            self.scheduled.remove(&(next, id));
        }
        self.wall.remove(&id);
        self.motion.remove(&id);
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.scheduled.first().map(|(time, _)| *time)
    }

    pub fn moving(&self) -> impl Iterator<Item = K> + '_ {
        self.motion.iter().copied()
    }

    pub fn take_due(&mut self, now: Instant, clock_changed: bool, mut due: impl FnMut(K)) {
        if clock_changed {
            for &id in &self.wall {
                if self.entries[&id].is_none_or(|at| at > now) {
                    due(id);
                }
            }
        }
        while self.scheduled.first().is_some_and(|(at, _)| *at <= now) {
            let (_, id) = self.scheduled.pop_first().unwrap();
            *self.entries.get_mut(&id).unwrap() = None;
            due(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn due_and_clock_jump_coalesce_without_losing_future_wall_notifications() {
        let mut queue = Timing::default();
        let now = Instant::now();
        queue.replace(
            1_u32,
            &Deadlines {
                next: Some(now + Duration::from_secs(1)),
                wall_clock: true,
                motion: true,
                ..Default::default()
            },
        );
        let mut due = Vec::new();
        queue.take_due(now + Duration::from_secs(1), true, |id| due.push(id));
        assert_eq!(due, [1]);
        assert_eq!(queue.deadline(), None);
        due.clear();
        queue.take_due(now + Duration::from_secs(2), true, |id| due.push(id));
        assert_eq!(due, [1]);
        queue.remove(1);
        assert_eq!(queue.moving().count(), 0);
        queue.take_due(now + Duration::from_secs(3), true, |_| {
            panic!("removed owner")
        });
    }
}
