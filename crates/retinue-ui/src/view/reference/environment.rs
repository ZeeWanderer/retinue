use std::{any::Any, rc::Rc};

use super::{Element, Kind};
use crate::environment::{Environment, EnvironmentKey};

pub(crate) trait Provider {
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn Provider) -> bool;
    fn apply(&self, environment: &Environment);
}

struct Value<K: EnvironmentKey>(K::Value);

impl<K: EnvironmentKey> Provider for Value<K>
where
    K::Value: PartialEq,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn Provider) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }
    fn apply(&self, environment: &Environment) {
        environment.set::<K>(self.0.clone());
    }
}

struct Model<T>(T);

impl<T: Clone + PartialEq + 'static> Provider for Model<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn Provider) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }
    fn apply(&self, environment: &Environment) {
        environment.provide(self.0.clone());
    }
}

pub(super) fn value<K: EnvironmentKey>(value: K::Value, child: Element) -> Element
where
    K::Value: PartialEq,
{
    Element::new(Kind::Environment(Rc::new(Value::<K>(value)), child))
}

pub(super) fn model<T: Clone + PartialEq + 'static>(value: T, child: Element) -> Element {
    Element::new(Kind::Environment(Rc::new(Model(value)), child))
}
