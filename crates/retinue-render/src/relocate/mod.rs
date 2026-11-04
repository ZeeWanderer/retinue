//! Reuse completed presentation pixels without owning another pixel buffer.

use crate::surface::Snapshot as Scene;
#[cfg(test)]
use crate::surface::history::History;

#[cfg(any(test, feature = "benchmark"))]
mod experiment;
#[cfg(any(test, feature = "benchmark"))]
pub use experiment::{Motion, Sample, Target};

#[cfg(test)]
mod copy_tests;
#[cfg(test)]
mod planning_tests;

use crate::{Rect, damage::Region};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug)]
struct Copy {
    source: Rect,
    destination: Rect,
}

pub struct Plan {
    size: (u32, u32),
    copies: Vec<Copy>,
    pub repair: Region,
}

impl Plan {
    pub fn new(current: &Scene, previous: Option<&Scene>, repair: &Region) -> Self {
        Self::with_execution(current, previous, repair, crate::cost::Execution::SERIAL)
    }

    pub(crate) fn with_execution(
        current: &Scene,
        previous: Option<&Scene>,
        repair: &Region,
        execution: crate::cost::Execution,
    ) -> Self {
        let mut best = Self {
            size: current.size,
            copies: Vec::new(),
            repair: repair.clone(),
        };
        let Some(old) = previous.filter(|old| current.compatible(old)) else {
            return best;
        };
        let before: FxHashMap<_, _> = old
            .paint
            .iter()
            .enumerate()
            .map(|(n, p)| (p.key, (n, p)))
            .collect();
        let after: FxHashMap<_, _> = current.paint.iter().map(|p| (p.key, p)).collect();
        let mut votes: FxHashMap<_, (u64, Rect, Rect)> = FxHashMap::default();
        let mut last = None;
        for paint in &current.paint {
            let Some(&(order, prior)) = before.get(&paint.key) else {
                continue;
            };
            if last.is_some_and(|last| order <= last) {
                return best;
            }
            last = Some(order);
            if !paint.same_content(prior) {
                continue;
            }
            let delta = (
                i64::from(paint.origin.0) - i64::from(prior.origin.0),
                i64::from(paint.origin.1) - i64::from(prior.origin.1),
            );
            if delta == (0, 0) || paint.visible.is_empty() || prior.visible.is_empty() {
                continue;
            }
            let vote = votes.entry(delta).or_default();
            vote.0 = vote.0.saturating_add(area(paint.visible));
            vote.1 = vote.1.union(prior.visible);
            vote.2 = vote.2.union(paint.visible);
        }
        let mut candidates: Vec<_> = votes.into_iter().collect();
        candidates
            .sort_unstable_by_key(|(delta, (weight, _, _))| (std::cmp::Reverse(*weight), *delta));
        let required = current.repair_work(repair);
        let mut benefit = 0.0;
        for (delta, (_, old_bounds, new_bounds)) in candidates.into_iter().take(4) {
            let destination = shifted(old_bounds, delta, new_bounds).intersection(current.bounds());
            if destination.is_empty() {
                continue;
            }
            let source = shifted(destination, (-delta.0, -delta.1), old.bounds());
            if (source.width, source.height) != (destination.width, destination.height) {
                continue;
            }
            let mut holes = Vec::new();
            for paint in &current.paint {
                let Some(&(_, prior)) = before.get(&paint.key) else {
                    holes.push(paint.visible);
                    continue;
                };
                let moved = shifted(prior.visible, delta, destination);
                if paint.same_content(prior)
                    && i64::from(prior.origin.0) + delta.0 == i64::from(paint.origin.0)
                    && i64::from(prior.origin.1) + delta.1 == i64::from(paint.origin.1)
                {
                    let visible = paint.visible.intersection(destination);
                    holes.extend(subtract(moved, visible));
                    holes.extend(subtract(visible, moved));
                } else {
                    let mut changed = vec![moved, paint.visible.intersection(destination)];
                    if paint.same_content(prior) {
                        for a in prior.uniform {
                            for b in paint.uniform {
                                let safe = shifted(a, delta, destination).intersection(b);
                                changed = changed
                                    .into_iter()
                                    .flat_map(|r| subtract(r, safe))
                                    .collect();
                            }
                        }
                    }
                    holes.extend(changed);
                }
            }
            for prior in &old.paint {
                if !after.contains_key(&prior.key) {
                    holes.push(shifted(prior.visible, delta, destination));
                }
            }
            let holes = Region::new(holes).intersection(destination);
            let mut valid = vec![destination];
            for hole in holes.rects() {
                valid = valid.into_iter().flat_map(|r| subtract(r, *hole)).collect();
                if valid.len() > 16 {
                    break;
                }
            }
            if holes.area() >= area(destination) {
                continue;
            }
            let remaining = Region::new(
                repair
                    .rects()
                    .iter()
                    .flat_map(|r| subtract(*r, destination))
                    .chain(holes.rects().iter().copied()),
            );
            let avoided = required.difference(current.repair_work(&remaining));
            let whole = vec![Copy {
                source,
                destination,
            }];
            let split = (valid.len() <= 16).then(|| {
                valid
                    .into_iter()
                    .map(|destination| Copy {
                        source: shifted(destination, (-delta.0, -delta.1), old.bounds()),
                        destination,
                    })
                    .collect::<Vec<_>>()
            });
            for copies in std::iter::once(whole).chain(split) {
                let bytes = copies
                    .iter()
                    .map(|c| area(c.destination))
                    .sum::<u64>()
                    .saturating_mul(4);
                let candidate_benefit = transfer_benefit(execution, avoided, bytes, copies.len());
                if candidate_benefit <= benefit {
                    continue;
                }
                if let Some(copies) = ordered(copies) {
                    benefit = candidate_benefit;
                    best = Self {
                        size: current.size,
                        copies,
                        repair: remaining.clone(),
                    };
                }
            }
        }
        best
    }

