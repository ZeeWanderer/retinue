use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    Linear,
    #[default]
    Standard,
    InCubic,
    OutCubic,
    InOutCubic,
}

impl Easing {
    pub fn sample(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::Standard => standard_easing(progress),
            Self::InCubic => progress.powi(3),
            Self::OutCubic => 1.0 - (1.0 - progress).powi(3),
            Self::InOutCubic if progress < 0.5 => 4.0 * progress.powi(3),
            Self::InOutCubic => 1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0,
        }
    }
}

#[derive(Clone, Default)]
pub struct ScalarTransition {
    from: f32,
    to: f32,
    started: Option<Instant>,
    duration: Duration,
    easing: Easing,
}

impl ScalarTransition {
    pub fn set(&mut self, target: f32, now: Instant, duration: Duration) {
        self.set_eased(target, now, duration, Easing::Standard);
    }

    pub fn set_eased(&mut self, target: f32, now: Instant, duration: Duration, easing: Easing) {
        if !target.is_finite() || (target == self.to && !duration.is_zero()) {
            return;
        }
        self.from = self.value(now);
        self.to = target;
        self.started = Some(now);
        self.duration = duration;
        self.easing = easing;
    }

    pub fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.to;
        };
        if self.duration.is_zero() || now.saturating_duration_since(started) >= self.duration {
            return self.to;
        }
        if now <= started {
            return self.from;
        }
        let progress =
            now.saturating_duration_since(started).as_secs_f32() / self.duration.as_secs_f32();
        let eased = self.easing.sample(progress);
        self.from + (self.to - self.from) * eased
    }

    pub fn deadline(&self, now: Instant, frame_interval: Duration) -> Option<Instant> {
        let end = self.started? + self.duration;
        (now < end).then(|| (now + frame_interval).min(end))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Animation {
    pub duration: Duration,
    pub easing: Easing,
}

impl Animation {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::Standard,
        }
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }
}

pub trait Interpolate: Clone + PartialEq + 'static {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        ((1.0 - progress as f64) * *from as f64 + progress as f64 * *to as f64) as f32
    }
}

impl Interpolate for f64 {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        (1.0 - progress as f64) * *from + progress as f64 * *to
    }
}

impl<A: Interpolate, B: Interpolate> Interpolate for (A, B) {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        (
            A::interpolate(&from.0, &to.0, progress),
            B::interpolate(&from.1, &to.1, progress),
        )
    }
}

impl<T: Interpolate, const N: usize> Interpolate for [T; N] {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        std::array::from_fn(|index| T::interpolate(&from[index], &to[index], progress))
    }
}

#[derive(Clone)]
pub struct Tween<T> {
    from: T,
    target: T,
    progress: ScalarTransition,
    end: Option<Instant>,
}

impl<T: Clone + PartialEq> Tween<T> {
    pub fn new(value: T) -> Self {
        Self {
            from: value.clone(),
            target: value,
            progress: ScalarTransition::default(),
            end: None,
        }
    }

    pub fn target(&self) -> &T {
        &self.target
    }

    pub fn snap(&mut self, value: T) {
        self.from = value.clone();
        self.target = value;
        self.end = None;
    }

    pub fn set_with(
        &mut self,
        target: T,
        now: Instant,
        animation: Animation,
        interpolate: impl Fn(&T, &T, f32) -> T,
    ) {
        if animation.duration.is_zero() || now.checked_add(animation.duration).is_none() {
            self.snap(target);
        } else if target != self.target {
            self.from = self.value_with(now, &interpolate);
            self.target = target;
            if self.from == self.target {
                self.end = None;
                return;
            }
            self.progress = ScalarTransition::default();
            self.progress
                .set_eased(1.0, now, animation.duration, animation.easing);
            self.end = now.checked_add(animation.duration);
        }
    }

    pub fn value_with(&self, now: Instant, interpolate: impl Fn(&T, &T, f32) -> T) -> T {
        if self.finished(now) {
            self.target.clone()
        } else {
            interpolate(&self.from, &self.target, self.progress.value(now))
        }
    }

    pub fn advance(&mut self, now: Instant) {
        if self.end.is_some_and(|end| now >= end) {
            self.from = self.target.clone();
            self.end = None;
        }
    }

    pub fn deadline(&self, now: Instant, interval: Duration) -> Option<Instant> {
        let end = self.end?;
        Some(now.checked_add(interval).unwrap_or(end).min(end).max(now))
    }

    pub fn moving(&self) -> bool {
        self.end.is_some()
    }

    pub fn finished(&self, now: Instant) -> bool {
        self.end.is_none_or(|end| now >= end)
    }
}

