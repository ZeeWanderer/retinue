use super::{Body, Compiler, NodeSchema, Resolution, Select, View, ViewContext};
use crate::{
    environment::Environment,
    ir::{Kind, Result},
    runtime::ViewId,
    state::{Observation, State},
    timeline::{Deadlines, FrameTime},
};
use std::{cell::RefCell, marker::PhantomData, rc::Rc, time::Duration};

/// Include fixed view structure in its caller's template, retaining observation and state.
pub fn compose<V: View + PartialEq>(view: V) -> impl Body {
    fn make<V, B: Body, F: Fn(&V, &mut ViewContext<'_>) -> B + 'static>(
        view: V,
        body: F,
    ) -> Composite<V, F, B> {
        Composite {
            view,
            body,
            marker: PhantomData,
        }
    }
    make(view, |view, context| view.body(context))
}

struct Composite<V, F, B> {
    view: V,
    body: F,
    marker: PhantomData<B>,
}

struct Retained<V, P> {
    observation: Observation,
    revision: State<u64>,
    value: Option<(V, Rc<P>)>,
    environment: Environment,
    deadlines: Deadlines,
    time: FrameTime,
    interval: Duration,
    motion: bool,
    hit_testing: Option<bool>,
    discard_revision: u64,
}

struct Deref<S>(PhantomData<S>);
impl<P, T: PartialEq + 'static, S: Select<P, Value = Rc<T>>> Select<P> for Deref<S> {
    type Value = T;
    fn get(value: &P) -> Option<&T> {
        S::get(value).map(Rc::as_ref)
    }
}

impl<V: PartialEq + 'static, B: Body, F: Fn(&V, &mut ViewContext<'_>) -> B + 'static> Body
    for Composite<V, F, B>
{
    type Values = Rc<B::Values>;
    const SCHEMA: NodeSchema = NodeSchema {
        kind: Kind::Group,
        children: &[B::SCHEMA],
    };
    const NODES: usize = 1 + B::NODES;
    const BACKGROUND_TARGET: Option<crate::ir::Local> = match B::BACKGROUND_TARGET {
        Some(crate::ir::Local(index)) => Some(crate::ir::Local(index + 1)),
        None => None,
    };
    fn bind<P: 'static, S: Select<P, Value = Self::Values>>(compiler: &mut Compiler<P>) {
        compiler.node();
        compiler.composite(
            |a, b| match (S::get(a), S::get(b)) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            },
            B::bind::<P, Deref<S>>,
        );
    }
    fn resolve(
        self,
        context: &mut ViewContext<'_>,
        id: ViewId,
        resolution: &mut Resolution<'_>,
    ) -> Result<Self::Values> {
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Composite);
        context.scoped(id, |context| {
            let environment = context.environment.clone();
            let time = context.time;
            let interval = context.frame_interval;
            let motion = context.motion_enabled;
            let retained = context.owned_property::<Self, _>("body", || {
                let revision = State::new(0u64);
                let wake = revision.clone();
                Rc::new(RefCell::new(Retained::<V, B::Values> {
                    observation: Observation::new(move || wake.update(|value| *value += 1)),
                    revision,
                    value: None,
                    environment,
                    deadlines: Deadlines::default(),
                    time,
                    interval,
                    motion,
                    hit_testing: resolution.hit_testing,
                    discard_revision: resolution.graph.discard_revision,
                }))
            });
            let mut retained = retained.borrow_mut();
            // The parent observes scope invalidation, not every descendant model read.
            retained.revision.get();
            let due = retained
                .deadlines
                .next
                .is_some_and(|next| time.instant >= next)
                || (retained.deadlines.wall_clock && time.clock_changed(retained.time));
            if retained.observation.is_dirty()
                || retained
                    .value
                    .as_ref()
                    .is_none_or(|(view, _)| *view != self.view)
                || retained.environment != *context.environment
                || due
                || retained.interval != interval
                || retained.motion != motion
                || retained.hit_testing != resolution.hit_testing
                || retained.discard_revision != resolution.graph.discard_revision
            {
                #[cfg(feature = "ir-profiling")]
                let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Renew);
                let mut deadlines = Deadlines::default();
                let mut child = ViewContext {
                    deadlines: &mut deadlines,
                    ..context.reborrow()
                };
                child.mutation = retained.observation.pending_mutation().or(child.mutation);
                let value = retained.observation.track(|| {
                    let body = {
                        #[cfg(feature = "profiling")]
                        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Body);
                        (self.body)(&self.view, &mut child)
                    };
                    #[cfg(feature = "ir-profiling")]
                    let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Resolve);
                    body.resolve(&mut child, ViewId(id.0 + 1), resolution)
                })?;
                retained.value = Some((self.view, Rc::new(value)));
                retained.environment = context.environment.clone();
                retained.deadlines = deadlines;
                retained.time = time;
                retained.interval = interval;
                retained.motion = motion;
                retained.hit_testing = resolution.hit_testing;
                retained.discard_revision = resolution.graph.discard_revision;
                context.deadlines.invalid += deadlines.invalid;
            }
            context
                .deadlines
                .request(time.instant, retained.deadlines.next);
            context.deadlines.wall_clock |= retained.deadlines.wall_clock;
            context.deadlines.motion |= retained.deadlines.motion;
            Ok(retained.value.as_ref().unwrap().1.clone())
        })
    }
}
