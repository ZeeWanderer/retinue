use std::{marker::PhantomData, num::NonZeroUsize, rc::Rc};

use crossbeam_deque::{Injector, Steal};

/// Ready, disjoint work inside one admitted CPU task. The caller participates;
/// this never submits another graph or waits for an external dependency.
pub struct Parallelism {
    participants: NonZeroUsize,
    local: PhantomData<Rc<()>>,
}

impl Parallelism {
    pub(super) fn new(participants: NonZeroUsize) -> Self {
        Self {
            participants,
            local: PhantomData,
        }
    }

    pub fn participants(&self) -> NonZeroUsize {
        self.participants
    }

    pub fn for_each<T: Send>(&mut self, items: &mut [T], work: impl Fn(&mut T) + Sync) {
        if items.len() < 2 || self.participants.get() == 1 {
            items.iter_mut().for_each(work);
            return;
        }
        let width = items.len().min(self.participants.get());
        let ready = Injector::new();
        for item in items {
            ready.push(item);
        }
        let consume = || loop {
            match ready.steal() {
                Steal::Success(item) => work(item),
                Steal::Empty => break,
                Steal::Retry => std::hint::spin_loop(),
            }
        };
        rayon::scope(|scope| {
            for _ in 1..width {
                scope.spawn(|_| consume());
            }
            consume();
        });
    }
}
