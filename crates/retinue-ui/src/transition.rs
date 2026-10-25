use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
pub enum Easing {
    #[default]
    Standard,
    InOutCubic,
}

#[derive(Default)]
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
        let eased = match self.easing {
            Easing::Standard => standard_easing(progress),
            Easing::InOutCubic if progress < 0.5 => 4.0 * progress.powi(3),
            Easing::InOutCubic => 1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0,
        };
        self.from + (self.to - self.from) * eased
    }

    pub fn deadline(&self, now: Instant, frame_interval: Duration) -> Option<Instant> {
        let end = self.started? + self.duration;
        (now < end).then(|| (now + frame_interval).min(end))
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
