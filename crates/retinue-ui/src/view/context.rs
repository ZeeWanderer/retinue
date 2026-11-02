use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::{
    environment::Environment,
    runtime::ViewId,
    state::{Mutation, State, untracked},
    timeline::{Deadlines, FrameTime, TimelineSchedule},
    transition::{Animation, Interpolate, Tween},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Hash, PartialEq, Eq)]
enum Slot {
    Named(&'static str),
    Property(TypeId, &'static str),
}

#[derive(Default)]
pub(crate) struct Owned(HashMap<Slot, Box<dyn Any>>);

pub struct ViewContext<'a> {
    pub(crate) id: ViewId,
    pub(crate) environment: &'a Environment,
    pub(crate) owned: &'a mut Owned,
    pub(crate) mutation: Option<Mutation>,
    pub(crate) time: FrameTime,
    pub(crate) deadlines: &'a mut Deadlines,
    pub(crate) frame_interval: Duration,
    pub(crate) motion_enabled: bool,
}

impl ViewContext<'_> {
    pub fn playback<P: crate::motion::MotionProgram>(
        &mut self,
        playback: &crate::motion::Playback<P>,
    ) -> P::Value {
        let enabled = self.motion_enabled
            && !self.environment.get::<crate::motion::ReduceMotion>()
            && !self
                .mutation
                .is_some_and(|cause| cause.transaction.disables_animations);
        let (value, next) = playback.sample(self.time.instant, self.frame_interval, enabled);
        self.deadlines.motion |= next.is_some();
        self.deadlines.request(self.time.instant, next);
        value
    }

    pub fn properties<P: super::ViewProperties>(&mut self) -> P {
        P::resolve(self)
    }

    pub fn owned_property<P: 'static, T: Clone + 'static>(
        &mut self,
        name: &'static str,
        initialize: impl FnOnce() -> T,
    ) -> T {
        self.retain(Slot::Property(TypeId::of::<P>(), name), initialize)
    }

    /// Retained presentation value; model state already contains the final target.
    pub fn animated<T: Interpolate>(&mut self, name: &'static str, target: T) -> T {
        let animation = self.mutation.and_then(|cause| cause.transaction.animation);
        self.sample(name, target, animation)
    }

    pub fn animated_with<T: Interpolate>(
        &mut self,
        name: &'static str,
        target: T,
        animation: Animation,
    ) -> T {
        self.sample(name, target, Some(animation))
    }

    fn sample<T: Interpolate>(
        &mut self,
        name: &'static str,
        target: T,
        animation: Option<Animation>,
    ) -> T {
        let track = self.owned(name, || Rc::new(RefCell::new(Tween::new(target.clone()))));
        let reduced = !self.motion_enabled
            || self.environment.get::<crate::motion::ReduceMotion>()
            || self
                .mutation
                .is_some_and(|cause| cause.transaction.disables_animations);
        let mut track = track.borrow_mut();
        if reduced {
            track.snap(target);
        } else if track.target() != &target {
            match animation {
                Some(animation) => track.set(target, self.time.instant, animation),
                None => track.snap(target),
            }
        }
        let value = track.value(self.time.instant);
        track.advance(self.time.instant);
        self.deadlines.motion |= track.moving();
        self.deadlines.request(
            self.time.instant,
            track.deadline(self.time.instant, self.frame_interval),
        );
        value
    }

    pub fn time(&mut self, schedule: impl TimelineSchedule) -> FrameTime {
        self.deadlines.wall_clock |= schedule.uses_wall_clock();
        self.deadlines
            .request(self.time.instant, schedule.next(self.time));
        self.time
    }

    pub fn id(&self) -> ViewId {
        self.id
    }

    pub fn environment(&self) -> &Environment {
        self.environment
    }

    pub fn mutation(&self) -> Option<Mutation> {
        self.mutation
    }

    /// Initialize once per retained view identity. Return a cheap model handle.
    pub fn owned<T: Clone + 'static>(
        &mut self,
        name: &'static str,
        initialize: impl FnOnce() -> T,
    ) -> T {
        self.retain(Slot::Named(name), initialize)
    }

    fn retain<T: Clone + 'static>(&mut self, slot: Slot, initialize: impl FnOnce() -> T) -> T {
        self.owned
            .0
            .entry(slot)
            .or_insert_with(|| Box::new(untracked(initialize)))
            .downcast_ref::<T>()
            .expect("a retained field changed type without changing view identity")
            .clone()
    }

    pub fn state<T: 'static>(
        &mut self,
        name: &'static str,
        initialize: impl FnOnce() -> T,
    ) -> State<T> {
        self.owned(name, || State::new(initialize()))
    }
}
