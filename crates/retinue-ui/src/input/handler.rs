use super::{Event, EventContext};
use std::{any::Any, rc::Rc};

trait Handler: Any {
    fn call(&self, event: &Event, context: &mut EventContext);
    fn value(&self) -> &dyn Any;
    fn same(&self, other: &dyn Any) -> bool;
}

struct Captured<D> {
    data: D,
    call: fn(&D, &Event, &mut EventContext),
}

impl<D: PartialEq + 'static> Handler for Captured<D> {
    fn call(&self, event: &Event, context: &mut EventContext) {
        (self.call)(&self.data, event, context);
    }
    fn value(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().is_some_and(|other| {
            self.data == other.data && std::ptr::fn_addr_eq(self.call, other.call)
        })
    }
}

struct Closure<F>(F);
impl<F: Fn(&Event, &mut EventContext) + 'static> Handler for Closure<F> {
    fn call(&self, event: &Event, context: &mut EventContext) {
        (self.0)(event, context);
    }
    fn value(&self) -> &dyn Any {
        self
    }
    fn same(&self, _: &dyn Any) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct EventHandler(Rc<dyn Handler>);

impl EventHandler {
    pub fn new(call: impl Fn(&Event, &mut EventContext) + 'static) -> Self {
        Self(Rc::new(Closure(call)))
    }

    /// Comparable captures avoid input changes when a parent body runs again.
    pub fn with<D: PartialEq + 'static>(data: D, call: fn(&D, &Event, &mut EventContext)) -> Self {
        Self(Rc::new(Captured { data, call }))
    }

    pub fn call(&self, event: &Event, context: &mut EventContext) {
        (self.0).call(event, context);
    }
}

impl PartialEq for EventHandler {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.same(other.0.value())
    }
}
