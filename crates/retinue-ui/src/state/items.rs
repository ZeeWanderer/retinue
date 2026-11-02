use super::{DuplicateKey, KeyedOrder, State, state::WeakState};
use crate::Key;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

struct Storage<T> {
    order: State<KeyedOrder>,
    entries: RefCell<Entries<T>>,
}
struct Entries<T> {
    current: HashMap<Key, State<T>>,
    retired: HashMap<Key, WeakState<T>>,
}

/// Ordered identity and individual row contents have separate observation sources.
pub struct Items<T>(Rc<Storage<T>>);
impl<T> Clone for Items<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> PartialEq for Items<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl<T> Eq for Items<T> {}
impl<T: 'static> Default for Items<T> {
    fn default() -> Self {
        Self(Rc::new(Storage {
            order: State::default(),
            entries: RefCell::new(Entries {
                current: HashMap::new(),
                retired: HashMap::new(),
            }),
        }))
    }
}

impl<T: 'static> Items<T> {
    pub fn order(&self) -> KeyedOrder {
        self.0.order.get()
    }

    pub fn item(&self, key: Key) -> Option<State<T>> {
        self.0.order.read();
        self.0.entries.borrow().current.get(&key).cloned()
    }

    pub fn get(&self, key: Key) -> Option<T>
    where
        T: Clone,
    {
        self.item(key).map(|value| value.get())
    }

    pub fn set(&self, key: Key, value: T) -> Option<bool>
    where
        T: PartialEq,
    {
        self.0
            .entries
            .borrow()
            .current
            .get(&key)
            .map(|row| row.set(value))
    }

    pub fn replace(&self, rows: impl IntoIterator<Item = (Key, T)>) -> Result<bool, DuplicateKey>
    where
        T: PartialEq,
    {
        let rows: Vec<_> = rows.into_iter().collect();
        let order = KeyedOrder::new(rows.iter().map(|(key, _)| *key))?;
        let mut entries = self.0.entries.borrow_mut();
        entries.retired.retain(|_, row| row.upgrade().is_some());
        let mut old = std::mem::take(&mut entries.current);
        let mut changed = false;
        for (key, value) in rows {
            let row = old
                .remove(&key)
                .or_else(|| entries.retired.remove(&key)?.upgrade());
            let row = match row {
                Some(row) => {
                    changed |= row.set(value);
                    row
                }
                None => {
                    changed = true;
                    State::new(value)
                }
            };
            entries.current.insert(key, row);
        }
        changed |= !old.is_empty();
        for (key, row) in old {
            entries.retired.insert(key, row.downgrade());
        }
        drop(entries);
        Ok(self.0.order.set(order) | changed)
    }
}
