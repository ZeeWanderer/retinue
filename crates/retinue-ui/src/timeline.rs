use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTime {
    pub instant: Instant,
    pub wall_time: SystemTime,
}

impl FrameTime {
    pub fn now() -> Self {
        Self {
            instant: Instant::now(),
            wall_time: SystemTime::now(),
        }
    }

    pub fn at(self, instant: Instant) -> Self {
        let wall_time = if instant >= self.instant {
            self.wall_time.checked_add(instant - self.instant)
        } else {
            self.wall_time.checked_sub(self.instant - instant)
        };
        Self {
            instant,
            wall_time: wall_time.unwrap_or(self.wall_time),
        }
    }

    pub(crate) fn clock_changed(self, previous: Self) -> bool {
        let expected = previous.at(self.instant).wall_time;
        self.wall_time
            .duration_since(expected)
            .or_else(|_| expected.duration_since(self.wall_time))
            .is_ok_and(|delta| delta >= Duration::from_millis(100))
    }
}

/// Return a strictly future deadline, or `None` when no visual update is needed.
pub trait TimelineSchedule: Clone + 'static {
    fn next(&self, time: FrameTime) -> Option<Instant>;
    fn uses_wall_clock(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Schedule {
    Periodic { start: Instant, interval: Duration },
    WallClock { interval: Duration },
    At(Instant),
    AtDate(SystemTime),
    Paused,
}

impl TimelineSchedule for Schedule {
    fn next(&self, time: FrameTime) -> Option<Instant> {
        match *self {
            Self::Periodic { start, interval } => {
                if interval.is_zero() {
                    return None;
                }
                if time.instant < start {
                    return Some(start);
                }
                time.instant
                    .checked_add(until_next(time.instant - start, interval)?)
            }
            Self::WallClock { interval } => {
                let elapsed = time.wall_time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
                time.instant.checked_add(until_next(elapsed, interval)?)
            }
            Self::At(at) => (at > time.instant).then_some(at),
            Self::AtDate(date) => time
                .instant
                .checked_add(date.duration_since(time.wall_time).ok()?)
                .filter(|at| *at > time.instant),
            Self::Paused => None,
        }
    }

    fn uses_wall_clock(&self) -> bool {
        matches!(self, Self::WallClock { .. } | Self::AtDate(_))
    }
}

fn until_next(elapsed: Duration, interval: Duration) -> Option<Duration> {
    let remainder = elapsed.as_nanos().checked_rem(interval.as_nanos())?;
    let nanos = interval.as_nanos() - remainder;
    Some(Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    ))
}

#[derive(Default)]
pub(crate) struct Deadlines {
    pub next: Option<Instant>,
    pub wall_clock: bool,
    pub invalid: usize,
    pub motion: bool,
}

impl Deadlines {
    pub fn request(&mut self, now: Instant, next: Option<Instant>) {
        if let Some(next) = next {
            if next <= now {
                self.invalid += 1;
            } else {
                self.next = self.next.into_iter().chain(Some(next)).min();
            }
        }
    }
}
