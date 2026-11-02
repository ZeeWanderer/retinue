use retinue_ui::transition::ScalarTransition;
use serde::Deserialize;
use std::time::{Duration, Instant};

const TRANSITION: Duration = Duration::from_millis(300);
const FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Debug, Deserialize)]
pub struct Snapshot {
    pub observed_at_ms: u64,
    pub retry_after_ms: u64,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Window {
    pub usage: f32,
    pub expires_in_ms: u64,
}

#[derive(Default)]
pub struct Budget {
    observed_at_ms: u64,
    windows: Vec<(Instant, f32)>,
    retry_at: Option<Instant>,
    cooldown_peak: u64,
    cooldown_seconds: u64,
    width: ScalarTransition,
    active: bool,
}

impl Budget {
    pub fn observe(&mut self, snapshot: Snapshot, now: Instant) {
        if snapshot.observed_at_ms < self.observed_at_ms {
            return;
        }
        self.observed_at_ms = snapshot.observed_at_ms;
        self.windows = snapshot
            .windows
            .into_iter()
            .filter(|w| w.usage.is_finite() && w.usage > 0.0 && w.expires_in_ms > 0)
            .map(|w| {
                (
                    now + Duration::from_millis(w.expires_in_ms.min(86_400_000)),
                    w.usage.min(1.0),
                )
            })
            .collect();
        self.retry_at = (snapshot.retry_after_ms > 0)
            .then(|| now + Duration::from_millis(snapshot.retry_after_ms.min(86_400_000)));
        self.tick(now);
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let was_active = self.active;
        self.windows.retain(|(until, _)| *until > now);
        self.cooldown_seconds = self.retry_at.map_or(0, |until| {
            until
                .saturating_duration_since(now)
                .as_millis()
                .div_ceil(1000) as u64
        });
        let target = if self.cooldown_seconds > 0 {
            self.cooldown_peak = self.cooldown_peak.max(self.cooldown_seconds);
            self.cooldown_seconds as f32 / self.cooldown_peak as f32
        } else {
            self.cooldown_peak = 0;
            self.windows
                .iter()
                .map(|(_, usage)| *usage)
                .fold(0.0, f32::max)
        };
        self.width.set(target, now, TRANSITION);
        self.active = target > 0.0 || self.width.value(now) > 0.0;
        was_active != self.active
    }

    pub fn visible(&self) -> bool {
        self.active
    }

    pub fn fill(&self, now: Instant) -> (f32, bool) {
        (self.width.value(now), self.cooldown_seconds > 0)
    }

    pub fn retry_at(&self) -> Option<Instant> {
        self.retry_at
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        let countdown = self
            .retry_at
            .filter(|_| self.cooldown_seconds > 0)
            .map(|until| (until - Duration::from_secs(self.cooldown_seconds - 1)).max(now));
        self.windows
            .iter()
            .map(|(until, _)| *until)
            .chain(countdown)
            .chain(self.width.deadline(now, FRAME))
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(usage: f32, expires: u64, retry: u64) -> Snapshot {
        Snapshot {
            observed_at_ms: 100,
            windows: vec![Window {
                usage,
                expires_in_ms: expires,
            }],
            retry_after_ms: retry,
        }
    }

    #[test]
    fn usage_changes_only_at_expiry_and_becomes_idle_after_fading() {
        let now = Instant::now();
        let mut budget = Budget::default();
        assert_eq!(budget.deadline(now), None);
        budget.observe(sample(0.6, 2000, 0), now);
        let settled = now + TRANSITION;
        assert_eq!(budget.fill(settled), (0.6, false));
        assert_eq!(budget.deadline(settled), Some(now + Duration::from_secs(2)));
        let expired = now + Duration::from_secs(2);
        assert!(!budget.tick(expired));
        assert!(budget.tick(expired + TRANSITION));
        assert!(!budget.visible());
        assert_eq!(budget.deadline(expired + TRANSITION), None);
    }

    #[test]
    fn cooldown_drains_right_aligned_then_reveals_remaining_usage() {
        let now = Instant::now();
        let mut budget = Budget::default();
        budget.observe(sample(0.4, 10000, 2000), now);
        assert_eq!(budget.fill(now + TRANSITION), (1.0, true));
        let next = now + Duration::from_secs(1);
        budget.tick(next);
        assert_eq!(budget.fill(next + TRANSITION), (0.5, true));
        budget.tick(next + Duration::from_secs(1));
        assert_eq!(
            budget.fill(next + Duration::from_secs(1) + TRANSITION),
            (0.4, false)
        );
        let mut old = sample(0.0, 0, 0);
        old.observed_at_ms = 99;
        budget.observe(old, next + Duration::from_secs(1) + TRANSITION);
        assert!(budget.visible());
    }
}
