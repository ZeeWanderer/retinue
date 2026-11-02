use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    rc::Rc,
};

use super::{
    Mutation,
    registrar::{Listener, Registrar, Source, Subscription},
};

thread_local! {
    static CURRENT: RefCell<Option<Rc<Accesses>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Accesses(RefCell<HashMap<usize, Access>>);

struct Access {
    source: Rc<dyn Source>,
    revision: u64,
}

struct Scope(Option<Rc<Accesses>>);

impl Scope {
    fn enter(accesses: Option<Rc<Accesses>>) -> Self {
        Self(CURRENT.replace(accesses))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0.take());
    }
}

pub(super) fn accessed<S: Source + 'static>(source: &Rc<S>) {
    CURRENT.with_borrow(|current| {
        let Some(current) = current else { return };
        let id = Rc::as_ptr(source) as usize;
        if let Entry::Vacant(entry) = current.0.borrow_mut().entry(id) {
            entry.insert(Access {
                source: source.clone(),
                revision: source.registrar().revision(),
            });
        }
    });
}

/// Evaluate without subscribing the current observer to reads inside `read`.
pub fn untracked<R>(read: impl FnOnce() -> R) -> R {
    let _scope = Scope::enter(None);
    read()
}

/// A retained reactive consumer. Its wake callback must schedule work, not render.
///
/// Each successful `track` replaces the dependency set. Nested observations own
/// their reads; ordinary computed getters inherit the caller's tracking scope.
pub struct Observation {
    listener: Rc<Listener>,
    accesses: Rc<Accesses>,
    subscriptions: HashMap<usize, Subscription>,
}

impl Observation {
    pub fn new(wake: impl Fn() + 'static) -> Self {
        Self {
            listener: Rc::new(Listener::new(wake)),
            accesses: Rc::default(),
            subscriptions: HashMap::new(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.listener.is_dirty()
    }

    pub fn dependency_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn pending_mutation(&self) -> Option<Mutation> {
        self.listener.mutation()
    }

    pub fn track<R>(&mut self, read: impl FnOnce() -> R) -> R {
        let mutation = self.listener.mutation();
        self.listener.begin();
        let mut attempt = Attempt {
            listener: &self.listener,
            accesses: &self.accesses,
            complete: false,
            mutation,
        };
        let value = {
            let _scope = Scope::enter(Some(self.accesses.clone()));
            read()
        };
        let mut accesses = self.accesses.0.borrow_mut();
        self.subscriptions.retain(|id, _| accesses.contains_key(id));
        let mut changed_during_read = None;
        for (id, access) in accesses.drain() {
            self.subscriptions
                .entry(id)
                .or_insert_with(|| Registrar::subscribe(&access.source, &self.listener));
            if access.source.registrar().revision() != access.revision {
                changed_during_read =
                    Mutation::latest(changed_during_read, access.source.registrar().mutation());
            }
        }
        drop(accesses);
        attempt.complete = true;
        if changed_during_read.is_some() {
            self.listener.invalidate(changed_during_read);
        }
        value
    }
}

struct Attempt<'a> {
    listener: &'a Listener,
    accesses: &'a Accesses,
    complete: bool,
    mutation: Option<Mutation>,
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.accesses.0.borrow_mut().clear();
            self.listener.invalidate(self.mutation);
        }
    }
}
