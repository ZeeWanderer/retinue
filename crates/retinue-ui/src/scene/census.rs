//! Owner-thread work counts. The optional feature is independent of frame timing.
use retinue_render::Key;
use rustc_hash::FxHashSet;
use std::cell::RefCell;

pub mod solver;

#[derive(Default)]
pub struct Counts {
    pub world_resolves: usize,
    pub restored_slots: usize,
    pub paint_placements: usize,
    pub recordings: usize,
    pub bounds_calls: usize,
    pub bounds_keys: FxHashSet<Key>,
    pub slice_requests: usize,
}

thread_local! {
    static ACTIVE: RefCell<Option<Counts>> = const { RefCell::new(None) };
}

pub(super) fn count(f: impl FnOnce(&mut Counts)) {
    ACTIVE.with_borrow_mut(|c| {
        if let Some(c) = c {
            f(c);
        }
    });
}

/// Measure just the enclosed work, excluding fixture setup and inspection.
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with_borrow_mut(Option::take);
        }
    }
    ACTIVE.with_borrow_mut(|slot| {
        assert!(slot.is_none(), "nested geometry census");
        *slot = Some(Counts::default());
    });
    let _reset = Reset;
    (f(), ACTIVE.with_borrow_mut(Option::take).unwrap())
}
