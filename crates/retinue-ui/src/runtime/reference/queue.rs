use super::ViewId;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

pub(super) struct Queue {
    pending: RefCell<BTreeSet<(usize, ViewId)>>,
    deferred: RefCell<BTreeSet<(usize, ViewId)>>,
    scheduled: Cell<bool>,
    wake: Box<dyn Fn()>,
}

impl Queue {
    pub fn new(wake: impl Fn() + 'static) -> Self {
        Self {
            pending: RefCell::default(),
            deferred: RefCell::default(),
            scheduled: Cell::new(false),
            wake: Box::new(wake),
        }
    }

    pub fn wake(&self) {
        if !self.scheduled.replace(true) && !std::thread::panicking() {
            crate::state::untracked(|| (self.wake)());
        }
    }

    pub fn insert(&self, depth: usize, id: ViewId) {
        self.pending.borrow_mut().insert((depth, id));
        self.wake();
    }

    pub fn remove(&self, depth: usize, id: ViewId) {
        self.pending.borrow_mut().remove(&(depth, id));
        self.deferred.borrow_mut().remove(&(depth, id));
    }

    pub fn is_empty(&self) -> bool {
        self.pending.borrow().is_empty() && self.deferred.borrow().is_empty()
    }
    pub fn pop(&self) -> Option<(usize, ViewId)> {
        self.pending.borrow_mut().pop_first()
    }
    pub fn defer(&self, depth: usize, id: ViewId) {
        self.deferred.borrow_mut().insert((depth, id));
    }

    pub fn begin(self: &Rc<Self>) -> Turn {
        self.scheduled.set(true);
        Turn {
            queue: self.clone(),
            complete: false,
        }
    }

    pub fn attempt(self: &Rc<Self>, depth: usize, id: ViewId) -> Attempt {
        Attempt {
            queue: self.clone(),
            depth,
            id,
            complete: false,
        }
    }
}

pub(super) struct Turn {
    queue: Rc<Queue>,
    complete: bool,
}

impl Turn {
    pub fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        self.queue
            .pending
            .borrow_mut()
            .append(&mut self.queue.deferred.borrow_mut());
        self.queue.scheduled.set(false);
        if self.complete && !self.queue.is_empty() {
            self.queue.wake();
        }
    }
}

pub(super) struct Attempt {
    queue: Rc<Queue>,
    depth: usize,
    id: ViewId,
    complete: bool,
}

impl Attempt {
    pub fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.complete {
            self.queue.insert(self.depth, self.id);
        }
    }
}
