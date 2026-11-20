use super::super::owner::{Clock, Reader};
use super::*;
use crate::{environment::Environment, state::Observation, view::ViewContext};
use std::time::{Duration, Instant};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

#[cfg(test)]
mod tests;

pub(super) struct Queue {
    pending: RefCell<VecDeque<Handle>>,
    evaluated: Cell<usize>,
    invalid_deadlines: Cell<usize>,
    wake: Box<dyn Fn()>,
}

impl Queue {
    fn new(wake: impl Fn() + 'static) -> Self {
        Self {
            pending: RefCell::default(),
            evaluated: Cell::default(),
            invalid_deadlines: Cell::default(),
            wake: Box::new(wake),
        }
    }

    fn enqueue(&self, handle: Handle) {
        let wake = {
            let mut pending = self.pending.borrow_mut();
            let empty = pending.is_empty();
            pending.push_back(handle);
            empty
        };
        if wake && !std::thread::panicking() {
            (self.wake)();
        }
    }
}

struct Attempt<'a> {
    graph: &'a mut InstanceGraph,
    reader: Option<Reader>,
    queue: &'a Queue,
    handle: Handle,
    complete: bool,
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            self.graph
                .banks
                .get_mut(&self.handle.bank)
                .unwrap()
                .restore_reader(self.handle, reader);
        }
        if !self.complete {
            self.queue.enqueue(self.handle);
        }
    }
}

impl InstanceGraph {
    /// Wake schedules an owner-thread update; it must not render inline.
    pub fn new(wake: impl Fn() + 'static) -> Self {
        Self {
            observations: Some(Rc::new(Queue::new(wake))),
            ..Self::default()
        }
    }

