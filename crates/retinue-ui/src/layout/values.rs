use std::{
    any::{Any, TypeId, type_name},
    fmt,
    rc::Rc,
};

pub trait LayoutValueKey: 'static {
    type Value: Clone + PartialEq + fmt::Debug + 'static;
    fn default_value() -> Self::Value;
}

trait Value: fmt::Debug {
    fn as_any(&self) -> &dyn Any;
    fn same(&self, other: &dyn Value) -> bool;
}

impl<T: PartialEq + fmt::Debug + 'static> Value for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn Value) -> bool {
        other.as_any().downcast_ref() == Some(self)
    }
}

#[derive(Clone)]
struct Entry {
    kind: TypeId,
    name: &'static str,
    value: Rc<dyn Value>,
}

/// Values consumed by the immediate arranging parent, without inheritance.
#[derive(Clone, Default)]
pub struct LayoutValues(Option<Rc<Vec<Entry>>>);

impl LayoutValues {
    pub fn get<K: LayoutValueKey>(&self) -> K::Value {
        self.explicit::<K>()
            .cloned()
            .unwrap_or_else(K::default_value)
    }
    pub fn explicit<K: LayoutValueKey>(&self) -> Option<&K::Value> {
        self.by_type(TypeId::of::<K>())?.downcast_ref()
    }
    pub(super) fn by_type(&self, kind: TypeId) -> Option<&dyn Any> {
        let entries = self.0.as_ref()?;
        let index = entries
            .binary_search_by_key(&kind, |entry| entry.kind)
            .ok()?;
        Some(entries[index].value.as_ref().as_any())
    }
    pub fn with<K: LayoutValueKey>(mut self, value: K::Value) -> Self {
        if self.explicit::<K>() == Some(&value) {
            return self;
        }
        let entries = Rc::make_mut(self.0.get_or_insert_with(|| Rc::new(Vec::new())));
        let entry = Entry {
            kind: TypeId::of::<K>(),
            name: type_name::<K>(),
            value: Rc::new(value),
        };
        match entries.binary_search_by_key(&entry.kind, |entry| entry.kind) {
            Ok(index) => entries[index] = entry,
            Err(index) => entries.insert(index, entry),
        }
        self
    }
    pub fn without<K: LayoutValueKey>(mut self) -> Self {
        if let Some(entries) = &mut self.0 {
            if let Ok(index) = entries.binary_search_by_key(&TypeId::of::<K>(), |entry| entry.kind)
            {
                Rc::make_mut(entries).remove(index);
            }
            if entries.is_empty() {
                self.0 = None;
            }
        }
        self
    }

    pub fn overlaid(mut self, overrides: &Self) -> Self {
        let Some(overrides) = &overrides.0 else {
            return self;
        };
        if self.0.is_none() {
            return Self(Some(overrides.clone()));
        }
        let entries = Rc::make_mut(self.0.as_mut().unwrap());
        for entry in overrides.iter() {
            match entries.binary_search_by_key(&entry.kind, |entry| entry.kind) {
                Ok(index) => entries[index] = entry.clone(),
                Err(index) => entries.insert(index, entry.clone()),
            }
        }
        self
    }
}

impl PartialEq for LayoutValues {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                Rc::ptr_eq(a, b)
                    || a.len() == b.len()
                        && a.iter().zip(b.iter()).all(|(a, b)| {
                            a.kind == b.kind && a.value.as_ref().same(b.value.as_ref())
                        })
            }
            _ => false,
        }
    }
}

impl fmt::Debug for LayoutValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(
                self.0
                    .iter()
                    .flat_map(|entries| entries.iter())
                    .map(|entry| (entry.name, &entry.value)),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Weight;
    struct Priority;
    impl LayoutValueKey for Weight {
        type Value = u32;
        fn default_value() -> u32 {
            1
        }
    }
    impl LayoutValueKey for Priority {
        type Value = u32;
        fn default_value() -> u32 {
            0
        }
    }

    #[test]
    fn keys_are_distinct_and_copies_only_diverge_on_changes() {
        let empty = LayoutValues::default();
        assert!(empty.0.is_none());
        assert_eq!(empty.get::<Weight>(), 1);
        assert_eq!(empty.explicit::<Weight>(), None);
        let a = empty.clone().with::<Weight>(4).with::<Priority>(2);
        let b = LayoutValues::default()
            .with::<Priority>(2)
            .with::<Weight>(4);
        assert_eq!(a, b);
        let same = a.clone().with::<Weight>(4);
        assert!(Rc::ptr_eq(a.0.as_ref().unwrap(), same.0.as_ref().unwrap()));
        let changed = a.clone().with::<Weight>(9);
        assert_eq!(a.get::<Weight>(), 4);
        assert_eq!(changed.get::<Weight>(), 9);
        assert_eq!(changed.get::<Priority>(), 2);
        assert_ne!(a, changed);
        assert_eq!(changed.without::<Weight>().without::<Priority>(), empty);
        assert_ne!(empty.clone().with::<Weight>(1), empty);
    }
}
