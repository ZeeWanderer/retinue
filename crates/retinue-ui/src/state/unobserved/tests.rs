use super::*;
use crate::state::{Observation, State};
use std::cell::Cell;

#[test]
fn ignored_storage_shares_identity_without_suppressing_nested_reads() {
    let value = Unobserved::new(State::new(1));
    let copy = value.clone();
    assert_eq!(value, copy);
    let mut observer = Observation::new(|| {});
    observer.track(|| value.read().get());
    copy.read().set(2);
    assert!(observer.is_dirty());
    observer.track(|| value.read().get());
    let old = copy.replace(State::new(3));
    assert!(!observer.is_dirty());
    assert_eq!(old.get(), 2);
    assert_eq!(value.read().get(), 3);
}

struct ReentrantDrop {
    value: u32,
    drop: Rc<dyn Fn()>,
}
impl PartialEq for ReentrantDrop {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Drop for ReentrantDrop {
    fn drop(&mut self) {
        (self.drop)();
    }
}

#[test]
fn replaced_payload_drops_after_the_mutable_borrow_is_released() {
    let slot: Rc<RefCell<Option<Unobserved<ReentrantDrop>>>> = Rc::default();
    let observed = Rc::new(Cell::new(0));
    let value = Unobserved::new(ReentrantDrop {
        value: 1,
        drop: Rc::new({
            let slot = Rc::downgrade(&slot);
            let observed = observed.clone();
            move || {
                observed.set(
                    slot.upgrade()
                        .unwrap()
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .read()
                        .value,
                );
            }
        }),
    });
    *slot.borrow_mut() = Some(value.clone());
    assert!(value.set(ReentrantDrop {
        value: 2,
        drop: Rc::new(|| {})
    }));
    assert_eq!(observed.get(), 2);
}
