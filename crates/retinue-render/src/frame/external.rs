use super::*;
use crate::Command;
use std::collections::BTreeMap;

impl PaintFrame {
    /// Repair demand in each live child renderer's coordinates, including empty demand.
    pub fn external_repairs(&self, repair: &Region) -> BTreeMap<Key, Region> {
        self.freeze().external_repairs(repair)
    }
}

impl FrozenFrame {
    pub fn external_repairs(&self, repair: &Region) -> BTreeMap<Key, Region> {
        let mut result = BTreeMap::new();
        for instance in self.chunks.iter() {
            if instance.program.summary().external {
                commands(
                    &instance.program,
                    instance.program.commands(),
                    instance.origin,
                    instance.clip,
                    repair,
                    &mut result,
                );
            }
        }
        result
    }
}

fn commands(
    program: &PaintProgram,
    list: &[Command],
    origin: (i32, i32),
    clip: Rect,
    repair: &Region,
    result: &mut BTreeMap<Key, Region>,
) {
    for command in list {
        match command {
            Command::Translate {
                offset,
                commands: list,
            } => {
                let offset = (
                    (offset.0 * program.scale()).round() as i32,
                    (offset.1 * program.scale()).round() as i32,
                );
                if let Some(origin) = add(origin, offset) {
                    commands(program, list, origin, clip, repair, result);
                }
            }
            Command::Clip {
                bounds,
                commands: list,
            } => {
                commands(
                    program,
                    list,
                    origin,
                    bounds.pixels(program.scale()).visible(origin, clip),
                    repair,
                    result,
                );
            }
            Command::Fragment { program, .. } if program.summary().external => {
                commands(program, program.commands(), origin, clip, repair, result);
            }
            Command::External { bounds, key } => {
                let bounds = bounds.pixels(program.scale());
                if let Some(local) = add(origin, (bounds.x, bounds.y)) {
                    let region = repair
                        .intersection(bounds.visible(origin, clip))
                        .local((local.0.into(), local.1.into()));
                    result
                        .entry(*key)
                        .and_modify(|old| *old = old.union(&region))
                        .or_insert(region);
                }
            }
            _ => {}
        }
    }
}

fn add(a: (i32, i32), b: (i32, i32)) -> Option<(i32, i32)> {
    Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Bounds, PaintKey, ProgramStore, RasterMode, Renderer, ResidencyPolicy, painter::Painter,
    };
    use std::sync::Arc;

    #[test]
    fn child_repair_matches_native_clips_transforms_and_repeated_placements() {
        let child = Key::new("child", 0);
        let inactive = Key::new("inactive", 0);
        let root = Key::new("root", 0);
        let size = (160, 130);
        let clip = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let repair = Region::new([
            Rect {
                x: 10,
                y: 0,
                width: 19,
                height: 110,
            },
            Rect {
                x: 45,
                y: 24,
                width: 60,
                height: 19,
            },
        ]);
        for scale in [0.9, 1.0, 1.2, 1.5] {
            let mut store = ProgramStore::default();
            let mut frame = PaintFrame::default();
            let commands = Arc::new(vec![
                Command::Translate {
                    offset: (7.5, -8.5),
                    commands: Arc::new(vec![Command::Clip {
                        bounds: Bounds::new(0.0, 3.0, 100.0, 70.0),
                        commands: Arc::new(vec![Command::External {
                            key: child,
                            bounds: Bounds::new(-5.0, 9.0, 80.0, 66.0),
                        }]),
                    }]),
                },
                Command::External {
                    key: child,
                    bounds: Bounds::new(30.0, 60.0, 65.0, 40.0),
                },
                Command::External {
                    key: inactive,
                    bounds: Bounds::new(120.0, 110.0, 3.0, 3.0),
                },
            ]);
            let program = store.compile(PaintKey::new(root, 0), (160.0, 130.0), scale, commands);
            frame
                .replace(
                    size,
                    scale,
                    vec![PaintInstance::new(root, program, (-3, 12), clip)],
                    Vec::new(),
                )
                .unwrap();
            let expected = frame.external_repairs(&repair);
            assert!(expected[&inactive].is_empty());
            let mut actual: BTreeMap<Key, Region> = BTreeMap::new();
            let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
            renderer.prepare_for(&frame, RasterMode::Replay, &repair);
            let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
            let mut painter =
                Painter::with_threads(&mut pixels, size.0, size.1, std::num::NonZeroU32::MIN)
                    .unwrap();
            painter.repair(&repair);
            renderer.draw_with(&frame, &mut painter, clip, |key, painter| {
                let region = painter.visible_regions();
                actual
                    .entry(key)
                    .and_modify(|old| *old = old.union(&region))
                    .or_insert(region);
            });
            painter.finish().unwrap();
            for (key, expected) in expected {
                assert_eq!(
                    actual.remove(&key).unwrap_or_default(),
                    expected,
                    "scale {scale}"
                );
            }
            assert!(actual.is_empty());
        }
    }
}
