use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

use super::{Mutation, observation::untracked};

pub(super) trait Source {
    fn registrar(&self) -> &Registrar;
}

#[derive(Default)]
pub(super) struct Registrar {
    revision: Cell<u64>,
    mutation: Cell<Option<Mutation>>,
    next_subscription: Cell<u64>,
    listeners: RefCell<HashMap<u64, Weak<Listener>>>,
}

pub(super) struct Listener {
    dirty: Cell<bool>,
    mutation: Cell<Option<Mutation>>,
    wake: Box<dyn Fn()>,
}

impl Listener {
    pub fn new(wake: impl Fn() + 'static) -> Self {
        Self {
            dirty: Cell::new(true),
            mutation: Cell::new(None),
            wake: Box::new(wake),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub fn begin(&self) {
        self.dirty.set(false);
        self.mutation.set(None);
    }

    pub fn mutation(&self) -> Option<Mutation> {
        self.mutation.get()
    }

    pub fn invalidate(&self, mutation: Option<Mutation>) {
        self.mutation
            .set(Mutation::latest(self.mutation.get(), mutation));
        if !self.dirty.replace(true) && !std::thread::panicking() {
            untracked(|| (self.wake)());
        }
    }
}

impl Registrar {
    pub fn revision(&self) -> u64 {
        self.revision.get()
    }

    pub fn mutation(&self) -> Option<Mutation> {
        self.mutation.get()
    }

    pub fn subscribe(source: &Rc<dyn Source>, listener: &Rc<Listener>) -> Subscription {
        let registrar = source.registrar();
        let id = registrar.next_subscription.get();
        registrar.next_subscription.set(id.checked_add(1).unwrap());
        registrar
            .listeners
            .borrow_mut()
            .insert(id, Rc::downgrade(listener));
        Subscription {
            source: Rc::downgrade(source),
            id,
        }
    }

    pub fn changed(&self) {
        let mutation = Mutation::capture();
        self.mutation.set(Some(mutation));
        self.revision.set(self.revision.get().wrapping_add(1));
        // Release the registry before waking a host, which may drop subscriptions.
        let listeners: Vec<_> = self
            .listeners
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for listener in listeners {
            listener.invalidate(Some(mutation));
        }
    }

    #[cfg(test)]
    pub fn subscription_count(&self) -> usize {
        self.listeners.borrow().len()
    }
}

pub(super) struct Subscription {
    source: Weak<dyn Source>,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(source) = self.source.upgrade() {
            source.registrar().listeners.borrow_mut().remove(&self.id);
        }
    }
}
