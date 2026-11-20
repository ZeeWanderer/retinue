use crate::{Key, scene::Scene};
use rustc_hash::FxHashMap as Map;

#[derive(Default)]
pub(super) struct Names {
    pending: Map<Key, Option<Key>>,
    published: Map<Key, Key>,
    lookup: Map<Key, Vec<Key>>,
}

impl Names {
    pub fn accept(&mut self, changes: impl Iterator<Item = (Key, Option<Key>)>) {
        self.pending.extend(changes);
    }

    pub fn publish(&mut self, scene: &Scene, topology_changed: bool) {
        if self.pending.is_empty() && !topology_changed {
            return;
        }
        #[cfg(feature = "ir-profiling")]
        let _detail = crate::profiling::Scope::ir(crate::profiling::IrPart::Names);
        for (key, name) in self.pending.drain() {
            let old = self.published.get(&key).copied();
            if old == name {
                continue;
            }
            if let Some(old) = old {
                Self::unlink(&mut self.lookup, old, key);
            }
            match name {
                Some(name) => {
                    self.published.insert(key, name);
                    self.lookup.entry(name).or_default().push(key);
                }
                None => {
                    self.published.remove(&key);
                }
            }
        }
        if topology_changed {
            self.published.retain(|key, name| {
                if scene.contains(*key) {
                    return true;
                }
                Self::unlink(&mut self.lookup, *name, *key);
                false
            });
        }
    }

    fn unlink(lookup: &mut Map<Key, Vec<Key>>, name: Key, key: Key) {
        let bucket = lookup.get_mut(&name).unwrap();
        let i = bucket
            .iter()
            .position(|&candidate| candidate == key)
            .unwrap();
        bucket.swap_remove(i);
        if bucket.is_empty() {
            lookup.remove(&name);
        }
    }

    pub fn keys(&self, name: Key) -> &[Key] {
        self.lookup.get(&name).map_or(&[], Vec::as_slice)
    }

    pub fn name(&self, key: Key) -> Option<Key> {
        self.published.get(&key).copied()
    }
}
