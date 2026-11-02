use super::*;
use crate::state::Observation;
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
};

#[test]
fn edits_the_source_without_observing_binding_construction() {
    let value = State::new(3);
    let wakes = Rc::new(Cell::new(0));
    let wake = wakes.clone();
    let mut observer = Observation::new(move || wake.set(wake.get() + 1));
    let binding = observer.track(|| value.binding());
    assert_eq!(observer.dependency_count(), 0);
    assert_eq!(observer.track(|| binding.get()), Ok(3));
    assert_eq!(binding.set(3), Ok(false));
    assert!(!observer.is_dirty());
    assert_eq!(binding.set(4), Ok(true));
    assert_eq!(value.get(), 4);
    assert_eq!(binding.clone().replace(5), Ok(4));
    assert_eq!(wakes.get(), 1);
    assert_eq!(observer.track(|| value.get()), 5);
    binding.update(|value| *value += 2).unwrap();
    assert_eq!(binding.get(), Ok(7));
    assert_eq!(wakes.get(), 2);
}

#[test]
fn borrowed_projection_does_not_require_cloning_the_payload() {
    struct Payload {
        name: String,
        unused: Vec<u8>,
    }
    let value = State::new(Payload {
        name: "item".into(),
        unused: vec![0; 4096],
    });
    let name = value
        .binding()
        .map(|value| &value.name, |value| &mut value.name);
    let mut observer = Observation::new(|| {});
    assert_eq!(observer.track(|| name.with(String::len)), Ok(4));
    assert_eq!(name.set("item".into()), Ok(false));
    assert!(!observer.is_dirty());
    name.update(|name| name.push('!')).unwrap();
    assert!(observer.is_dirty());
    assert_eq!(value.read().name, "item!");
    assert_eq!(value.read().unused.len(), 4096);
}

#[derive(Clone, PartialEq)]
struct Item {
    id: u32,
    rank: State<u32>,
}

#[test]
fn follows_parent_replacement_for_reads_writes_and_subscriptions() {
    let first = Item {
        id: 1,
        rank: State::new(2),
    };
    let second = Item {
        id: 2,
        rank: State::new(9),
    };
    let selected = State::new(first.clone());
    let rank = selected.binding().follow(|item| item.rank.clone());
    let mut observer = Observation::new(|| {});
    assert_eq!(observer.track(|| rank.get()), Ok(2));
    assert_eq!(observer.dependency_count(), 2);
    selected.set(second.clone());
    assert_eq!(rank.set(10), Ok(true));
    assert_eq!(first.rank.get(), 2);
    assert_eq!(observer.track(|| rank.get()), Ok(10));
    first.rank.set(3);
    assert!(!observer.is_dirty());
    second.rank.set(11);
    assert!(observer.is_dirty());
}

#[test]
fn missing_optional_target_observes_its_return_and_rejects_writes() {
    let selected: State<Option<Item>> = State::new(None);
    let rank = selected
        .binding()
        .try_follow(|item| item.as_ref().map(|item| item.rank.clone()));
    let mut observer = Observation::new(|| {});
    assert!(!observer.track(|| rank.is_available()));
    assert_eq!(rank.set(10), Err(Unavailable));
    assert!(!observer.is_dirty());
    let item = Item {
        id: 1,
        rank: State::new(4),
    };
    selected.set(Some(item.clone()));
    assert!(observer.is_dirty());
    assert_eq!(observer.track(|| rank.get()), Ok(4));
    selected.set(None);
    assert_eq!(observer.track(|| rank.get()), Err(Unavailable));
    item.rank.set(5);
    assert!(!observer.is_dirty());
    let mut called = false;
    assert_eq!(rank.update(|_| called = true), Err(Unavailable));
    assert!(!called);
}

