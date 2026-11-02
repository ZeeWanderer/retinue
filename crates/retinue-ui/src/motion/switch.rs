use crate::{
    Key,
    transition::{Animation, Easing, Tween},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranslationTransition {
    pub enter: (f32, f32),
    pub exit: (f32, f32),
    pub duration: Duration,
    pub easing: Easing,
}

impl TranslationTransition {
    pub fn new(enter: (f32, f32), exit: (f32, f32), duration: Duration) -> Self {
        Self {
            enter,
            exit,
            duration,
            easing: Easing::Standard,
        }
    }
    pub(crate) fn valid(self) -> bool {
        [self.enter.0, self.enter.1, self.exit.0, self.exit.1]
            .into_iter()
            .all(f32::is_finite)
            && Instant::now().checked_add(self.duration).is_some()
    }
}

#[derive(Clone)]
struct OffsetTrack(Tween<(f32, f32)>);

impl OffsetTrack {
    fn new(
        from: (f32, f32),
        to: (f32, f32),
        spec: TranslationTransition,
        now: Instant,
    ) -> Option<Self> {
        let mut tween = Tween::new(from);
        tween.set(to, now, Animation::new(spec.duration).easing(spec.easing));
        tween.moving().then_some(Self(tween))
    }
    fn value(&self, now: Instant) -> (f32, f32) {
        self.0.value(now)
    }
}

#[derive(Clone)]
pub(crate) struct ContentState {
    pub selected: Option<Key>,
    pub departing: Option<Key>,
    entry: Option<OffsetTrack>,
    exit: Option<OffsetTrack>,
}

impl ContentState {
    pub fn new(selected: Option<Key>) -> Self {
        Self {
            selected,
            departing: None,
            entry: None,
            exit: None,
        }
    }
    pub fn moving(&self) -> bool {
        self.entry.is_some() || self.exit.is_some()
    }
    pub fn offset(&self, key: Key, now: Instant) -> (f32, f32) {
        let track = if Some(key) == self.selected {
            self.entry.as_ref()
        } else if Some(key) == self.departing {
            self.exit.as_ref()
        } else {
            None
        };
        track.map_or((0.0, 0.0), |track| track.value(now))
    }
    pub fn retiring(&self, now: Instant) -> bool {
        self.exit
            .as_ref()
            .is_some_and(|track| track.0.finished(now))
    }
    pub fn advance(&mut self, now: Instant, enabled: bool) {
        if !enabled
            || self
                .entry
                .as_ref()
                .is_some_and(|track| track.0.finished(now))
        {
            self.entry = None;
        }
        if !enabled || self.retiring(now) {
            self.exit = None;
            self.departing = None;
        }
    }
    pub fn select(
        &mut self,
        selected: Option<Key>,
        spec: Option<TranslationTransition>,
        now: Instant,
        enabled: bool,
    ) {
        self.advance(now, enabled);
        if self.selected == selected {
            return;
        }
        let Some(spec) = spec.filter(|spec| enabled && !spec.duration.is_zero()) else {
            *self = Self::new(selected);
            return;
        };
        let enter = selected.map_or((0.0, 0.0), |key| {
            if Some(key) == self.departing {
                self.offset(key, now)
            } else {
                spec.enter
            }
        });
        let exit = self.selected.map(|key| self.offset(key, now));
        self.exit = exit.and_then(|from| OffsetTrack::new(from, spec.exit, spec, now));
        self.departing = self.selected.filter(|_| self.exit.is_some());
        self.selected = selected;
        self.entry = selected.and_then(|_| OffsetTrack::new(enter, (0.0, 0.0), spec, now));
    }
    pub fn deadline(&self, now: Instant, interval: Duration) -> Option<Instant> {
        [self.entry.as_ref(), self.exit.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|track| track.0.deadline(now, interval))
            .min()
    }
}
