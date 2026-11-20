use super::{Handle, InstanceGraph, Result};
use crate::{
    environment::Environment,
    runtime::ViewId,
    state::{Mutation, Observation},
    timeline::{Deadlines, FrameTime},
    view::{Owned, ViewContext},
};
use std::time::Duration;

#[derive(Clone, Copy)]
pub(super) struct Clock {
    pub time: FrameTime,
    pub interval: Duration,
    pub motion: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            time: FrameTime::now(),
            interval: Duration::from_millis(16),
            motion: true,
        }
    }
}

type Read =
    dyn FnMut(&mut InstanceGraph, Handle, Option<Mutation>, Option<Clock>) -> Result<Deadlines>;

pub(super) struct Reader {
    observation: Observation,
    read: Box<Read>,
    requested: bool,
}

impl Reader {
    pub fn plain<P: 'static>(
        observation: Observation,
        mut read: impl FnMut() -> P + 'static,
    ) -> Self {
        Self {
            observation,
            read: Box::new(move |graph, handle, _, _| {
                graph.stage(handle, read())?;
                Ok(Deadlines::default())
            }),
            requested: false,
        }
    }

    pub fn contextual<P: 'static>(
        observation: Observation,
        id: ViewId,
        environment: Environment,
        mut read: impl FnMut(&mut ViewContext<'_>, &mut InstanceGraph) -> Result<P> + 'static,
    ) -> Self {
        let mut owned = Owned::default();
        Self {
            observation,
            read: Box::new(move |graph, handle, cause, clock| {
                let clock = clock.expect("contextual owner requires a frame clock");
                let mut deadlines = Deadlines::default();
                let value = read(
                    &mut ViewContext {
                        id,
                        environment: &environment,
                        owned: &mut owned,
                        mutation: cause,
                        time: clock.time,
                        deadlines: &mut deadlines,
                        frame_interval: clock.interval,
                        motion_enabled: clock.motion,
                    },
                    graph,
                )?;
                graph.stage(handle, value)?;
                Ok(deadlines)
            }),
            requested: false,
        }
    }

    pub fn request(&mut self) -> bool {
        let enqueue = !self.requested && !self.observation.is_dirty();
        self.requested = true;
        enqueue
    }

    pub fn evaluate(
        &mut self,
        graph: &mut InstanceGraph,
        handle: Handle,
        clock: Option<Clock>,
    ) -> Result<Option<Deadlines>> {
        if !self.requested && !self.observation.is_dirty() {
            return Ok(None);
        }
        self.requested = true;
        let cause = self.observation.pending_mutation();
        let value = self.observation.track(|| {
            #[cfg(feature = "ir-profiling")]
            let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Resolve);
            (self.read)(graph, handle, cause, clock)
        })?;
        self.requested = false;
        Ok(Some(value))
    }
}
