//! Wayland presentation storage with disjoint read-only source and writable target leases.

use retinue_render::damage::Region;
use smithay_client_toolkit::shm::{
    Shm,
    slot::{Buffer, SlotPool},
};
use wayland_client::{
    Proxy,
    protocol::{wl_shm, wl_surface},
};

/// Convert logical surface dimensions and output scale to bounded physical pixels.
pub fn buffer_size(size: (u32, u32), scale: u32) -> Result<(u32, u32), &'static str> {
    let width = size.0.checked_mul(scale).ok_or("surface width overflow")?;
    let height = size.1.checked_mul(scale).ok_or("surface height overflow")?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > 40_000_000 {
        return Err("unsupported presentation size");
    }
    Ok((width, height))
}

struct Slot {
    buffer: Buffer,
    pool: SlotPool,
    serial: Option<u64>,
}

#[derive(Default)]
pub struct Buffers {
    slots: Vec<Slot>,
    latest: Option<usize>,
    serial: u64,
    geometry: Option<((u32, u32), wl_shm::Format)>,
}

pub struct Pixels<'a> {
    pub age: u8,
    pub target: &'a mut [u8],
    pub source: Option<(usize, &'a [u8])>,
}

impl Buffers {
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    /// Mapped pool capacity, not resident memory.
    pub fn allocated_bytes(&self) -> usize {
        self.slots.iter().map(|slot| slot.pool.len()).sum()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn drawable(&self) -> bool {
        self.slots.len() < 3
            || self
                .slots
                .iter()
                .any(|s| !s.buffer.slot().has_active_buffers())
    }
    pub fn buffer(&self, index: usize) -> &Buffer {
        &self.slots[index].buffer
    }
    pub fn acquire(
        &mut self,
        shm: &Shm,
        size: (u32, u32),
        format: wl_shm::Format,
        preserve_source: bool,
    ) -> Result<Option<usize>, String> {
        if self
            .geometry
            .is_some_and(|geometry| geometry != (size, format))
        {
            return Err("presentation geometry changed without resetting history".into());
        }
        let mut fallback = None;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.buffer.slot().has_active_buffers() {
                continue;
            }
            if !preserve_source || Some(index) != self.latest {
                return Ok(Some(index));
            }
            fallback = Some(index);
        }
        if self.slots.len() < 3 {
            let (width, height) = buffer_size(size, 1)?;
            let stride = width as i32 * 4;
            let len = stride as usize * height as usize;
            // SCTK rounds slots to 64 bytes; missing that padding doubles its pool.
            let mut pool =
                SlotPool::new(len.next_multiple_of(64), shm).map_err(|e| e.to_string())?;
            let (buffer, _) = pool
                .create_buffer(width as i32, height as i32, stride, format)
                .map_err(|e| e.to_string())?;
            self.slots.push(Slot {
                buffer,
                pool,
                serial: None,
            });
            self.geometry = Some((size, format));
            return Ok(Some(self.slots.len() - 1));
        }
        Ok(fallback)
    }

    pub fn pixels(&mut self, index: usize) -> Option<Pixels<'_>> {
        let age = self.slots[index]
            .serial
            .and_then(|last| self.serial.checked_sub(last)?.checked_add(1))
            .and_then(|age| u8::try_from(age).ok())
            .unwrap_or(0);
        let previous = self.latest.filter(|from| *from != index);
        let (target, source) = if let Some(from) = previous {
            let (target, source) = if index < from {
                let (left, right) = self.slots.split_at_mut(from);
                (&mut left[index], &mut right[0])
            } else {
                let (left, right) = self.slots.split_at_mut(index);
                (&mut right[0], &mut left[from])
            };
            let length = source.buffer.height() as usize * source.buffer.stride() as usize;
            // The compositor only reads this slot. Its pool cannot resize during this lease.
            let pixels = &source.pool.raw_data_mut(&source.buffer.slot())[..length];
            (target, Some((from, pixels)))
        } else {
            (&mut self.slots[index], None)
        };
        Some(Pixels {
            age,
            target: target.buffer.canvas(&mut target.pool)?,
            source,
        })
    }

    /// Queue attachment, physical damage and host state in one native commit.
    /// The host still owns flushing and commits its renderer submission on success.
    pub fn present(
        &mut self,
        index: usize,
        surface: &wl_surface::WlSurface,
        damage: &Region,
        before_commit: impl FnOnce(),
    ) -> Result<(), String> {
        self.buffer(index)
            .attach_to(surface)
            .map_err(|e| e.to_string())?;
        if surface.version() < 4 {
            surface.damage(0, 0, i32::MAX, i32::MAX);
        } else {
            for rect in damage.rects() {
                surface.damage_buffer(
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                );
            }
        }
        before_commit();
        surface.commit();
        self.submitted(index);
        Ok(())
    }

    pub fn submitted(&mut self, index: usize) {
        if self.serial == u64::MAX {
            self.serial = 0;
            for slot in &mut self.slots {
                slot.serial = None;
            }
        }
        self.serial += 1;
        self.slots[index].serial = Some(self.serial);
        self.latest = Some(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_size_checks_scale_overflow_and_allocation_bounds() {
        assert_eq!(buffer_size((1920, 1080), 2), Ok((3840, 2160)));
        assert_eq!(buffer_size((40_000_000, 1), 1), Ok((40_000_000, 1)));
        for (size, scale) in [
            ((0, 1080), 1),
            ((1920, 0), 1),
            ((1920, 1080), 0),
            ((u32::MAX, 1080), 2),
            ((1920, u32::MAX), 2),
            ((8000, 8000), 1),
            ((40_000_001, 1), 1),
        ] {
            assert!(buffer_size(size, scale).is_err());
        }
    }
}