#[test]
fn keyed_selection_does_not_follow_the_old_array_position() {
    let a = Item {
        id: 1,
        rank: State::new(1),
    };
    let b = Item {
        id: 2,
        rank: State::new(2),
    };
    let items = State::new(vec![a.clone(), b.clone()]);
    let rank = items.binding().try_follow(|items| {
        items
            .iter()
            .find(|item| item.id == 1)
            .map(|item| item.rank.clone())
    });
    items.update(|items| items.reverse());
    rank.set(3).unwrap();
    assert_eq!(a.rank.get(), 3);
    assert_eq!(b.rank.get(), 2);
    items.update(|items| items.retain(|item| item.id != 1));
    assert_eq!(rank.set(4), Err(Unavailable));
    assert_eq!(b.rank.get(), 2);
}

#[test]
fn writes_and_debug_do_not_create_incidental_dependencies() {
    let value = State::new(State::new(2));
    let binding = value.binding().follow(Clone::clone);
    let mut observer = Observation::new(|| {});
    observer.track(|| {
        binding.set(3).unwrap();
        assert!(format!("{binding:?}").contains("Binding"));
    });
    assert_eq!(observer.dependency_count(), 0);
    value.set(State::new(4));
    assert!(!observer.is_dirty());
}

#[test]
fn dynamic_target_is_not_resolved_until_used() {
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let value = State::new(1);
    let mut observer = Observation::new(|| {});
    let binding = observer.track(|| {
        Binding::resolve(move || {
            count.set(count.get() + 1);
            Some(value.clone())
        })
    });
    assert_eq!(calls.get(), 0);
    assert_eq!(observer.dependency_count(), 0);
    assert_eq!(binding.get(), Ok(1));
    assert_eq!(calls.get(), 1);
    binding.set(2).unwrap();
    assert_eq!(calls.get(), 2);
}

#[test]
fn wake_can_read_the_binding_after_mutation_borrows_are_released() {
    let value = State::new(vec![1]);
    let binding = value.binding().map(|v| &v[0], |v| &mut v[0]);
    let observed = Rc::new(Cell::new(0));
    let output = observed.clone();
    let read = binding.clone();
    let mut observer = Observation::new(move || output.set(read.get().unwrap()));
    observer.track(|| binding.get().unwrap());
    binding.set(2).unwrap();
    assert_eq!(observed.get(), 2);
}

#[test]
fn replaced_value_is_dropped_after_the_source_borrow_is_released() {
    struct Value {
        number: u32,
        on_drop: Option<Box<dyn Fn()>>,
    }

    impl PartialEq for Value {
        fn eq(&self, other: &Self) -> bool {
            self.number == other.number
        }
    }

    impl Drop for Value {
        fn drop(&mut self) {
            if let Some(callback) = &self.on_drop {
                callback();
            }
        }
    }

    let value = State::new(Value {
        number: 0,
        on_drop: None,
    });
    let read = value.clone();
    let seen = Rc::new(Cell::new(0));
    let output = seen.clone();
    value.replace(Value {
        number: 1,
        on_drop: Some(Box::new(move || output.set(read.read().number))),
    });
    value
        .binding()
        .set(Value {
            number: 2,
            on_drop: None,
        })
        .unwrap();
    assert_eq!(seen.get(), 2);
}

#[test]
fn partially_completed_update_invalidates_on_unwind() {
    let value = State::new(vec![1]);
    let binding = value.binding().map(|v| &v[0], |v| &mut v[0]);
    let mut observer = Observation::new(|| {});
    observer.track(|| binding.get().unwrap());
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            binding
                .update(|value| {
                    *value = 2;
                    panic!("interrupted mutation");
                })
                .unwrap();
        }))
        .is_err()
    );
    assert!(observer.is_dirty());
    assert_eq!(observer.track(|| binding.get()), Ok(2));
}

#[test]
fn cloned_binding_keeps_target_alive_without_cloning_its_value() {
    struct Payload(Rc<()>);
    let alive = Rc::new(());
    let binding = State::new(Payload(alive.clone())).binding();
    let copy = binding.clone();
    drop(binding);
    assert_eq!(copy.with(|p| Rc::strong_count(&p.0)), Ok(2));
    drop(copy);
    assert_eq!(Rc::strong_count(&alive), 1);
}
