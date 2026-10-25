use lru::LruCache;
use std::{
    hash::Hash,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub entries: usize,
    pub budget_bytes: usize,
    pub cached_bytes: usize,
    pub live_bytes: usize,
    pub retained_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

pub trait ResourceSize {
    /// Heap storage owned exclusively by this value, excluding its inline size.
    fn heap_bytes(&self) -> usize;
}

struct Allocation<V> {
    value: V,
    bytes: usize,
    live_bytes: Arc<AtomicUsize>,
}

impl<V> Drop for Allocation<V> {
    fn drop(&mut self) {
        self.live_bytes.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

pub struct Resource<V>(Arc<Allocation<V>>);

impl<V> Clone for Resource<V> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<V> Deref for Resource<V> {
    type Target = V;

    fn deref(&self) -> &V {
        &self.0.value
    }
}

/// Session-owned RAM cache. Handles remain valid, and accounted for, after eviction.
pub struct ResourceCache<K, V> {
    entries: LruCache<K, Resource<V>>,
    budget: usize,
    cached_bytes: usize,
    live_bytes: Arc<AtomicUsize>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl<K: Copy + Hash + Eq, V: ResourceSize> ResourceCache<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: LruCache::unbounded(),
            budget,
            cached_bytes: 0,
            live_bytes: Arc::default(),
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<Resource<V>> {
        if let Some(value) = self.entries.get(key) {
            self.hits += 1;
            Some(value.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn get_or_insert_with(&mut self, key: K, prepare: impl FnOnce() -> V) -> Resource<V> {
        self.get(&key)
            .unwrap_or_else(|| self.insert(key, prepare()))
    }

    pub fn try_get_or_insert_with<E>(
        &mut self,
        key: K,
        prepare: impl FnOnce() -> Result<V, E>,
    ) -> Result<Resource<V>, E> {
        match self.get(&key) {
            Some(value) => Ok(value),
            None => Ok(self.insert(key, prepare()?)),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Resource<V> {
        let bytes = value.heap_bytes()
            + size_of::<Allocation<V>>()
            + size_of::<K>()
            + 2 * size_of::<usize>();
        let resource = Resource(Arc::new(Allocation {
            value,
            bytes,
            live_bytes: Arc::clone(&self.live_bytes),
        }));
        self.live_bytes.fetch_add(bytes, Ordering::Relaxed);
        if let Some(previous) = self.entries.pop(&key) {
            self.cached_bytes -= previous.0.bytes;
        }
        // A one-off oversized resource must not evict the entire working set.
        if bytes <= self.budget {
            while self.cached_bytes > self.budget - bytes {
                let (_, old) = self.entries.pop_lru().expect("cache accounting");
                self.cached_bytes -= old.0.bytes;
                self.evictions += 1;
            }
            self.cached_bytes += bytes;
            self.entries.put(key, resource.clone());
        }
        resource
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.cached_bytes = 0;
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.entries.retain(|key, value| {
            if keep(key) {
                true
            } else {
                self.cached_bytes -= value.0.bytes;
                false
            }
        });
    }

    pub fn stats(&self) -> CacheStats {
        let evictable = self
            .entries
            .iter()
            .filter(|(_, value)| Arc::strong_count(&value.0) == 1)
            .map(|(_, value)| value.0.bytes)
            .sum::<usize>();
        let live_bytes = self.live_bytes.load(Ordering::Relaxed);
        CacheStats {
            entries: self.entries.len(),
            budget_bytes: self.budget,
            cached_bytes: self.cached_bytes,
            live_bytes,
            retained_bytes: live_bytes.saturating_sub(evictable),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ResourceSize for Vec<u8> {
        fn heap_bytes(&self) -> usize {
            self.capacity()
        }
    }

    fn weight(payload: usize) -> usize {
        payload + size_of::<Allocation<Vec<u8>>>() + size_of::<u8>() + 2 * size_of::<usize>()
    }

    #[test]
    fn evicts_by_bytes_and_recent_use_not_entry_count() {
        let mut cache = ResourceCache::new(weight(16) + weight(32));
        drop(cache.insert(1_u8, vec![1; 16]));
        drop(cache.insert(2, vec![2; 32]));
        drop(cache.get(&1).unwrap());
        drop(cache.insert(3, vec![3; 32]));
        assert!(cache.get(&2).is_none());
        assert!(cache.get(&1).is_some());
        assert!(cache.get(&3).is_some());
        assert_eq!(cache.stats().cached_bytes, cache.stats().budget_bytes);
    }

    #[test]
    fn evicted_and_replaced_handles_remain_live_until_dropped() {
        let mut cache = ResourceCache::new(weight(16));
        let old = cache.insert(1_u8, vec![1; 16]);
        let new = cache.insert(1, vec![2; 16]);
        assert_eq!(old[0], 1);
        assert_eq!(new[0], 2);
        assert_eq!(cache.stats().live_bytes, 2 * weight(16));
        assert_eq!(cache.stats().retained_bytes, 2 * weight(16));
        drop(old);
        drop(new);
        assert_eq!(cache.stats().retained_bytes, 0);
        let held = cache.get(&1).unwrap();
        cache.clear();
        assert_eq!(cache.stats().cached_bytes, 0);
        assert_eq!(cache.stats().live_bytes, weight(16));
        drop(held);
        assert_eq!(cache.stats().live_bytes, 0);
    }

    #[test]
    fn oversized_and_zero_budget_resources_do_not_flush_working_set() {
        for budget in [0, weight(8)] {
            let mut cache = ResourceCache::new(budget);
            drop(cache.insert(1_u8, vec![1; 8]));
            let large = cache.insert(2, vec![2; 512]);
            assert_eq!(cache.stats().cached_bytes, budget);
            assert!(cache.get(&2).is_none());
            drop(large);
            assert_eq!(cache.stats().live_bytes, budget);
        }
    }

    #[test]
    fn hits_do_not_prepare_again_and_empty_values_are_bounded() {
        let mut cache = ResourceCache::new(weight(0) * 2);
        drop(cache.get_or_insert_with(1_u8, Vec::new));
        drop(cache.get_or_insert_with(1, || panic!("prepared twice")));
        assert_eq!((cache.stats().hits, cache.stats().misses), (1, 1));
        for key in 2..200 {
            drop(cache.insert(key, Vec::new()));
        }
        assert_eq!(cache.stats().entries, 2);
        assert_eq!(cache.stats().live_bytes, weight(0) * 2);
    }

    #[test]
    fn failed_preparation_is_not_cached_and_does_not_evict() {
        let mut cache = ResourceCache::new(weight(8));
        drop(cache.insert(1_u8, vec![1; 8]));
        let failed = cache.try_get_or_insert_with(2, || Err("failed"));
        assert!(matches!(failed, Err("failed")));
        assert_eq!(cache.stats().entries, 1);
        assert!(cache.get(&1).is_some());
        assert_eq!(
            cache
                .try_get_or_insert_with(2, || Ok::<_, &str>(vec![2; 8]))
                .unwrap()[0],
            2,
        );
    }

    #[test]
    fn handles_can_outlive_the_cache_without_losing_accounting() {
        let mut cache = ResourceCache::new(1024);
        let handle = cache.insert(1_u8, vec![7; 16]);
        let counter = Arc::clone(&cache.live_bytes);
        drop(cache);
        assert_eq!(handle[0], 7);
        assert_eq!(counter.load(Ordering::Relaxed), weight(16));
        drop(handle);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pruning_releases_cache_ownership_without_invalidating_handles() {
        let mut cache = ResourceCache::new(1024);
        let held = cache.insert(1_u8, vec![1; 16]);
        drop(cache.insert(2, vec![2; 32]));
        cache.retain(|key| *key == 2);
        assert_eq!(cache.stats().cached_bytes, weight(32));
        assert_eq!(cache.stats().live_bytes, weight(16) + weight(32));
        assert_eq!(cache.stats().retained_bytes, weight(16));
        assert_eq!(held[0], 1);
        drop(held);
        assert_eq!(cache.stats().live_bytes, weight(32));
        assert!(cache.get(&1).is_none());
        assert!(cache.get(&2).is_some());
    }
}
