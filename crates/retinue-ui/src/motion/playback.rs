use super::MotionProgram;
use crate::state::State;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Finished,
    Cancelled,
    Superseded,
}

#[derive(Clone)]
pub struct Run(State<Option<Completion>>);

impl Run {
    pub fn result(&self) -> Option<Completion> {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackStatus {
    Paused,
    Playing,
    Finished,
    Cancelled,
}

struct Position {
    value: Duration,
    anchor: Instant,
    forward: bool,
    status: PlaybackStatus,
    run: Option<Run>,
}

impl Position {
    fn at(&self, now: Instant, duration: Duration) -> Duration {
        if self.status != PlaybackStatus::Playing {
            return self.value;
        }
        let elapsed = now.saturating_duration_since(self.anchor);
        if self.forward {
            self.value.saturating_add(elapsed).min(duration)
        } else {
            self.value.saturating_sub(elapsed)
        }
    }
    fn complete(&mut self, result: Completion) -> Option<(Run, Completion)> {
        self.run.take().map(|run| (run, result))
    }
}

struct Core<P> {
    program: P,
    position: RefCell<Position>,
    revision: State<u64>,
    status: State<PlaybackStatus>,
}

impl<P> Drop for Core<P> {
    fn drop(&mut self) {
        publish(self.position.get_mut().complete(Completion::Cancelled));
    }
}

fn publish(completion: Option<(Run, Completion)>) {
    if let Some((run, result)) = completion {
        run.0.set(Some(result));
    }
}

pub struct Playback<P>(Rc<Core<P>>);

impl<P> Clone for Playback<P> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<P> PartialEq for Playback<P> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<P: MotionProgram> Playback<P> {
    pub fn new(program: P) -> Self {
        Self(Rc::new(Core {
            program,
            revision: State::new(0),
            status: State::new(PlaybackStatus::Paused),
            position: RefCell::new(Position {
                value: Duration::ZERO,
                anchor: Instant::now(),
                forward: true,
                status: PlaybackStatus::Paused,
                run: None,
            }),
        }))
    }

    fn notify(&self) {
        let status = self.0.position.borrow().status;
        self.0.status.set(status);
        self.0
            .revision
            .update(|value| *value = value.wrapping_add(1));
    }

    pub fn play(&self, now: Instant) -> Run {
        let mut position = self.0.position.borrow_mut();
        if position.status == PlaybackStatus::Playing {
            return position.run.as_ref().unwrap().clone();
        }
        if position.run.is_none() {
            position.run = Some(Run(State::new(None)));
        }
        let run = position.run.as_ref().unwrap().clone();
        position.anchor = now;
        position.status = PlaybackStatus::Playing;
        drop(position);
        self.notify();
        run
    }

    pub fn restart(&self, now: Instant) -> Run {
        let mut position = self.0.position.borrow_mut();
        let completion = position.complete(Completion::Superseded);
        position.value = Duration::ZERO;
        position.forward = true;
        position.status = PlaybackStatus::Paused;
        drop(position);
        let run = self.play(now);
        publish(completion);
        run
    }

    pub fn pause(&self, now: Instant) {
        let mut position = self.0.position.borrow_mut();
        if position.status != PlaybackStatus::Playing {
            return;
        }
        position.value = position.at(now, self.0.program.duration());
        let finished = if position.forward {
            position.value == self.0.program.duration()
        } else {
            position.value.is_zero()
        };
        let completion = if finished {
            position.status = PlaybackStatus::Finished;
            position.complete(Completion::Finished)
        } else {
            position.status = PlaybackStatus::Paused;
            None
        };
        drop(position);
        self.notify();
        publish(completion);
    }

    pub fn seek(&self, at: Duration, now: Instant) {
        let mut position = self.0.position.borrow_mut();
        position.value = at.min(self.0.program.duration());
        position.anchor = now;
        if position.status != PlaybackStatus::Playing {
            position.status = PlaybackStatus::Paused;
        }
        drop(position);
        self.notify();
    }

    pub fn reverse(&self, now: Instant) {
        let mut position = self.0.position.borrow_mut();
        position.value = position.at(now, self.0.program.duration());
        position.forward = !position.forward;
        position.anchor = now;
        drop(position);
        self.notify();
    }

    pub fn cancel(&self, now: Instant) {
        let mut position = self.0.position.borrow_mut();
        position.value = position.at(now, self.0.program.duration());
        position.status = PlaybackStatus::Cancelled;
        let completion = position.complete(Completion::Cancelled);
        drop(position);
        self.notify();
        publish(completion);
    }

    pub fn status(&self) -> PlaybackStatus {
        self.0.status.get()
    }

    pub fn value(&self, now: Instant) -> P::Value {
        self.0.revision.get();
        self.0
            .program
            .sample(self.0.position.borrow().at(now, self.0.program.duration()))
    }

    pub(crate) fn sample(
        &self,
        now: Instant,
        interval: Duration,
        enabled: bool,
    ) -> (P::Value, Option<Instant>) {
        self.0.revision.get();
        let mut position = self.0.position.borrow_mut();
        let duration = self.0.program.duration();
        let mut at = position.at(now, duration);
        let mut next = None;
        let mut completion = None;
        if position.status == PlaybackStatus::Playing {
            if !enabled {
                at = if position.forward {
                    duration
                } else {
                    Duration::ZERO
                };
            }
            position.value = at;
            position.anchor = now;
            let finished = if position.forward {
                at == duration
            } else {
                at.is_zero()
            };
            if finished {
                position.status = PlaybackStatus::Finished;
                completion = position.complete(Completion::Finished);
            } else {
                next = self
                    .0
                    .program
                    .next_sample(at, position.forward, interval)
                    .and_then(|next| now.checked_add(next.abs_diff(at)));
            }
        }
        let status = position.status;
        drop(position);
        self.0.status.set(status);
        publish(completion);
        (self.0.program.sample(at), next)
    }
}

struct Owner<P: MotionProgram>(Playback<P>);
impl<P: MotionProgram> Drop for Owner<P> {
    fn drop(&mut self) {
        self.0.cancel(Instant::now());
    }
}

/// Retain this owner in the view; exported controls do not keep its animations alive.
pub struct PlaybackOwner<P: MotionProgram>(Rc<Owner<P>>);
impl<P: MotionProgram> Clone for PlaybackOwner<P> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<P: MotionProgram> PlaybackOwner<P> {
    pub fn new(program: P) -> Self {
        Self(Rc::new(Owner(Playback::new(program))))
    }
    pub fn playback(&self) -> Playback<P> {
        self.0.0.clone()
    }
}