    /// Track reads for one typed input record, not a subscription per property.
    ///
    /// The reader runs on the owner thread before layout admission. Captured models
    /// and environments use the existing observation registrar. This does not
    /// declare dynamic children or replace the composite View lifecycle.
    pub fn observe<P: 'static>(
        &mut self,
        handle: Handle,
        read: impl FnMut() -> P + 'static,
    ) -> Result<()> {
        self.install_reader::<P>(handle, |observation| Reader::plain(observation, read))?
            .enqueue(handle);
        Ok(())
    }

    /// Bind contextual inputs using the same retained state, environment and
    /// animation sampling as composite views. Dynamic children are staged separately.
    pub fn observe_in<P: 'static>(
        &mut self,
        handle: Handle,
        environment: Environment,
        mut read: impl FnMut(&mut ViewContext<'_>) -> P + 'static,
    ) -> Result<()> {
        self.observe_body(handle, environment, move |context, _| Ok(read(context)))
    }

    pub(crate) fn observe_body<P: 'static>(
        &mut self,
        handle: Handle,
        environment: Environment,
        read: impl FnMut(&mut ViewContext<'_>, &mut InstanceGraph) -> Result<P> + 'static,
    ) -> Result<()> {
        let id = crate::runtime::ViewId(self.key(handle, Local(0))?.value);
        self.clock.get_or_insert_with(Clock::default);
        self.install_reader::<P>(handle, |observation| {
            Reader::contextual(observation, id, environment, read)
        })?
        .enqueue(handle);
        Ok(())
    }

    // A child discovered during evaluation needs its initial outputs in this admission.
    pub(crate) fn observe_child<P: 'static>(
        &mut self,
        handle: Handle,
        environment: Environment,
        read: impl FnMut(&mut ViewContext<'_>, &mut InstanceGraph) -> Result<P> + 'static,
    ) -> Result<()> {
        let id = crate::runtime::ViewId(self.key(handle, Local(0))?.value);
        let queue = self.install_reader::<P>(handle, |observation| {
            Reader::contextual(observation, id, environment, read)
        })?;
        self.evaluate_owner(handle, &queue)
    }

    fn install_reader<P: 'static>(
        &mut self,
        handle: Handle,
        create: impl FnOnce(Observation) -> Reader,
    ) -> Result<Rc<Queue>> {
        let bank = self
            .banks
            .get_mut(&handle.bank)
            .ok_or("instance belongs to another graph")?
            .as_any_mut()
            .downcast_mut::<Bank<P>>()
            .ok_or("input schema differs from instance")?;
        let queue = self
            .observations
            .get_or_insert_with(|| Rc::new(Queue::new(|| {})));
        let weak = Rc::downgrade(queue);
        bank.observe(
            handle,
            create(Observation::new(move || {
                if let Some(queue) = weak.upgrade() {
                    queue.enqueue(handle);
                }
            })),
        )?;
        self.timing.remove(handle);
        Ok(queue.clone())
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.timing.deadline()
    }

    pub fn motion_active(&self) -> bool {
        self.timing.moving().next().is_some()
    }

    pub fn set_frame_interval(&mut self, interval: Duration) {
        assert!(
            !interval.is_zero(),
            "animation frame interval must be positive"
        );
        let clock = self.clock.get_or_insert_with(Clock::default);
        if clock.interval != interval {
            clock.interval = interval;
            self.request_motion();
        }
    }

    pub fn set_motion_enabled(&mut self, enabled: bool) {
        let clock = self.clock.get_or_insert_with(Clock::default);
        if clock.motion != enabled {
            clock.motion = enabled;
            self.request_motion();
        }
    }

    fn request_motion(&mut self) {
        let Some(queue) = &self.observations else {
            return;
        };
        for handle in self.timing.moving() {
            if self.banks.get_mut(&handle.bank).unwrap().request(handle) == Some(true) {
                queue.enqueue(handle);
            }
        }
    }

    pub fn has_pending_inputs(&self) -> bool {
        self.observations
            .as_ref()
            .is_some_and(|queue| !queue.pending.borrow().is_empty())
    }

    pub(super) fn evaluate_inputs(
        &mut self,
        topology: Option<&Topology>,
        time: Option<crate::timeline::FrameTime>,
    ) -> Result<Work> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Observe);
        let Some(queue) = self.observations.clone() else {
            return Ok(Work::default());
        };
        let active = topology.map_or(&self.active, |plan| &plan.active);
        if let Some(time) = time
            && let Some(clock) = &mut self.clock
        {
            let changed = time.clock_changed(clock.time);
            clock.time = time;
            self.timing.take_due(time.instant, changed, |handle| {
                if active.contains(&handle)
                    && self.banks.get_mut(&handle.bank).unwrap().request(handle) == Some(true)
                {
                    queue.enqueue(handle);
                }
            });
        }
        if topology.is_some() {
            for handle in active.difference(&self.active) {
                if !self.born.contains(handle)
                    && self
                        .banks
                        .get_mut(&handle.bank)
                        .unwrap()
                        .request(*handle)
                        .is_some()
                {
                    queue.enqueue(*handle);
                }
            }
        }
        let count = queue.pending.borrow().len();
        queue.evaluated.set(0);
        queue.invalid_deadlines.set(0);
        // Mutations during evaluation remain queued for the next owner turn.
        for _ in 0..count {
            let handle = queue.pending.borrow_mut().pop_front().unwrap();
            if !topology
                .map_or(&self.active, |plan| &plan.active)
                .contains(&handle)
            {
                continue;
            }
            self.evaluate_owner(handle, &queue)?;
        }
        Ok(Work {
            owners_evaluated: queue.evaluated.get(),
            invalid_deadlines: queue.invalid_deadlines.get(),
            ..Work::default()
        })
    }

    fn evaluate_owner(&mut self, handle: Handle, queue: &Queue) -> Result<()> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Observe);
        let reader = self
            .banks
            .get_mut(&handle.bank)
            .unwrap()
            .take_reader(handle)?;
        let clock = self.clock;
        let mut attempt = Attempt {
            graph: self,
            reader,
            queue,
            handle,
            complete: false,
        };
        if let Some(reader) = &mut attempt.reader
            && let Some(deadlines) = reader.evaluate(attempt.graph, handle, clock)?
        {
            attempt.graph.timing.replace(handle, &deadlines);
            queue.evaluated.set(queue.evaluated.get() + 1);
            queue
                .invalid_deadlines
                .set(queue.invalid_deadlines.get() + deadlines.invalid);
        }
        attempt.complete = true;
        Ok(())
    }
}
