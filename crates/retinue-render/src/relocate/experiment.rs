//! Benchmark-only relocate executor: move completed target pixels, then repair holes.

use crate::{
    Color, Command, Key, PaintFrame, PaintInstance, RasterMode, Rect, Renderer, damage::Region,
    painter::Painter, program::Signature,
};
use rustc_hash::FxHashMap;
use std::{
    num::NonZeroU32,
    rc::{Rc, Weak},
    time::Instant,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub struct Motion {
    pub viewport: Rect,
    pub origin: (i32, i32),
}

#[derive(Default)]
pub struct Target {
    pixels: Vec<u8>,
    state: Option<State>,
}

pub struct Sample {
    pub plan_ms: f64,
    pub copy_ms: f64,
    pub prepare_ms: f64,
    pub submit_ms: f64,
    pub finish_ms: f64,
    pub total_ms: f64,
    pub copied_pixels: u64,
    pub repaired_pixels: u64,
    pub metadata_bytes: usize,
    pub reason: &'static str,
    pub repair: Region,
}

struct State {
    identity: Weak<()>,
    generation: u64,
    size: (u32, u32),
    scale: f32,
    background: Color,
    motion: Motion,
    confined: bool,
    tracked: bool,
    instances: Vec<Instance>,
}

struct Instance {
    key: Key,
    signature: Signature,
    origin: (i32, i32),
    visible: Rect,
    uniform: [Rect; 2],
}

struct Plan {
    copy: Option<(Rect, Rect)>,
    repair: Region,
    reason: &'static str,
}

impl State {
    fn capture(frame: &PaintFrame, motion: Motion, background: Color, reuse: bool) -> Self {
        let full = full_rect(frame.size());
        let motion = Motion {
            viewport: motion.viewport.intersection(full),
            ..motion
        };
        let confined = frame.instances().iter().all(|i| {
            let r = i.visible_bounds().intersection(full);
            r.is_empty() || r.intersection(motion.viewport) == r
        });
        Self {
            identity: Rc::downgrade(&frame.identity),
            generation: frame.generation(),
            size: frame.size(),
            scale: frame.scale(),
            background,
            motion,
            confined,
            tracked: reuse,
            instances: if reuse {
                frame
                    .instances()
                    .iter()
                    .map(|i| Instance {
                        key: i.key,
                        signature: i.program.signature(),
                        origin: i.origin(),
                        visible: i.visible_bounds().intersection(full),
                        uniform: uniform_regions(i, full),
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    fn compatible(&self, other: &Self) -> bool {
        self.identity.ptr_eq(&other.identity)
            && self.generation >= other.generation
            && self.size == other.size
            && self.scale == other.scale
            && self.background == other.background
            && self.confined
            && other.confined
    }

    fn plan(&self, previous: Option<&Self>, reuse: bool) -> Plan {
        let Some(old) = previous.filter(|old| self.compatible(old)) else {
            return Plan::repaint(
                Region::new([full_rect(self.size)]),
                "new-or-incompatible-target",
            );
        };
        let damage = Region::new([old.motion.viewport, self.motion.viewport]);
        if !reuse {
            return Plan::repaint(damage, "baseline");
        }
        if !old.tracked {
            return Plan::repaint(damage, "untracked-target");
        }
        let delta = (
            i64::from(self.motion.origin.0) - i64::from(old.motion.origin.0),
            i64::from(self.motion.origin.1) - i64::from(old.motion.origin.1),
        );
        let destination = shifted(old.motion.viewport, delta, self.motion.viewport);
        if destination.is_empty() {
            return Plan::repaint(damage, "no-overlap");
        }
        let source = shifted(destination, (-delta.0, -delta.1), old.motion.viewport);
        let before: FxHashMap<_, _> = old
            .instances
            .iter()
            .enumerate()
            .map(|(n, i)| (i.key, (n, i)))
            .collect();
        let after: FxHashMap<_, _> = self.instances.iter().map(|i| (i.key, i)).collect();
        let mut holes = Vec::new();
        let mut last = None;
        for current in &self.instances {
            let Some(&(order, prior)) = before.get(&current.key) else {
                holes.push(current.visible);
                continue;
            };
            if last.is_some_and(|last| order <= last) {
                return Plan::repaint(damage, "painter-order-changed");
            }
            last = Some(order);
            let moved = shifted(prior.visible, delta, destination);
            let same_placement = i64::from(prior.origin.0) + delta.0 == i64::from(current.origin.0)
                && i64::from(prior.origin.1) + delta.1 == i64::from(current.origin.1);
            if current.signature == prior.signature && same_placement {
                let visible = current.visible.intersection(destination);
                holes.extend(subtract(moved, visible));
                holes.extend(subtract(visible, moved));
            } else if current.signature == prior.signature {
                let mut changed = vec![moved, current.visible.intersection(destination)];
                for old in prior.uniform {
                    for new in current.uniform {
                        let safe = shifted(old, delta, destination).intersection(new);
                        changed = changed
                            .into_iter()
                            .flat_map(|r| subtract(r, safe))
                            .collect();
                    }
                }
                holes.extend(changed);
            } else {
                holes.extend([moved, current.visible]);
            }
        }
        for prior in &old.instances {
            if !after.contains_key(&prior.key) {
                holes.push(shifted(prior.visible, delta, destination));
            }
        }
        let holes = Region::new(holes).intersection(destination);
        if holes.area() == area(destination) {
            return Plan::repaint(damage, "all-overlap-changed");
        }
        let repair = Region::new(
            damage
                .rects()
                .iter()
                .flat_map(|r| subtract(*r, destination))
                .chain(holes.rects().iter().copied()),
        );
        Plan {
            copy: (delta != (0, 0)).then_some((source, destination)),
            repair,
            reason: "reuse",
        }
    }
}

impl Plan {
    fn repaint(repair: Region, reason: &'static str) -> Self {
        Self {
            copy: None,
            repair,
            reason,
        }
    }
}

impl Target {
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn invalidate(&mut self) {
        self.state = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        frame: &PaintFrame,
        motion: Motion,
        background: Color,
        renderer: &mut Renderer,
        mode: RasterMode,
        threads: NonZeroU32,
        reuse: bool,
    ) -> Result<Sample, String> {
        let started = Instant::now();
        let previous = self.state.take();
        if frame.has_external_paint() {
            return Err(
                "target reuse experiment requires recorded paint, not external callbacks".into(),
            );
        }
        let size = frame.size();
        let length = (size.0 as usize)
            .checked_mul(size.1 as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("target size overflow")?;
        self.pixels.resize(length, 0);
        let state = State::capture(frame, motion, background, reuse);
        let plan = state.plan(previous.as_ref(), reuse);
        let planned = Instant::now();
        if let Some((source, destination)) = plan.copy {
            move_pixels(&mut self.pixels, size.0 as usize * 4, source, destination);
        }
        let copied = Instant::now();
        renderer.prepare_for(frame, mode, &plan.repair);
        let prepared = Instant::now();
        let mut painter = Painter::with_threads(&mut self.pixels, size.0, size.1, threads)
            .map_err(|e| e.to_string())?;
        painter.repair(&plan.repair);
        renderer.paint(frame, &mut painter, plan.repair.bounds(), background);
        let submitted = Instant::now();
        painter.finish().map_err(|e| e.to_string())?;
        let finished = Instant::now();
        let metadata_bytes = std::mem::size_of::<State>()
            + state.instances.capacity() * std::mem::size_of::<Instance>();
        self.state = Some(state);
        drop(previous);
        Ok(Sample {
            plan_ms: (planned - started).as_secs_f64() * 1000.0,
            copy_ms: (copied - planned).as_secs_f64() * 1000.0,
            prepare_ms: (prepared - copied).as_secs_f64() * 1000.0,
            submit_ms: (submitted - prepared).as_secs_f64() * 1000.0,
            finish_ms: (finished - submitted).as_secs_f64() * 1000.0,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
            copied_pixels: plan.copy.map_or(0, |(_, r)| area(r)),
            repaired_pixels: plan.repair.area(),
            metadata_bytes,
            reason: plan.reason,
            repair: plan.repair,
        })
    }
}

fn full_rect(size: (u32, u32)) -> Rect {
    Rect {
        width: size.0,
        height: size.1,
        ..Rect::default()
    }
}

fn area(r: Rect) -> u64 {
    u64::from(r.width) * u64::from(r.height)
}

fn uniform_regions(instance: &PaintInstance, full: Rect) -> [Rect; 2] {
    let [Command::Fill { bounds, radius, .. }] = instance.program.commands().as_slice() else {
        return [Rect::default(); 2];
    };
    let bounds = bounds.pixels(instance.program.scale());
    let inset = (radius * instance.program.scale()).ceil().max(0.0) as u32;
    let vertical = crate::PaintExtent {
        x: bounds.x.saturating_add(inset.min(i32::MAX as u32) as i32),
        width: bounds.width.saturating_sub(inset.saturating_mul(2)),
        ..bounds
    };
    let horizontal = crate::PaintExtent {
        y: bounds.y.saturating_add(inset.min(i32::MAX as u32) as i32),
        height: bounds.height.saturating_sub(inset.saturating_mul(2)),
        ..bounds
    };
    [vertical, horizontal].map(|r| r.visible(instance.origin(), instance.clip().intersection(full)))
}

fn shifted(r: Rect, delta: (i64, i64), clip: Rect) -> Rect {
    let x = (i64::from(r.x) + delta.0).max(i64::from(clip.x));
    let y = (i64::from(r.y) + delta.1).max(i64::from(clip.y));
    let right = (i64::from(r.x) + i64::from(r.width) + delta.0)
        .min(i64::from(clip.x) + i64::from(clip.width));
    let bottom = (i64::from(r.y) + i64::from(r.height) + delta.1)
        .min(i64::from(clip.y) + i64::from(clip.height));
    if right <= x || bottom <= y {
        return Rect::default();
    }
    Rect {
        x: x as u32,
        y: y as u32,
        width: (right - x) as u32,
        height: (bottom - y) as u32,
    }
}

fn subtract(a: Rect, b: Rect) -> Vec<Rect> {
    let i = a.intersection(b);
    if i.is_empty() {
        return vec![a];
    }
    [
        Rect {
            height: i.y - a.y,
            ..a
        },
        Rect {
            y: i.y + i.height,
            height: a.y + a.height - i.y - i.height,
            ..a
        },
        Rect {
            x: a.x,
            y: i.y,
            width: i.x - a.x,
            height: i.height,
        },
        Rect {
            x: i.x + i.width,
            y: i.y,
            width: a.x + a.width - i.x - i.width,
            height: i.height,
        },
    ]
    .into_iter()
    .filter(|r| !r.is_empty())
    .collect()
}

fn move_pixels(pixels: &mut [u8], stride: usize, source: Rect, destination: Rect) {
    assert_eq!(
        (source.width, source.height),
        (destination.width, destination.height)
    );
    // Each prior Painter is finished before this mutable borrow; row order handles 2D overlap.
    for row in 0..source.height {
        let row = if destination.y > source.y {
            source.height - row - 1
        } else {
            row
        };
        let from = (source.y + row) as usize * stride + source.x as usize * 4;
        let to = (destination.y + row) as usize * stride + destination.x as usize * 4;
        pixels.copy_within(from..from + source.width as usize * 4, to);
    }
}
