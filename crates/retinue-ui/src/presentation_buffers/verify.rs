//! Full-render oracle for mapped benchmark runs, never enabled in shipping builds.

use super::*;

#[derive(Default)]
pub(super) struct Oracle {
    pub pending: Option<Vec<u8>>,
    last: Option<Vec<u8>>,
}

impl Oracle {
    pub fn check(
        &self,
        target: &[u8],
        size: (u32, u32),
        format: wl_shm::Format,
        damage: Option<&Region>,
    ) -> Result<(), String> {
        let Some(reference) = &self.pending else {
            return Ok(());
        };
        let mask = match format {
            wl_shm::Format::Xrgb8888 => 0x00ff_ffff,
            wl_shm::Format::Argb8888 => u32::MAX,
            _ => return Err(format!("unsupported oracle format: {format:?}")),
        };
        let value = |pixel: &[u8]| u32::from_ne_bytes(pixel.try_into().unwrap()) & mask;
        let compare = |actual: &[u8], stage| {
            if actual.len() != reference.len()
                || actual.len() != size.0 as usize * size.1 as usize * 4
            {
                return Err(format!("{stage}: size mismatch"));
            }
            if let Some(i) = actual
                .chunks_exact(4)
                .zip(reference.chunks_exact(4))
                .position(|(a, b)| value(a) != value(b))
            {
                return Err(format!(
                    "{stage}: pixel mismatch at {},{}: {:?} vs {:?}",
                    i % size.0 as usize,
                    i / size.0 as usize,
                    &actual[i * 4..i * 4 + 4],
                    &reference[i * 4..i * 4 + 4]
                ));
            }
            Ok(())
        };
        compare(target, "SHM target")?;
        if let (Some(previous), Some(damage)) = (&self.last, damage) {
            let mut display = previous.clone();
            for r in damage.rects() {
                let r = r.intersection(retinue_render::Rect {
                    width: size.0,
                    height: size.1,
                    ..Default::default()
                });
                for y in r.y..r.y + r.height {
                    let start = (y * size.0 + r.x) as usize * 4;
                    let end = start + r.width as usize * 4;
                    display[start..end].copy_from_slice(&target[start..end]);
                }
            }
            compare(&display, "committed surface damage")?;
        }
        Ok(())
    }

    pub fn committed(&mut self) {
        self.last = self.pending.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_is_relative_to_last_commit_and_only_ignores_undefined_channels() {
        let mut oracle = Oracle {
            pending: Some(vec![0; 8]),
            last: None,
        };
        oracle.committed();
        let current = [0xff00_0000_u32, 0xff09_0807]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect::<Vec<_>>();
        oracle.pending = Some(current.clone());
        let changed = Region::from(retinue_render::Rect {
            x: 1,
            y: 0,
            width: 1,
            height: 1,
        });
        assert!(
            oracle
                .check(&current, (2, 1), wl_shm::Format::Xrgb8888, Some(&changed))
                .is_ok()
        );
        assert!(
            oracle
                .check(&current, (2, 1), wl_shm::Format::Argb8888, Some(&changed))
                .is_err()
        );
        assert!(
            oracle
                .check(
                    &current,
                    (2, 1),
                    wl_shm::Format::Xrgb8888,
                    Some(&Region::default())
                )
                .is_err()
        );
        assert!(
            oracle
                .check(&[0; 8], (2, 1), wl_shm::Format::Xrgb8888, None)
                .is_err()
        );
    }
}
