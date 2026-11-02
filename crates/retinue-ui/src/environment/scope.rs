use super::{EnvironmentKey, MissingModel, key::Model};
use crate::state::{State, untracked};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    fmt,
    rc::Rc,
};

type Entries = RefCell<HashMap<TypeId, Box<dyn Any>>>;

#[derive(Clone)]
pub struct Environment(Rc<Scope>);

struct Scope {
    parent: Option<Environment>,
    values: Entries,
    defaults: Rc<Entries>,
}

impl Default for Environment {
    fn default() -> Self {
        Self(Rc::new(Scope {
            parent: None,
            values: Entries::default(),
            defaults: Rc::default(),
        }))
    }
}

impl PartialEq for Environment {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Environment {}

impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Environment").finish_non_exhaustive()
    }
}

impl Environment {
    pub fn child(&self) -> Self {
        Self(Rc::new(Scope {
            parent: Some(self.clone()),
            values: Entries::default(),
            defaults: self.0.defaults.clone(),
        }))
    }

    pub fn get<K: EnvironmentKey>(&self) -> K::Value {
        let mut current = Some(self);
        while let Some(scope) = current {
            // Missing slots also participate: a nearer provider may appear later.
            if let Some(value) = scope.slot::<K>().get() {
                return value;
            }
            current = scope.0.parent.as_ref();
        }
        self.default_value::<K>()
    }

    /// Install a local override without comparing or observing its old value.
    pub fn replace<K: EnvironmentKey>(&self, value: K::Value) -> Option<K::Value> {
        self.slot::<K>().replace(Some(value))
    }

    pub fn set<K: EnvironmentKey>(&self, value: K::Value) -> bool
    where
        K::Value: PartialEq,
    {
        self.slot::<K>().set(Some(value))
    }

    pub fn remove<K: EnvironmentKey>(&self) -> Option<K::Value> {
        self.slot::<K>().update_if_changed(|current| {
            let old = current.take();
            let changed = old.is_some();
            (old, changed)
        })
    }

    pub fn provide<T: Clone + PartialEq + 'static>(&self, model: T) -> bool {
        self.set::<Model<T>>(Some(model))
    }

    pub fn replace_model<T: Clone + 'static>(&self, model: T) -> Option<T> {
        self.replace::<Model<T>>(Some(model)).flatten()
    }

    pub fn model<T: Clone + 'static>(&self) -> Option<T> {
        self.get::<Model<T>>()
    }

    pub fn require<T: Clone + 'static>(&self) -> Result<T, MissingModel> {
        self.model().ok_or_else(MissingModel::of::<T>)
    }

    pub fn remove_model<T: Clone + 'static>(&self) -> Option<T> {
        self.remove::<Model<T>>().flatten()
    }

    fn slot<K: EnvironmentKey>(&self) -> State<Option<K::Value>> {
        self.0
            .values
            .borrow_mut()
            .entry(TypeId::of::<K>())
            .or_insert_with(|| Box::new(State::new(None::<K::Value>)))
            .downcast_ref::<State<Option<K::Value>>>()
            .unwrap()
            .clone()
    }

    fn default_value<K: EnvironmentKey>(&self) -> K::Value {
        let kind = TypeId::of::<K>();
        if let Some(value) = self.0.defaults.borrow().get(&kind) {
            return value.downcast_ref::<K::Value>().unwrap().clone();
        }
        let value = untracked(K::default_value);
        self.0
            .defaults
            .borrow_mut()
            .entry(kind)
            .or_insert_with(|| Box::new(value))
            .downcast_ref::<K::Value>()
            .unwrap()
            .clone()
    }
}
