use crate::transition::{Animation, Easing, Interpolate};
use std::time::Duration;

pub trait MotionProgram: 'static {
    type Value: Clone;
    fn duration(&self) -> Duration;
    fn sample(&self, position: Duration) -> Self::Value;

    fn next_sample(
        &self,
        position: Duration,
        forward: bool,
        interval: Duration,
    ) -> Option<Duration> {
        if forward {
            (position < self.duration())
                .then(|| position.saturating_add(interval).min(self.duration()))
        } else {
            (!position.is_zero()).then(|| position.saturating_sub(interval))
        }
    }

    fn parallel<P: MotionProgram>(self, other: P) -> Parallel<Self, P>
    where
        Self: Sized,
    {
        Parallel(self, other)
    }
}

#[derive(Clone, PartialEq)]
struct Segment<T> {
    from: T,
    to: T,
    start: Duration,
    end: Duration,
    easing: Option<Easing>,
}

#[derive(Clone, PartialEq)]
pub struct Keyframes<T> {
    initial: T,
    segments: Vec<Segment<T>>,
}

impl<T: Interpolate> Keyframes<T> {
    pub fn new(initial: T) -> Self {
        Self {
            initial,
            segments: Vec::new(),
        }
    }

    pub fn to(mut self, value: T, animation: Animation) -> Self {
        self.append(value, animation.duration, Some(animation.easing));
        self
    }

    pub fn hold(mut self, duration: Duration) -> Self {
        self.append(self.target().clone(), duration, None);
        self
    }

    pub fn jump(mut self, value: T) -> Self {
        self.append(value, Duration::ZERO, None);
        self
    }

    fn target(&self) -> &T {
        self.segments
            .last()
            .map_or(&self.initial, |segment| &segment.to)
    }

    fn append(&mut self, value: T, duration: Duration, easing: Option<Easing>) {
        let start = self.duration();
        self.segments.push(Segment {
            from: self.target().clone(),
            to: value,
            start,
            end: start
                .checked_add(duration)
                .expect("motion duration overflow"),
            easing,
        });
    }
}

impl<T: Interpolate> MotionProgram for Keyframes<T> {
    type Value = T;
    fn duration(&self) -> Duration {
        self.segments.last().map_or(Duration::ZERO, |s| s.end)
    }

    fn sample(&self, position: Duration) -> T {
        let next = self
            .segments
            .partition_point(|segment| segment.end <= position);
        let Some(segment) = self.segments.get(next) else {
            return self.target().clone();
        };
        match segment.easing {
            Some(easing) => T::interpolate(
                &segment.from,
                &segment.to,
                easing.sample(
                    (position - segment.start).as_secs_f32()
                        / (segment.end - segment.start).as_secs_f32(),
                ),
            ),
            None => segment.from.clone(),
        }
    }

    fn next_sample(
        &self,
        position: Duration,
        forward: bool,
        interval: Duration,
    ) -> Option<Duration> {
        if !forward
            && self
                .segments
                .iter()
                .any(|s| s.start == position && s.end == position && s.from != s.to)
        {
            return (!position.is_zero()).then(|| position.saturating_sub(interval));
        }
        let candidate = if forward {
            self.segments.iter().find(|s| s.end > position)
        } else {
            self.segments.iter().rev().find(|s| s.start < position)
        }?;
        let changing = candidate.easing.is_some() && candidate.from != candidate.to;
        Some(if forward {
            if changing {
                position.saturating_add(interval).min(candidate.end)
            } else {
                candidate.end
            }
        } else if changing {
            position.saturating_sub(interval).max(candidate.start)
        } else {
            // Sampling just before a jump exposes its preceding value when reversing.
            candidate.start.saturating_sub(Duration::from_nanos(1))
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct Parallel<A, B>(pub A, pub B);

impl<A: MotionProgram, B: MotionProgram> MotionProgram for Parallel<A, B> {
    type Value = (A::Value, B::Value);
    fn duration(&self) -> Duration {
        self.0.duration().max(self.1.duration())
    }
    fn sample(&self, at: Duration) -> Self::Value {
        (
            self.0.sample(at.min(self.0.duration())),
            self.1.sample(at.min(self.1.duration())),
        )
    }
    fn next_sample(&self, at: Duration, forward: bool, interval: Duration) -> Option<Duration> {
        let next = |duration: Duration, sample: Option<Duration>| {
            if !forward && at > duration {
                Some(duration)
            } else {
                sample
            }
        };
        let a = next(
            self.0.duration(),
            self.0
                .next_sample(at.min(self.0.duration()), forward, interval),
        );
        let b = next(
            self.1.duration(),
            self.1
                .next_sample(at.min(self.1.duration()), forward, interval),
        );
        if forward {
            a.into_iter().chain(b).min()
        } else {
            a.into_iter().chain(b).max()
        }
    }
}
