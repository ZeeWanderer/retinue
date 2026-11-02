use super::paint::Pixels;
use crate::Rect;
use std::ops::Range;

const LEAF: usize = 8;

pub(super) struct ReplayIndex {
    nodes: Vec<Node>,
}

struct Node {
    commands: Range<usize>,
    bounds: Option<Pixels>,
    end: usize,
}

impl ReplayIndex {
    pub fn reservation(commands: usize) -> Option<usize> {
        commands
            .checked_mul(2)?
            .max(4)
            .checked_mul(size_of::<Node>())
    }

    pub fn build(bounds: &[Option<Pixels>], cancelled: impl Fn() -> bool) -> Option<Self> {
        let mut index = Self { nodes: Vec::new() };
        index.append(bounds, 0, &cancelled)?;
        Some(index)
    }

    fn append(
        &mut self,
        bounds: &[Option<Pixels>],
        start: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Option<Pixels>> {
        if cancelled() {
            return None;
        }
        let node = self.nodes.len();
        self.nodes.push(Node {
            commands: start..start + bounds.len(),
            bounds: None,
            end: 0,
        });
        let extent = if bounds.len() > LEAF {
            let split = bounds.len() / 2;
            let a = self.append(&bounds[..split], start, cancelled)?;
            let b = self.append(&bounds[split..], start + split, cancelled)?;
            combine(a, b)
        } else {
            bounds
                .iter()
                .copied()
                .fold(Some(Pixels::default()), combine)
        };
        self.nodes[node].bounds = extent;
        self.nodes[node].end = self.nodes.len();
        Some(extent)
    }

    pub fn visit(
        &self,
        origin: (i32, i32),
        clip: Rect,
        mut draw: impl FnMut(Range<usize>),
    ) -> (usize, usize) {
        let mut next = 0;
        let mut inspected = 0;
        let mut skipped = 0;
        while let Some(node) = self.nodes.get(next) {
            inspected += 1;
            if node
                .bounds
                .is_some_and(|bounds| bounds.visible(origin, clip).is_empty())
            {
                skipped += node.commands.len();
                next = node.end;
            } else if node.end == next + 1 {
                draw(node.commands.clone());
                next = node.end;
            } else {
                next += 1;
            }
        }
        (inspected, skipped)
    }
}

fn combine(a: Option<Pixels>, b: Option<Pixels>) -> Option<Pixels> {
    let (a, b) = (a?, b?);
    if a.is_empty() {
        return Some(b);
    }
    if b.is_empty() {
        return Some(a);
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Some(Pixels {
        x,
        y,
        width: u32::try_from(
            (i64::from(a.x) + i64::from(a.width)).max(i64::from(b.x) + i64::from(b.width))
                - i64::from(x),
        )
        .ok()?,
        height: u32::try_from(
            (i64::from(a.y) + i64::from(a.height)).max(i64::from(b.y) + i64::from(b.height))
                - i64::from(y),
        )
        .ok()?,
    })
}

#[cfg(test)]
mod tests;