    pub fn copied_pixels(&self) -> u64 {
        self.copies.iter().map(|copy| area(copy.destination)).sum()
    }

    pub fn copy_count(&self) -> usize {
        self.copies.len()
    }

    pub fn sources(&self) -> Region {
        Region::new(self.copies.iter().map(|copy| copy.source))
    }

    pub fn enqueue<'a>(
        &self,
        source: &crate::painter::PixelSource<'a>,
        painter: &mut crate::painter::Painter<'a>,
    ) {
        for copy in &self.copies {
            painter.copy_source(
                source,
                copy.source,
                (copy.destination.x, copy.destination.y),
            );
        }
    }

    /// Call before opening the target Painter; completion evidence is committed by the host.
    pub fn apply(&self, pixels: &mut [u8]) -> Result<(), &'static str> {
        let stride = (self.size.0 as usize)
            .checked_mul(4)
            .ok_or("target stride overflow")?;
        let length = stride
            .checked_mul(self.size.1 as usize)
            .ok_or("target size overflow")?;
        if pixels.len() != length {
            return Err("relocate target size mismatch");
        }
        for copy in &self.copies {
            move_pixels(pixels, stride, copy.source, copy.destination);
        }
        Ok(())
    }

    /// Copy from an independent completed target before opening the target Painter.
    #[cfg(any(test, feature = "benchmark"))]
    pub fn apply_from(&self, source: &[u8], target: &mut [u8]) -> Result<(), &'static str> {
        let stride = self.copy_stride(source, target)?;
        self.copy_rows(source, target, stride, 0);
        Ok(())
    }

    #[cfg(any(test, feature = "benchmark"))]
    pub(crate) fn apply_parallel(
        &self,
        pool: &rayon::ThreadPool,
        source: &[u8],
        target: &mut [u8],
        threads: u32,
    ) -> Result<(), &'static str> {
        if threads == 0 {
            return Err("copy thread count must be positive");
        }
        let stride = self.copy_stride(source, target)?;
        let bounds = self
            .copies
            .iter()
            .map(|c| c.destination)
            .fold(Rect::default(), Rect::union);
        if bounds.is_empty() {
            return Ok(());
        }
        let height = (bounds.height as usize).div_ceil(threads as usize);
        let start = bounds.y as usize * stride;
        let end = (bounds.y + bounds.height) as usize * stride;
        let mut chunks = target[start..end].chunks_mut(height * stride);
        let caller = chunks.next().unwrap();
        pool.in_place_scope(|scope| {
            for (n, chunk) in chunks.enumerate() {
                let y = bounds.y as usize + (n + 1) * height;
                scope.spawn(move |_| self.copy_rows(source, chunk, stride, y));
            }
            self.copy_rows(source, caller, stride, bounds.y as usize);
        });
        Ok(())
    }

    #[cfg(any(test, feature = "benchmark"))]
    fn copy_stride(&self, source: &[u8], target: &[u8]) -> Result<usize, &'static str> {
        if self.size.0 == 0 || self.size.1 == 0 {
            return Err("relocate source/target size must be positive");
        }
        let stride = (self.size.0 as usize)
            .checked_mul(4)
            .ok_or("target stride overflow")?;
        let length = stride
            .checked_mul(self.size.1 as usize)
            .ok_or("target size overflow")?;
        if source.len() != length || target.len() != length {
            return Err("relocate source/target size mismatch");
        }
        Ok(stride)
    }

    #[cfg(any(test, feature = "benchmark"))]
    fn copy_rows(&self, source: &[u8], target: &mut [u8], stride: usize, first: usize) {
        let last = first + target.len() / stride;
        for copy in &self.copies {
            let top = (copy.destination.y as usize).max(first);
            let bottom = ((copy.destination.y + copy.destination.height) as usize).min(last);
            if top >= bottom {
                continue;
            }
            let from = (copy.source.y as usize + top - copy.destination.y as usize) * stride
                + copy.source.x as usize * 4;
            let to = (top - first) * stride + copy.destination.x as usize * 4;
            let row = copy.source.width as usize * 4;
            if row == stride {
                let len = row * (bottom - top);
                target[to..to + len].copy_from_slice(&source[from..from + len]);
            } else {
                for y in 0..bottom - top {
                    let (from, to) = (from + y * stride, to + y * stride);
                    target[to..to + row].copy_from_slice(&source[from..from + row]);
                }
            }
        }
    }
}

