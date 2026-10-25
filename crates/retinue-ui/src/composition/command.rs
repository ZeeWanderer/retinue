use super::{Command, paint::Pixels};
use crate::Rect;
use std::rc::Rc;

impl PartialEq for Command {
    fn eq(&self, other: &Self) -> bool {
        use Command::*;
        match (self, other) {
            (
                Ring {
                    center: a,
                    radius: b,
                    thickness: c,
                    color: d,
                },
                Ring {
                    center: w,
                    radius: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                Translate {
                    offset: a,
                    commands: b,
                },
                Translate {
                    offset: x,
                    commands: y,
                },
            ) => a == x && (Rc::ptr_eq(b, y) || b == y),
            (
                Fill {
                    bounds: a,
                    radius: b,
                    color: c,
                },
                Fill {
                    bounds: x,
                    radius: y,
                    color: z,
                },
            ) => (a, b, c) == (x, y, z),
            (
                Outline {
                    bounds: a,
                    radius: b,
                    thickness: c,
                    color: d,
                },
                Outline {
                    bounds: w,
                    radius: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                InsetShadow {
                    bounds: a,
                    radius: b,
                    blur: c,
                    spread: d,
                    color: e,
                },
                InsetShadow {
                    bounds: v,
                    radius: w,
                    blur: x,
                    spread: y,
                    color: z,
                },
            ) => (a, b, c, d, e) == (v, w, x, y, z),
            (
                Text {
                    bounds: a,
                    size: b,
                    text: c,
                    color: d,
                    align: e,
                    font: f,
                },
                Text {
                    bounds: u,
                    size: v,
                    text: w,
                    color: x,
                    align: y,
                    font: z,
                },
            ) => (a, b, c, d, e) == (u, v, w, x, y) && Rc::ptr_eq(f, z),
            (
                Image {
                    bounds: a,
                    image: b,
                    fit: c,
                },
                Image {
                    bounds: x,
                    image: y,
                    fit: z,
                },
            ) => (a, b, c) == (x, y, z),
            (
                Clip {
                    bounds: a,
                    commands: b,
                },
                Clip {
                    bounds: x,
                    commands: y,
                },
            ) => a == x && (Rc::ptr_eq(b, y) || b == y),
            (
                Line {
                    from: a,
                    to: b,
                    thickness: c,
                    color: d,
                },
                Line {
                    from: w,
                    to: x,
                    thickness: y,
                    color: z,
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (
                Polygon {
                    points: a,
                    color: b,
                },
                Polygon {
                    points: x,
                    color: y,
                },
            ) => (a, b) == (x, y),
            (External { bounds: a, key: b }, External { bounds: x, key: y }) => (a, b) == (x, y),
            _ => false,
        }
    }
}

impl Command {
    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Text { text, .. } => text.capacity(),
                Self::Polygon { points, .. } => points.capacity() * size_of::<[f32; 2]>(),
                Self::Clip { commands, .. } | Self::Translate { commands, .. } => {
                    commands.iter().map(Self::retained_bytes).sum()
                }
                _ => 0,
            }
    }

    pub(super) fn cacheable(&self) -> bool {
        match self {
            Self::External { .. } => false,
            Self::Clip { commands, .. } | Self::Translate { commands, .. } => {
                commands.iter().all(Self::cacheable)
            }
            _ => true,
        }
    }
}

pub(super) fn changed_regions(
    old: &[Command],
    new: &[Command],
    scale: f32,
    bounds: Pixels,
) -> Vec<Rect> {
    let canvas = Rect {
        width: bounds.width,
        height: bounds.height,
        ..Rect::default()
    };
    let mut regions: Vec<Rect> = Vec::new();
    for index in 0..old.len().max(new.len()) {
        if old.get(index) == new.get(index) {
            continue;
        }
        for command in [old.get(index), new.get(index)].into_iter().flatten() {
            let mut rect = command
                .bounds()
                .pixels(scale)
                .visible((-bounds.x, -bounds.y), canvas);
            if rect.is_empty() {
                continue;
            }
            // Keep repairs disjoint: replaying translucent content twice changes its pixels.
            let mut i = 0;
            while i < regions.len() {
                if !rect.intersection(regions[i]).is_empty() {
                    rect = rect.union(regions.swap_remove(i));
                    i = 0;
                } else {
                    i += 1;
                }
            }
            regions.push(rect);
        }
    }
    if regions.len() > 16 {
        vec![canvas]
    } else {
        regions
    }
}
