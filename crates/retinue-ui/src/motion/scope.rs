use crate::{state::Transaction, transition::Animation};
use std::{any::Any, rc::Rc};

trait Trigger: Any {
    fn value(&self) -> &dyn Any;
    fn same(&self, other: &dyn Any) -> bool;
}
impl<T: PartialEq + 'static> Trigger for T {
    fn value(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

#[derive(Clone)]
pub(crate) struct AnimationScope {
    trigger: Rc<dyn Trigger>,
    pub animation: Option<Animation>,
}

impl AnimationScope {
    pub fn new<T: PartialEq + 'static>(animation: Option<Animation>, trigger: T) -> Self {
        Self {
            trigger: Rc::new(trigger),
            animation,
        }
    }
    pub fn same_trigger(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.trigger, &other.trigger) || self.trigger.same(other.trigger.value())
    }
    pub fn apply(&self, mut transaction: Transaction) -> Transaction {
        transaction.animation = self.animation;
        transaction
    }
}

impl PartialEq for AnimationScope {
    fn eq(&self, other: &Self) -> bool {
        self.animation == other.animation && self.same_trigger(other)
    }
}
