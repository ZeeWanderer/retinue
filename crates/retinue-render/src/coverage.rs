use super::{Key, PaintFrame, PaintGroup, PaintInstance};
use crate::{PaintExtent, Rect};
use std::{collections::HashSet, ops::Range};

mod tiles;

/// Preferred physical coverage size; the planner may decline tiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileHint {
    pub size: (u32, u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollectionMotion {
    #[default]
    Steady,
    Translating,
    Reordering,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionRow {
    pub key: Key,
    /// Indices in the frame, not relative to the group.
    pub instances: Range<usize>,
}

/// Advisory structure in resolved pixel coordinates, independent of the layout engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CollectionContext {
    /// Screen position of content (0, 0), after scaling and pixel snapping.
    pub origin: (i32, i32),
    pub rows: Vec<CollectionRow>,
    pub motion: CollectionMotion,
}

impl CollectionContext {
    pub(super) fn validate(&self, range: &Range<usize>) -> Result<(), &'static str> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let mut keys = HashSet::new();
        let mut start = range.start;
        for row in &self.rows {
            if !keys.insert(row.key) {
                return Err("duplicate collection row");
            }
            if row.instances.start != start
                || row.instances.is_empty()
                || row.instances.end > range.end
            {
                return Err("collection rows must partition their instance range");
            }
            start = row.instances.end;
        }
        if start != range.end {
            return Err("collection rows must partition their instance range");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Partition {
    Whole,
    Row(Key),
    Tile {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId {
    pub group: Key,
    pub partition: Partition,
}

impl ScopeId {
    pub const fn group(group: Key) -> Self {
        Self {
            group,
            partition: Partition::Whole,
        }
    }
}

pub(super) struct Coverage {
    pub id: ScopeId,
    pub instances: Range<usize>,
    pub content: PaintExtent,
    pub origin: (i32, i32),
    pub visible: Rect,
    pub contextual: bool,
}

pub(super) fn candidates(frame: &PaintFrame, group: &PaintGroup) -> Vec<Coverage> {
    if let Some(regions) = tiles::candidates(frame, group) {
        return regions;
    }
    let ranges = match group.collection() {
        Some(context) if context.motion == CollectionMotion::Reordering => return Vec::new(),
        Some(context) if !context.rows.is_empty() && rows_are_laminar(frame, group) => context
            .rows
            .iter()
            .map(|row| (Partition::Row(row.key), row.instances.clone()))
            .collect(),
        _ => vec![(Partition::Whole, group.instances.clone())],
    };
    ranges
        .into_iter()
        .filter_map(|(partition, instances)| {
            let chunks = &frame.chunks[instances.clone()];
            let visible = chunks
                .iter()
                .map(|c| c.visible)
                .filter(|r| !r.is_empty())
                .reduce(Rect::union)?;
            if chunks.iter().any(|c| c.program.analysis().effects.barrier) {
                return None;
            }
            let (content, origin) = if let Some(context) = group.collection() {
                let content = content_bounds(chunks, context.origin)?;
                let origin = (
                    i32::try_from(i64::from(context.origin.0) + i64::from(content.x)).ok()?,
                    i32::try_from(i64::from(context.origin.1) + i64::from(content.y)).ok()?,
                );
                (content, origin)
            } else {
                (
                    PaintExtent {
                        x: 0,
                        y: 0,
                        width: visible.width,
                        height: visible.height,
                    },
                    (
                        i32::try_from(visible.x).ok()?,
                        i32::try_from(visible.y).ok()?,
                    ),
                )
            };
            Some(Coverage {
                id: ScopeId {
                    group: group.key,
                    partition,
                },
                instances,
                content,
                origin,
                visible,
                contextual: group.collection().is_some(),
            })
        })
        .collect()
}

pub(super) fn is_hidden(frame: &PaintFrame, group: &PaintGroup, id: ScopeId) -> bool {
    if group
        .collection()
        .is_some_and(|c| c.motion == CollectionMotion::Reordering)
    {
        return false;
    }
    let range = match id.partition {
        Partition::Row(key) => group.collection().and_then(|c| {
            c.rows
                .iter()
                .find(|r| r.key == key)
                .map(|r| r.instances.clone())
        }),
        Partition::Whole if group.collection().is_none_or(|c| c.rows.is_empty()) => {
            Some(group.instances.clone())
        }
        Partition::Whole => None,
        Partition::Tile {
            x,
            y,
            width,
            height,
        } => {
            return declared(group, id)
                && group.collection().is_some_and(|context| {
                    let tile = PaintExtent {
                        x,
                        y,
                        width,
                        height,
                    };
                    frame.chunks[group.instances.clone()]
                        .iter()
                        .all(|chunk| tile.visible(context.origin, chunk.visible).is_empty())
                });
        }
    };
    range.is_some_and(|range| frame.chunks[range].iter().all(|c| c.visible.is_empty()))
}

pub(super) fn declared(group: &PaintGroup, id: ScopeId) -> bool {
    if group.key != id.group {
        return false;
    }
    match id.partition {
        Partition::Whole => true,
        Partition::Row(key) => group
            .collection()
            .is_some_and(|c| c.rows.iter().any(|r| r.key == key)),
        Partition::Tile { width, height, .. } => {
            group
                .collection()
                .is_some_and(|c| c.motion != CollectionMotion::Reordering)
                && group
                    .context
                    .get::<TileHint>()
                    .is_some_and(|hint| hint.size == (width, height))
        }
    }
}

pub(super) fn remaining(rect: Rect, covered: impl IntoIterator<Item = Rect>) -> Vec<Rect> {
    let mut regions = if rect.is_empty() {
        Vec::new()
    } else {
        vec![rect]
    };
    for cover in covered {
        regions = regions
            .into_iter()
            .flat_map(|r| uncovered(r, cover))
            .filter(|r| !r.is_empty())
            .collect();
        if regions.is_empty() {
            break;
        }
    }
    regions
}

pub(super) fn uncovered(rect: Rect, covered: Rect) -> [Rect; 4] {
    let overlap = rect.intersection(covered);
    if overlap.is_empty() {
        return [rect, Rect::default(), Rect::default(), Rect::default()];
    }
    [
        Rect {
            height: overlap.y - rect.y,
            ..rect
        },
        Rect {
            y: overlap.y + overlap.height,
            height: rect.height - (overlap.y - rect.y) - overlap.height,
            ..rect
        },
        Rect {
            y: overlap.y,
            width: overlap.x - rect.x,
            height: overlap.height,
            ..rect
        },
        Rect {
            x: overlap.x + overlap.width,
            y: overlap.y,
            width: rect.width - (overlap.x - rect.x) - overlap.width,
            height: overlap.height,
        },
    ]
}

pub(super) fn local_clip(rect: Rect, origin: (i32, i32), size: (u32, u32)) -> Rect {
    let x = (i64::from(rect.x) - i64::from(origin.0)).clamp(0, i64::from(size.0));
    let y = (i64::from(rect.y) - i64::from(origin.1)).clamp(0, i64::from(size.1));
    let right = (i64::from(rect.x) + i64::from(rect.width) - i64::from(origin.0))
        .clamp(0, i64::from(size.0));
    let bottom = (i64::from(rect.y) + i64::from(rect.height) - i64::from(origin.1))
        .clamp(0, i64::from(size.1));
    Rect {
        x: x as u32,
        y: y as u32,
        width: (right - x) as u32,
        height: (bottom - y) as u32,
    }
}

fn content_bounds(chunks: &[PaintInstance], origin: (i32, i32)) -> Option<PaintExtent> {
    let mut bounds: Option<(i64, i64, i64, i64)> = None;
    for chunk in chunks {
        let p = chunk.program.bounds();
        if p.width == 0 || p.height == 0 {
            continue;
        }
        let x = i64::from(chunk.origin.0) + i64::from(p.x) - i64::from(origin.0);
        let y = i64::from(chunk.origin.1) + i64::from(p.y) - i64::from(origin.1);
        let right = x + i64::from(p.width);
        let bottom = y + i64::from(p.height);
        bounds = Some(bounds.map_or((x, y, right, bottom), |(l, t, r, b)| {
            (l.min(x), t.min(y), r.max(right), b.max(bottom))
        }));
    }
    let (x, y, right, bottom) = bounds?;
    Some(PaintExtent {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        width: u32::try_from(right - x).ok()?,
        height: u32::try_from(bottom - y).ok()?,
    })
}

fn rows_are_laminar(frame: &PaintFrame, group: &PaintGroup) -> bool {
    group.collection().unwrap().rows.iter().all(|row| {
        frame.groups.iter().all(|other| {
            let a = &row.instances;
            let laminar = |b: &Range<usize>| {
                a.end <= b.start
                    || b.end <= a.start
                    || (a.start <= b.start && a.end >= b.end)
                    || (b.start <= a.start && b.end >= a.end)
            };
            laminar(&other.instances)
                && other
                    .collection()
                    .is_none_or(|c| c.rows.iter().all(|r| laminar(&r.instances)))
        })
    })
}
