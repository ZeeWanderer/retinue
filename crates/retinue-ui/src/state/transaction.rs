use std::cell::Cell;

use crate::transition::Animation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transaction {
    pub animation: Option<Animation>,
    pub disables_animations: bool,
}

thread_local! {
    static CURRENT: Cell<Transaction> = Cell::new(Transaction::default());
    static SEQUENCE: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mutation {
    pub sequence: u64,
    pub transaction: Transaction,
}

impl Mutation {
    pub fn capture() -> Self {
        let sequence = SEQUENCE
            .get()
            .checked_add(1)
            .expect("mutation sequence exhausted");
        SEQUENCE.set(sequence);
        Self {
            sequence,
            transaction: CURRENT.get(),
        }
    }

    pub fn latest(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        a.into_iter().chain(b).max_by_key(|change| change.sequence)
    }
}

pub fn with_transaction<R>(transaction: Transaction, change: impl FnOnce() -> R) -> R {
    struct Scope(Transaction);
    impl Drop for Scope {
        fn drop(&mut self) {
            CURRENT.set(self.0);
        }
    }
    let _scope = Scope(CURRENT.replace(transaction));
    change()
}

pub fn with_animation<R>(animation: Animation, change: impl FnOnce() -> R) -> R {
    with_transaction(
        Transaction {
            animation: Some(animation),
            ..CURRENT.get()
        },
        change,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Observation, State};
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
        time::Duration,
    };

    #[test]
    fn wake_is_coalesced_but_mutation_provenance_is_not_discarded() {
        let state = State::new(0);
        let wakes = Rc::new(Cell::new(0));
        let mut observation = Observation::new({
            let wakes = wakes.clone();
            move || wakes.set(wakes.get() + 1)
        });
        observation.track(|| state.get());
        let animation = Animation::new(Duration::from_millis(80));
        with_animation(animation, || {
            state.set(1);
        });
        let first = observation.pending_mutation().unwrap();
        assert_eq!(first.transaction.animation, Some(animation));
        state.set(2);
        let last = observation.pending_mutation().unwrap();
        assert!(last.sequence > first.sequence);
        assert_eq!(last.transaction, Transaction::default());
        assert_eq!(wakes.get(), 1);
        state.set(2);
        assert_eq!(observation.pending_mutation(), Some(last));
        observation.track(|| state.get());
        assert_eq!(observation.pending_mutation(), None);
    }

    #[test]
    fn nested_scopes_restore_on_panic_and_preserve_disable_intent() {
        let transaction = Transaction {
            animation: None,
            disables_animations: true,
        };
        with_transaction(transaction, || {
            let animation = Animation::new(Duration::from_millis(10));
            let result = catch_unwind(AssertUnwindSafe(|| {
                with_animation(animation, || {
                    assert_eq!(
                        Mutation::capture().transaction,
                        Transaction {
                            animation: Some(animation),
                            disables_animations: true
                        }
                    );
                    panic!("intentional transaction failure");
                })
            }));
            assert!(result.is_err());
            assert_eq!(Mutation::capture().transaction, transaction);
        });
        assert_eq!(Mutation::capture().transaction, Transaction::default());
    }

    #[test]
    fn subscription_created_after_a_mutation_during_tracking_keeps_its_transaction() {
        let state = State::new(0);
        let mut observation = Observation::new(|| {});
        let animation = Animation::new(Duration::from_millis(80));
        observation.track(|| {
            state.get();
            with_animation(animation, || {
                state.set(1);
            });
        });
        assert!(observation.is_dirty());
        assert_eq!(
            observation
                .pending_mutation()
                .unwrap()
                .transaction
                .animation,
            Some(animation)
        );
    }

    #[test]
    fn unrelated_mutations_do_not_replace_a_consumers_cause() {
        let a = State::new(0);
        let b = State::new(0);
        let mut observation = Observation::new(|| {});
        observation.track(|| a.get());
        let animation = Animation::new(Duration::from_millis(80));
        with_animation(animation, || {
            a.set(1);
        });
        b.set(2);
        assert_eq!(
            observation
                .pending_mutation()
                .unwrap()
                .transaction
                .animation,
            Some(animation)
        );
    }
}
