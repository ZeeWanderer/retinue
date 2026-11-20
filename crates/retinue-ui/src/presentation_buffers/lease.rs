use super::*;

/// Owns existing mappings, not copies. Drop returns both slots after native work has finished.
pub struct Lease {
    returned: Arc<Mutex<Vec<(usize, Slot)>>>,
    target: Option<(usize, Slot)>,
    source: Option<(usize, Slot)>,
    age: u8,
}

impl Lease {
    pub fn index(&self) -> usize {
        self.target.as_ref().unwrap().0
    }

    pub fn pixels(&mut self) -> Option<Pixels<'_>> {
        let target = &mut self.target.as_mut()?.1;
        let source = self.source.as_mut().map(|(index, slot)| {
            let length = slot.buffer.height() as usize * slot.buffer.stride() as usize;
            (
                *index,
                &slot.pool.raw_data_mut(&slot.buffer.slot())[..length],
            )
        });
        Some(Pixels {
            age: self.age,
            target: target.buffer.canvas(&mut target.pool)?,
            source,
        })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut returned = self.returned.lock().unwrap();
        returned.extend(self.target.take());
        returned.extend(self.source.take());
    }
}

impl Buffers {
    pub(super) fn collect(&mut self) {
        for (index, slot) in self.returned.lock().unwrap().drain(..) {
            assert!(self.slots[index].is_none());
            self.slots[index] = Some(slot);
        }
    }

    pub fn detach(&mut self, index: usize) -> Option<Lease> {
        self.collect();
        let slot = self.slots.get(index)?.as_ref()?;
        if slot.buffer.slot().has_active_buffers() {
            return None;
        }
        let age = slot
            .serial
            .and_then(|last| self.serial.checked_sub(last)?.checked_add(1))
            .and_then(|age| u8::try_from(age).ok())
            .unwrap_or(0);
        let target = Some((index, self.slots[index].take()?));
        let source = self
            .latest
            .filter(|from| *from != index)
            .and_then(|from| self.slots[from].take().map(|slot| (from, slot)));
        Some(Lease {
            returned: self.returned.clone(),
            target,
            source,
            age,
        })
    }

    /// None means the surface was resized/reset while this lease was in flight.
    pub fn restore(&mut self, lease: Lease) -> Option<usize> {
        let index = Arc::ptr_eq(&lease.returned, &self.returned).then(|| lease.index());
        drop(lease);
        self.collect();
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_can_move_to_a_worker_without_a_pointer_escape() {
        fn send<T: Send>() {}
        send::<Lease>();
    }
}
