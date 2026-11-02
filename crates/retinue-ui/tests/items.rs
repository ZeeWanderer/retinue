use retinue_ui::{Key, prelude::*, state::Observation};
use std::{cell::Cell, rc::Rc};

fn key(value: u64) -> Key {
    Key::new("item", value)
}
fn observer() -> (Observation, Rc<Cell<usize>>) {
    let wakes = Rc::new(Cell::new(0));
    let signal = wakes.clone();
    (
        Observation::new(move || signal.set(signal.get() + 1)),
        wakes,
    )
}

#[test]
fn row_updates_do_not_invalidate_order_or_other_rows() {
    let items = Items::default();
    items.replace([(key(1), 10), (key(2), 20)]).unwrap();
    let (mut structure, structure_wakes) = observer();
    let row = structure.track(|| {
        assert_eq!(items.order().len(), 2);
        items.item(key(1)).unwrap()
    });
    let (mut first, first_wakes) = observer();
    let (mut second, second_wakes) = observer();
    first.track(|| row.get());
    second.track(|| items.get(key(2)));
    items.replace([(key(1), 11), (key(2), 20)]).unwrap();
    assert_eq!(structure_wakes.get(), 0);
    assert_eq!(first_wakes.get(), 1);
    assert_eq!(second_wakes.get(), 0);
    assert_eq!(items.item(key(1)), Some(row));
}

#[test]
fn reorder_and_remove_notify_presence_without_resetting_row_identity() {
    let items = Items::default();
    items.replace([(key(1), 10), (key(2), 20)]).unwrap();
    let row = items.item(key(1)).unwrap();
    let (mut presence, wakes) = observer();
    presence.track(|| items.item(key(1)));
    items.replace([(key(2), 20), (key(1), 10)]).unwrap();
    assert_eq!(wakes.get(), 1);
    assert_eq!(items.item(key(1)), Some(row.clone()));
    presence.track(|| items.item(key(1)));
    items.replace([(key(2), 20)]).unwrap();
    assert_eq!(wakes.get(), 2);
    assert!(items.item(key(1)).is_none());
    items.replace([(key(1), 30)]).unwrap();
    assert_eq!(items.item(key(1)), Some(row.clone()));
    assert_eq!(row.get(), 30);
}

#[test]
fn rejected_duplicate_snapshot_changes_nothing() {
    let items = Items::default();
    items.replace([(key(1), 10)]).unwrap();
    let row = items.item(key(1)).unwrap();
    assert!(items.replace([(key(1), 40), (key(1), 50)]).is_err());
    assert_eq!(items.order().keys(), [key(1)]);
    assert_eq!(row.get(), 10);
    assert!(!items.replace([(key(1), 10)]).unwrap());
}

#[test]
fn removed_data_is_not_retained_by_the_collection() {
    let item = Rc::new(String::from("data"));
    let weak = Rc::downgrade(&item);
    let items = Items::default();
    items.replace([(key(1), item)]).unwrap();
    items.replace([]).unwrap();
    assert!(weak.upgrade().is_none());
}
