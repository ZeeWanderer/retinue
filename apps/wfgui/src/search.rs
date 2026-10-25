use retinue_ui::transition::{Easing, ScalarTransition};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub enum Anchor {
    Leading,
    Trailing,
}

pub struct Motion {
    pub width: f32,
    content_width: f32,
    transition: ScalarTransition,
}

impl Default for Motion {
    fn default() -> Self {
        let mut transition = ScalarTransition::default();
        transition.set(32.0, Instant::now(), Duration::ZERO);
        Self {
            width: 32.0,
            content_width: 0.0,
            transition,
        }
    }
}

impl Motion {
    pub fn update(
        &mut self,
        active: bool,
        content_width: f32,
        now: Instant,
        animate: bool,
    ) -> bool {
        let required = content_width + 76.0;
        let target = if active || content_width > 0.0 {
            required.max(208.0)
        } else {
            32.0
        };
        let pressure = content_width > self.content_width && required > self.width;
        self.content_width = content_width;
        self.transition.set_eased(
            target,
            now,
            if animate && !pressure {
                Duration::from_millis(200)
            } else {
                Duration::ZERO
            },
            Easing::InOutCubic,
        );
        let width = self.transition.value(now);
        let changed = width != self.width;
        self.width = width;
        changed
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.transition.deadline(now, Duration::from_micros(8_333))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_animates_but_text_pressure_expands_immediately_and_blur_preserves_queries() {
        let now = Instant::now();
        let mut motion = Motion::default();
        motion.update(true, 0.0, now, true);
        motion.update(true, 0.0, now + Duration::from_millis(100), true);
        assert!(motion.width > 32.0 && motion.width < 208.0);
        motion.update(true, 350.0, now + Duration::from_millis(101), true);
        assert_eq!(motion.width, 426.0);
        motion.update(false, 350.0, now + Duration::from_secs(1), true);
        assert_eq!(motion.width, 426.0);
        motion.update(false, 0.0, now + Duration::from_secs(2), true);
        motion.update(false, 0.0, now + Duration::from_secs(3), true);
        assert_eq!(motion.width, 32.0);
        assert!(motion.deadline(now + Duration::from_secs(3)).is_none());
    }
}
