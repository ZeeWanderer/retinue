use std::{
    cell::{Ref, RefCell, RefMut},
    fmt,
    rc::{Rc, Weak},
};

use super::{
    Binding,
    observation::accessed,
    registrar::{Registrar, Source},
};

struct Storage<T> {
    value: RefCell<T>,
    registrar: Registrar,
}

impl<T> Source for Storage<T> {
    fn registrar(&self) -> &Registrar {
        &self.registrar
    }
}

/// A shared observable field, confined to the UI thread.
///
/// Cloning and equality operate on handle identity, not the stored value. Reads
/// subscribe the current observation; mutations never execute a view body.
pub struct State<T>(Rc<Storage<T>>);

pub(crate) struct WeakState<T>(Weak<Storage<T>>);
impl<T> WeakState<T> {
    pub(crate) fn upgrade(&self) -> Option<State<T>> {
        self.0.upgrade().map(State)
    }
}

impl<T: Default + 'static> Default for State<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for State<T> {}

impl<T: fmt::Debug> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("State")
            .field(&self.0.value.borrow())
            .finish()
    }
}

impl<T: 'static> State<T> {
    pub(crate) fn downgrade(&self) -> WeakState<T> {
        WeakState(Rc::downgrade(&self.0))
    }

    pub fn new(value: T) -> Self {
        Self(Rc::new(Storage {
            value: RefCell::new(value),
            registrar: Registrar::default(),
        }))
    }

    pub fn read(&self) -> Ref<'_, T> {
        accessed(&self.0);
        self.0.value.borrow()
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.read().clone()
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from(self.clone())
    }

    /// Replace without comparing or cloning a potentially large value.
    pub fn replace(&self, value: T) -> T {
        let old = self.0.value.replace(value);
        self.0.registrar.changed();
        old
    }

    /// Set only when the value differs; returns whether it changed.
    pub fn set(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        let old = {
            let mut current = self.0.value.borrow_mut();
            if *current == value {
                return false;
            }
            std::mem::replace(&mut *current, value)
        };
        self.0.registrar.changed();
        drop(old);
        true
    }

    /// Mutate in place without a snapshot. Invalidates even on a no-op or unwind.
    pub fn update<R>(&self, update: impl FnOnce(&mut T) -> R) -> R {
        self.update_if_changed(|value| (update(value), true))
    }

    pub(crate) fn update_if_changed<R>(&self, update: impl FnOnce(&mut T) -> (R, bool)) -> R {
        let mut mutation = Mutation {
            value: Some(self.0.value.borrow_mut()),
            registrar: &self.0.registrar,
            changed: true,
        };
        let (result, changed) = update(mutation.value.as_mut().unwrap());
        mutation.changed = changed;
        result
    }

    #[cfg(test)]
    pub(super) fn subscription_count(&self) -> usize {
        self.0.registrar.subscription_count()
    }
}

struct Mutation<'a, T> {
    value: Option<RefMut<'a, T>>,
    registrar: &'a Registrar,
    changed: bool,
}

impl<T> Drop for Mutation<'_, T> {
    fn drop(&mut self) {
        drop(self.value.take());
        if self.changed {
            self.registrar.changed();
        }
    }
}