impl<T: Interpolate> Tween<T> {
    pub fn set(&mut self, target: T, now: Instant, animation: Animation) {
        self.set_with(target, now, animation, T::interpolate);
    }

    pub fn value(&self, now: Instant) -> T {
        self.value_with(now, T::interpolate)
    }
}

// CSS cubic-bezier(0.4, 0, 0.2, 1): solve x before evaluating y.
fn standard_easing(progress: f32) -> f32 {
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        let x = 3.0 * (1.0 - t) * (1.0 - t) * t * 0.4 + 3.0 * (1.0 - t) * t * t * 0.2 + t * t * t;
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    3.0 * (1.0 - t) * t * t + t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_retargets_presentation_and_requires_one_final_sample() {
        let now = Instant::now();
        let duration = Duration::from_millis(100);
        let animation = Animation::new(duration).easing(Easing::Linear);
        let mut tween = Tween::new((0.0, 20.0));
        tween.set((100.0, 40.0), now, animation);
        let middle = now + duration / 2;
        assert_eq!(tween.value(middle), (50.0, 30.0));
        tween.set((0.0, 20.0), middle, animation);
        assert_eq!(tween.value(middle), (50.0, 30.0));
        assert_eq!(tween.value(middle + duration / 2), (25.0, 25.0));
        let late = middle + duration * 2;
        assert_eq!(tween.value(late), (0.0, 20.0));
        assert_eq!(tween.deadline(late, duration / 4), Some(late));
        tween.advance(late);
        assert_eq!(tween.deadline(late, duration / 4), None);
        assert!(!tween.moving());
    }

    #[test]
    fn tween_snap_unchanged_target_and_instant_reversal_do_not_schedule_work() {
        let now = Instant::now();
        let duration = Duration::from_millis(100);
        let animation = Animation::new(duration).easing(Easing::Linear);
        let mut tween = Tween::new(0.0);
        tween.set(1.0, now, animation);
        tween.set(1.0, now + duration / 2, animation);
        assert_eq!(tween.value(now + duration), 1.0);
        tween.snap(0.0);
        tween.set(1.0, now, animation);
        tween.set(0.0, now, animation);
        assert!(!tween.moving());
        tween.set(2.0, now, Animation::new(Duration::ZERO));
        assert_eq!(tween.value(now), 2.0);
        assert!(!tween.moving());
        tween.set(3.0, now, Animation::new(Duration::MAX));
        assert_eq!(tween.value(now), 3.0);
        assert!(!tween.moving());
    }

    #[test]
    fn custom_tween_releases_departing_resource_on_completion() {
        use std::rc::Rc;
        let now = Instant::now();
        let old = Rc::new(1);
        let weak = Rc::downgrade(&old);
        let mut tween = Tween::new(old);
        let duration = Duration::from_millis(10);
        let interpolate = |a: &Rc<i32>, b: &Rc<i32>, progress| {
            if progress < 0.5 { a.clone() } else { b.clone() }
        };
        tween.set_with(Rc::new(2), now, Animation::new(duration), interpolate);
        assert!(weak.upgrade().is_some());
        assert_eq!(*tween.value_with(now + duration, interpolate), 2);
        tween.advance(now + duration);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn cubic_retarget_and_immediate_completion_preserve_continuity() {
        let now = Instant::now();
        let duration = Duration::from_millis(170);
        let mut transition = ScalarTransition::default();
        transition.set_eased(1.0, now, duration, Easing::InOutCubic);
        assert_eq!(transition.value(now + duration / 2), 0.5);
        transition.set_eased(0.0, now + duration / 2, duration, Easing::InOutCubic);
        assert_eq!(transition.value(now + duration / 2), 0.5);
        transition.set(0.0, now + duration / 2, Duration::ZERO);
        assert_eq!(transition.value(now + duration / 2), 0.0);
        assert_eq!(
            transition.deadline(now + duration / 2, Duration::from_millis(8)),
            None
        );
    }

    #[test]
    fn retarget_starts_at_the_current_value_and_idle_has_no_deadline() {
        let now = Instant::now();
        let duration = Duration::from_millis(300);
        let frame = Duration::from_millis(16);
        let mut transition = ScalarTransition::default();
        assert_eq!(transition.deadline(now, frame), None);
        transition.set(1.0, now, duration);
        assert_eq!(transition.deadline(now, frame), Some(now + frame));
        let half = now + duration / 2;
        let value = transition.value(half);
        assert!((0.77..0.78).contains(&value));
        transition.set(0.0, half, duration);
        assert!((transition.value(half) - value).abs() < 0.0001);
        assert_eq!(transition.value(half + duration), 0.0);
        assert_eq!(transition.deadline(half + duration, frame), None);
    }
}
