use super::Key;
use crate::transition::{Easing, ScalarTransition};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub struct SelectionMotion {
    selected: Key,
    weights: BTreeMap<Key, f32>,
    transition: ScalarTransition,
}

impl SelectionMotion {
    pub fn new(selected: impl Into<Key>) -> Self {
        let selected = selected.into();
        let mut transition = ScalarTransition::default();
        transition.set(1.0, Instant::now(), Duration::ZERO);
        Self {
            selected,
            weights: BTreeMap::from([(selected, 1.0)]),
            transition,
        }
    }

    pub fn select(&mut self, selected: impl Into<Key>, now: Instant, reduced_motion: bool) {
        let selected = selected.into();
        if self.selected == selected {
            return;
        }
        let keys: Vec<_> = self.weights.keys().copied().collect();
        for key in keys {
            self.weights.insert(key, self.weight(key, now));
        }
        self.weights.retain(|_, weight| *weight > 0.0);
        self.weights.entry(selected).or_default();
        self.selected = selected;
        self.transition.set(0.0, now, Duration::ZERO);
        self.transition.set_eased(
            1.0,
            now,
            if reduced_motion {
                Duration::ZERO
            } else {
                Duration::from_millis(170)
            },
            Easing::InOutCubic,
        );
    }

    pub fn selected(&self) -> Key {
        self.selected
    }

    pub fn weight(&self, key: impl Into<Key>, now: Instant) -> f32 {
        let key = key.into();
        let from = self.weights.get(&key).copied().unwrap_or(0.0);
        from + (f32::from(key == self.selected) - from) * self.transition.value(now)
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.transition.deadline(now, Duration::from_micros(8333))
    }
}
