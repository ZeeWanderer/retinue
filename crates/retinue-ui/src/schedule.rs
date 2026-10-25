use std::time::{Duration, Instant};

#[cfg(feature = "overlay")]
pub use smithay_client_toolkit::reexports::calloop::ping::{Ping, PingSource, make_ping};

/// Control work must run even when the compositor withholds frames or buffers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Deadlines {
    control: Option<Instant>,
    visual: Option<Instant>,
}

impl Deadlines {
    pub fn control(&mut self, deadline: Option<Instant>) {
        self.control = earliest(self.control, deadline);
    }

    pub fn visual(&mut self, deadline: Option<Instant>) {
        self.visual = earliest(self.visual, deadline);
    }

    pub fn timeout(self, now: Instant, drawable: bool) -> Option<Duration> {
        earliest(self.control, self.visual.filter(|_| drawable))
            .map(|deadline| deadline.saturating_duration_since(now))
    }
}

fn earliest(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    left.into_iter().chain(right).min()
}

pub fn next_interval(start: Instant, interval: Duration, now: Instant) -> Instant {
    assert!(!interval.is_zero());
    let remainder = now.saturating_duration_since(start).as_nanos() % interval.as_nanos();
    now + interval
        - Duration::new(
            (remainder / 1_000_000_000) as u64,
            (remainder % 1_000_000_000) as u32,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_sleeps_until_an_event_not_a_fixed_frame_tick() {
        let now = Instant::now();
        assert_eq!(Deadlines::default().timeout(now, true), None);
        assert_eq!(Deadlines::default().timeout(now, false), None);
    }

    #[test]
    fn hidden_or_backpressured_surfaces_keep_control_deadlines() {
        let now = Instant::now();
        let mut deadlines = Deadlines::default();
        deadlines.visual(Some(now - Duration::from_secs(1)));
        assert_eq!(deadlines.timeout(now, false), None);
        deadlines.control(Some(now + Duration::from_secs(2)));
        deadlines.control(Some(now + Duration::from_secs(1)));
        assert_eq!(deadlines.timeout(now, false), Some(Duration::from_secs(1)));
        assert_eq!(deadlines.timeout(now, true), Some(Duration::ZERO));
        assert_eq!(
            deadlines.timeout(now + Duration::from_secs(3), false),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn late_animation_skips_missed_frames_and_preserves_phase() {
        let start = Instant::now();
        let interval = Duration::from_millis(500);
        for millis in [0, 499, 500, 1751, 100_000] {
            let now = start + Duration::from_millis(millis);
            assert_eq!(
                next_interval(start, interval, now),
                start + Duration::from_millis((millis / 500 + 1) * 500)
            );
        }
    }

    #[test]
    #[cfg(feature = "overlay")]
    fn wakeups_coalesce_and_survive_registration_and_dispatch_boundaries() {
        use smithay_client_toolkit::reexports::calloop::EventLoop;
        let (wake, source) = make_ping().unwrap();
        let mut events = EventLoop::<usize>::try_new().unwrap();
        for _ in 0..1000 {
            wake.ping();
        }
        events
            .handle()
            .insert_source(source, |(), _, count| *count += 1)
            .unwrap();
        let mut count = 0;
        events.dispatch(Duration::ZERO, &mut count).unwrap();
        assert_eq!(count, 1);
        events.dispatch(Duration::ZERO, &mut count).unwrap();
        assert_eq!(count, 1);
        wake.ping();
        events.dispatch(Duration::ZERO, &mut count).unwrap();
        assert_eq!(count, 2);
    }
}
