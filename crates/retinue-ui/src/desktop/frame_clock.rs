use std::time::Instant;

#[derive(Default)]
pub(super) struct FrameClock {
    pending: bool,
    deadline: Option<Instant>,
}

impl FrameClock {
    pub fn request(&mut self, dirty: bool, next: Option<Instant>, now: Instant) -> bool {
        let due = self.deadline.is_some_and(|time| time <= now);
        self.schedule(next);
        if !self.pending && (dirty || due || self.deadline.is_some_and(|time| time <= now)) {
            self.pending = true;
            true
        } else {
            false
        }
    }

    pub fn schedule(&mut self, next: Option<Instant>) {
        self.deadline = match (self.deadline, next) {
            (Some(previous), Some(next)) => Some(previous.min(next)),
            (_, next) => next,
        };
    }

    pub fn frame_started(&mut self) {
        self.pending = false;
        self.deadline = None;
    }

    pub fn deadline(&self) -> Option<Instant> {
        // Winit owns the compositor frame callback; do not spin waiting for it.
        if self.pending { None } else { self.deadline }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pending_frame_coalesces_wakes_and_samples_only_on_redraw() {
        let mut clock = FrameClock::default();
        let now = Instant::now();
        let due = now + Duration::from_millis(8);
        assert!(!clock.request(false, Some(due), now));
        assert_eq!(clock.deadline(), Some(due));
        assert!(clock.request(false, Some(due), due));
        assert_eq!(clock.deadline(), None);
        assert!(!clock.request(true, Some(due), due + Duration::from_millis(20)));
        assert_eq!(clock.deadline(), None);
        clock.frame_started();
        assert!(!clock.request(false, None, due));
        assert_eq!(clock.deadline(), None);
        assert!(clock.request(true, None, due));
    }

    #[test]
    fn events_do_not_postpone_or_keep_cancelled_deadlines() {
        let mut clock = FrameClock::default();
        let now = Instant::now();
        let due = now + Duration::from_millis(8);
        assert!(!clock.request(false, Some(due), now));
        assert!(!clock.request(false, Some(due + Duration::from_millis(4)), now));
        assert_eq!(clock.deadline(), Some(due));
        assert!(!clock.request(false, None, now));
        assert_eq!(clock.deadline(), None);
    }

    #[test]
    fn late_wake_still_samples_the_final_animation_position() {
        let mut clock = FrameClock::default();
        let now = Instant::now();
        clock.schedule(Some(now));
        assert!(clock.request(false, None, now + Duration::from_millis(20)));
        clock.frame_started();
        assert!(!clock.request(false, None, now));
        assert_eq!(clock.deadline(), None);
    }
}
