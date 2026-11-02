use std::{
    any::{Any, TypeId, type_name},
    fmt,
    sync::Arc,
};

trait Annotation: fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn equivalent(&self, other: &dyn Annotation) -> bool;
}

impl<T: fmt::Debug + Eq + Send + Sync + 'static> Annotation for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn equivalent(&self, other: &dyn Annotation) -> bool {
        other.as_any().downcast_ref() == Some(self)
    }
}

#[derive(Clone)]
struct Entry {
    kind: TypeId,
    name: &'static str,
    value: Arc<dyn Annotation>,
}

/// Advisory, scope-local metadata. Unknown annotations have no effect on paint.
#[derive(Clone, Default)]
pub struct OptimizationContext(Option<Arc<Vec<Entry>>>);

impl OptimizationContext {
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let entries = self.0.as_ref()?;
        let index = entries
            .binary_search_by_key(&TypeId::of::<T>(), |e| e.kind)
            .ok()?;
        entries[index].value.as_ref().as_any().downcast_ref()
    }

    pub fn with<T: fmt::Debug + Eq + Send + Sync + 'static>(mut self, value: T) -> Self {
        let entries = Arc::make_mut(self.0.get_or_insert_with(|| Arc::new(Vec::new())));
        let entry = Entry {
            kind: TypeId::of::<T>(),
            name: type_name::<T>(),
            value: Arc::new(value),
        };
        match entries.binary_search_by_key(&entry.kind, |e| e.kind) {
            Ok(index) => entries[index] = entry,
            Err(index) => entries.insert(index, entry),
        }
        self
    }

    pub fn without<T: 'static>(mut self) -> Self {
        if let Some(entries) = &mut self.0 {
            if let Ok(index) = entries.binary_search_by_key(&TypeId::of::<T>(), |e| e.kind) {
                Arc::make_mut(entries).remove(index);
            }
            if entries.is_empty() {
                self.0 = None;
            }
        }
        self
    }
}

impl PartialEq for OptimizationContext {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                Arc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter().zip(b.iter()).all(|(a, b)| {
                            a.kind == b.kind && a.value.as_ref().equivalent(b.value.as_ref())
                        }))
            }
            _ => false,
        }
    }
}

impl Eq for OptimizationContext {}

impl fmt::Debug for OptimizationContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(
                self.0
                    .iter()
                    .flat_map(|v| v.iter())
                    .map(|e| (e.name, &e.value)),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Cadence(u32);
    #[derive(Debug, PartialEq, Eq)]
    struct Repeated(bool);

    #[test]
    fn annotations_extend_without_registry_and_compare_independent_of_order() {
        let a = OptimizationContext::default()
            .with(Cadence(3))
            .with(Repeated(true));
        let b = OptimizationContext::default()
            .with(Repeated(true))
            .with(Cadence(3));
        assert_eq!(a, b);
        assert_eq!(a.get::<Cadence>(), Some(&Cadence(3)));
        assert!(a.get::<u64>().is_none());
        let c = a.clone().with(Cadence(5));
        assert_ne!(a, c);
        assert_eq!(a.get::<Cadence>(), Some(&Cadence(3)));
        assert_eq!(c.get::<Cadence>(), Some(&Cadence(5)));
        assert_eq!(
            c.without::<Cadence>().without::<Repeated>(),
            OptimizationContext::default()
        );
        fn snapshot_safe<T: Send + Sync>() {}
        snapshot_safe::<OptimizationContext>();
    }
}