fn transfer_benefit(
    execution: crate::cost::Execution,
    avoided: crate::cost::Work,
    bytes: u64,
    regions: usize,
) -> f64 {
    if [
        avoided.solid,
        avoided.blended,
        avoided.sampled,
        avoided.commands,
        avoided.edges,
        avoided.glyphs,
    ]
    .iter()
    .all(|v| *v <= 0.0)
    {
        return 0.0;
    }
    let replay = execution.replay(avoided);
    let transfer = execution.transfer(bytes, bytes, regions);
    if replay <= transfer * 1.1 {
        0.0
    } else {
        replay - transfer
    }
}

fn ordered(mut copies: Vec<Copy>) -> Option<Vec<Copy>> {
    let mut result = Vec::with_capacity(copies.len());
    while !copies.is_empty() {
        let index = copies.iter().enumerate().position(|(index, copy)| {
            copies.iter().enumerate().all(|(other, unread)| {
                index == other || copy.destination.intersection(unread.source).is_empty()
            })
        })?;
        result.push(copies.remove(index));
    }
    Some(result)
}

pub(crate) fn area(rect: Rect) -> u64 {
    u64::from(rect.width) * u64::from(rect.height)
}

pub(crate) fn shifted(r: Rect, delta: (i64, i64), clip: Rect) -> Rect {
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
        return (!a.is_empty()).then_some(a).into_iter().collect();
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
