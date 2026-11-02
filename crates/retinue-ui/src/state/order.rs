use crate::Key;
use std::{collections::HashMap, fmt, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateKey(pub Key);
impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate item key: {:?}", self.0)
    }
}
impl std::error::Error for DuplicateKey {}

#[derive(Clone, Debug, Default)]
pub struct KeyedOrder(Rc<Order>);
#[derive(Debug, Default)]
struct Order {
    keys: Vec<Key>,
    indices: HashMap<Key, usize>,
}

impl PartialEq for KeyedOrder {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.keys == other.0.keys
    }
}
impl Eq for KeyedOrder {}

impl KeyedOrder {
    pub fn new(keys: impl IntoIterator<Item = Key>) -> Result<Self, DuplicateKey> {
        let keys: Vec<_> = keys.into_iter().collect();
        let mut indices = HashMap::with_capacity(keys.len());
        for (index, &key) in keys.iter().enumerate() {
            if indices.insert(key, index).is_some() {
                return Err(DuplicateKey(key));
            }
        }
        Ok(Self(Rc::new(Order { keys, indices })))
    }
    pub fn keys(&self) -> &[Key] {
        &self.0.keys
    }
    pub fn len(&self) -> usize {
        self.0.keys.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.keys.is_empty()
    }
    pub fn index(&self, key: Key) -> Option<usize> {
        self.0.indices.get(&key).copied()
    }
}
