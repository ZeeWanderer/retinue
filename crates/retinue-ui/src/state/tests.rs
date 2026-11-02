use super::{Observation, State, untracked};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

fn observe() -> (Observation, Rc<Cell<usize>>) {
    let wakes = Rc::new(Cell::new(0));
    let output = wakes.clone();
    (
        Observation::new(move || output.set(output.get() + 1)),
        wakes,
    )
}

#[test]
fn observes_fields_not_whole_models_and_coalesces_wakes() {
    struct Model {
        name: State<String>,
        rank: State<u32>,
    }
    let model = Model {
        name: State::new("Karak".into()),
        rank: State::new(1),
    };
    let (mut observation, wakes) = observe();
    assert!(observation.is_dirty());
    assert_eq!(observation.track(|| model.rank.get()), 1);
    assert!(!observation.is_dirty());
    model.name.set("Vasto".into());
    assert!(!observation.is_dirty());
    assert!(!model.rank.set(1));
    assert!(!observation.is_dirty());
    model.rank.set(2);
    model.rank.set(3);
    assert!(observation.is_dirty());
    assert_eq!(wakes.get(), 1);
    assert_eq!(observation.track(|| model.rank.get()), 3);
    model.rank.set(4);
    assert_eq!(wakes.get(), 2);
}

#[test]
fn nested_models_and_computed_getters_follow_the_selected_instance() {
    #[derive(Clone, PartialEq)]
    struct Item {
        rank: State<u32>,
        bonus: State<u32>,
    }
    impl Item {
        fn total(&self) -> u32 {
            self.rank.get() + self.bonus.get()
        }
    }
    let first = Item {
        rank: State::new(2),
        bonus: State::new(3),
    };
    let second = Item {
        rank: State::new(10),
        bonus: State::new(20),
    };
    let selected = State::new(first.clone());
    let (mut observation, wakes) = observe();
    assert_eq!(observation.track(|| selected.read().total()), 5);
    assert_eq!(observation.dependency_count(), 3);
    second.rank.set(11);
    assert_eq!(wakes.get(), 0);
    selected.set(second.clone());
    assert_eq!(wakes.get(), 1);
    assert_eq!(observation.track(|| selected.read().total()), 31);
    assert_eq!(first.rank.subscription_count(), 0);
    assert_eq!(first.bonus.subscription_count(), 0);
    first.rank.set(4);
    assert!(!observation.is_dirty());
    second.bonus.set(25);
    assert!(observation.is_dirty());
}

#[test]
fn branches_replace_dependencies_instead_of_accumulating_them() {
    let expanded = State::new(true);
    let details = State::new(7);
    let (mut observation, wakes) = observe();
    let body = || expanded.get().then(|| details.get());
    assert_eq!(observation.track(body), Some(7));
    assert_eq!(observation.dependency_count(), 2);
    expanded.set(false);
    assert_eq!(observation.track(body), None);
    assert_eq!(observation.dependency_count(), 1);
    assert_eq!(details.subscription_count(), 0);
    details.set(8);
    assert!(!observation.is_dirty());
    assert_eq!(wakes.get(), 1);
    observation.track(|| ());
    assert_eq!(observation.dependency_count(), 0);
    expanded.set(true);
    assert!(!observation.is_dirty());
}

#[test]
fn keyed_collection_order_and_individual_items_are_independent() {
    let a = State::new(1);
    let b = State::new(2);
    let order = State::new(vec![a.clone(), b.clone()]);
    let (mut list, _) = observe();
    let (mut first, _) = observe();
    let (mut second, _) = observe();
    list.track(|| order.get());
    first.track(|| a.get());
    second.track(|| b.get());
    b.set(3);
    assert!(!list.is_dirty());
    assert!(!first.is_dirty());
    assert!(second.is_dirty());
    second.track(|| b.get());
    order.update(|items| items.reverse());
    assert!(list.is_dirty());
    assert!(!first.is_dirty());
    assert!(!second.is_dirty());
}

#[test]
fn nested_consumers_own_their_reads_and_restore_the_parent_scope() {
    let outer_field = State::new(1);
    let child_field = State::new(2);
    let after_child = State::new(3);
    let (mut parent, _) = observe();
    let (mut child, _) = observe();
    parent.track(|| {
        outer_field.get();
        child.track(|| child_field.get());
        after_child.get();
    });
    assert_eq!(parent.dependency_count(), 2);
    assert_eq!(child.dependency_count(), 1);
    child_field.set(4);
    assert!(child.is_dirty());
    assert!(!parent.is_dirty());
    after_child.set(5);
    assert!(parent.is_dirty());
}

