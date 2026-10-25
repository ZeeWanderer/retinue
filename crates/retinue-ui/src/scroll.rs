use crate::transition::ScalarTransition;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Scroll {
    position: ScalarTransition,
    target: f32,
    maximum: f32,
}

impl Scroll {
    pub fn resize(&mut self, content: f32, viewport: f32, now: Instant) {
        self.maximum = (content - viewport).max(0.0);
        if self.target > self.maximum {
            self.to(self.maximum, false, now);
        }
    }

    pub fn offset(&self, now: Instant) -> f32 {
        self.position.value(now).clamp(0.0, self.maximum)
    }
    pub fn maximum(&self) -> f32 {
        self.maximum
    }

    pub fn by(&mut self, delta: f32, smooth: bool, now: Instant) {
        self.to(self.target + delta, smooth, now);
    }

    pub fn to(&mut self, position: f32, smooth: bool, now: Instant) {
        if !position.is_finite() {
            return;
        }
        self.target = position.clamp(0.0, self.maximum);
        self.position.set(
            self.target,
            now,
            if smooth {
                Duration::from_millis(120)
            } else {
                Duration::ZERO
            },
        );
    }

    pub fn reveal(&mut self, top: f32, bottom: f32, viewport: f32, now: Instant) {
        if top < self.target {
            self.to(top, false, now);
        } else if bottom > self.target + viewport {
            self.to(bottom - viewport, false, now);
        }
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.position.deadline(now, Duration::from_millis(16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_clamps_reveals_and_stops_waking_when_settled() {
        let now = Instant::now();
        let mut scroll = Scroll::default();
        scroll.resize(1200.0, 400.0, now);
        scroll.by(300.0, true, now);
        assert_eq!(scroll.offset(now), 0.0);
        assert!(scroll.offset(now + Duration::from_millis(60)) > 200.0);
        let settled = now + Duration::from_secs(1);
        assert_eq!(scroll.offset(settled), 300.0);
        assert_eq!(scroll.deadline(settled), None);
        scroll.reveal(900.0, 1000.0, 400.0, settled);
        assert_eq!(scroll.offset(settled), 600.0);
        scroll.resize(500.0, 400.0, settled);
        assert_eq!(scroll.offset(settled), 100.0);
        scroll.to(f32::NAN, false, settled);
        assert_eq!(scroll.offset(settled), 100.0);
    }
}
