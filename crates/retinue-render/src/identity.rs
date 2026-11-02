#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub namespace: &'static str,
    pub value: u64,
}

impl Key {
    pub const fn new(namespace: &'static str, value: u64) -> Self {
        Self { namespace, value }
    }
}

impl From<u64> for Key {
    fn from(value: u64) -> Self {
        Self::new("local.element", value)
    }
}

impl From<&'static str> for Key {
    fn from(value: &'static str) -> Self {
        Self::new(value, 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaintKey {
    pub owner: Key,
    pub revision: u64,
}

impl PaintKey {
    pub const fn new(owner: Key, revision: u64) -> Self {
        Self { owner, revision }
    }
}

impl From<u64> for PaintKey {
    fn from(value: u64) -> Self {
        Self::new(Key::new("local.paint", value), 0)
    }
}