#[test]
fn repeated_reads_and_retracking_reuse_one_subscription() {
    let state = State::new(1);
    let (mut observation, _) = observe();
    for _ in 0..100 {
        observation.track(|| {
            for _ in 0..100 {
                state.get();
            }
        });
        assert_eq!(state.subscription_count(), 1);
        assert_eq!(observation.dependency_count(), 1);
    }
    drop(observation);
    assert_eq!(state.subscription_count(), 0);
}

#[test]
fn subscriptions_do_not_keep_models_or_observers_alive() {
    let (mut observation, _) = observe();
    let weak = {
        let lifetime = Rc::new(());
        let weak = Rc::downgrade(&lifetime);
        let state = State::new(lifetime);
        observation.track(|| state.get());
        weak
    };
    assert!(weak.upgrade().is_none());
    observation.track(|| ());
    assert_eq!(observation.dependency_count(), 0);
}

#[test]
fn borrowed_reads_and_mutation_need_no_clone_or_comparison() {
    struct Data(Vec<u32>);
    let state = State::new(Data(vec![1, 2]));
    let (mut observation, wakes) = observe();
    observation.track(|| assert_eq!(state.read().0, [1, 2]));
    state.update(|data| data.0.push(3));
    assert_eq!(wakes.get(), 1);
    observation.track(|| assert_eq!(state.read().0, [1, 2, 3]));
    let old = state.replace(Data(vec![]));
    assert_eq!(old.0, [1, 2, 3]);
    assert_eq!(wakes.get(), 2);
}

#[test]
fn mutations_during_evaluation_are_not_lost() {
    let state = State::new(1);
    let (mut observation, wakes) = observe();
    assert_eq!(
        observation.track(|| {
            let old = state.get();
            state.set(2);
            old
        }),
        1
    );
    assert!(observation.is_dirty());
    assert_eq!(wakes.get(), 1);
    assert_eq!(observation.track(|| state.get()), 2);
    assert!(!observation.is_dirty());
}

#[test]
fn untracked_and_debug_reads_do_not_create_dependencies() {
    let ignored = State::new(1);
    let tracked = State::new(2);
    let (mut observation, _) = observe();
    observation.track(|| {
        assert_eq!(untracked(|| ignored.get()), 1);
        assert_eq!(format!("{ignored:?}"), "State(1)");
        tracked.get();
    });
    assert_eq!(observation.dependency_count(), 1);
    ignored.set(3);
    assert!(!observation.is_dirty());
    tracked.set(4);
    assert!(observation.is_dirty());
}

#[test]
fn notification_releases_the_value_borrow_and_suppresses_tracking() {
    let state = State::new(1);
    let read_by_wake = State::new(10);
    let value = state.clone();
    let extra = read_by_wake.clone();
    let seen = Rc::new(Cell::new(0));
    let output = seen.clone();
    let mut observation = Observation::new(move || output.set(value.get() + extra.get()));
    observation.track(|| state.get());
    let (mut other, _) = observe();
    other.track(|| state.update(|value| *value = 2));
    assert_eq!(seen.get(), 12);
    assert_eq!(other.dependency_count(), 0);
}

#[test]
fn failed_evaluation_keeps_previous_dependencies_and_restores_scope() {
    let old = State::new(1);
    let temporary = State::new(2);
    let parent_field = State::new(3);
    let (mut parent, _) = observe();
    let (mut child, _) = observe();
    child.track(|| old.get());
    parent.track(|| {
        let result = catch_unwind(AssertUnwindSafe(|| {
            child.track(|| {
                temporary.get();
                panic!("abandoned body");
            });
        }));
        assert!(result.is_err());
        parent_field.get();
    });
    assert!(child.is_dirty());
    assert!(!parent.is_dirty());
    assert_eq!(parent.dependency_count(), 1);
    assert_eq!(old.subscription_count(), 1);
    assert_eq!(temporary.subscription_count(), 0);
    child.track(|| old.get());
    old.set(4);
    assert!(child.is_dirty());
    assert!(!parent.is_dirty());
}

#[test]
fn mutation_unwind_invalidates_partially_changed_values() {
    let state = State::new(vec![1]);
    let (mut observation, _) = observe();
    observation.track(|| state.get());
    let result = catch_unwind(AssertUnwindSafe(|| {
        state.update(|values| {
            values.push(2);
            panic!("failed edit");
        });
    }));
    assert!(result.is_err());
    assert!(observation.is_dirty());
    assert_eq!(observation.track(|| state.get()), [1, 2]);
    assert!(!observation.is_dirty());
}

#[test]
fn handle_comparison_is_identity_not_stored_data() {
    let a = State::new(1);
    let b = State::new(1);
    assert_eq!(a, a.clone());
    assert_ne!(a, b);
    a.set(2);
    assert_eq!(a, a.clone());
}
