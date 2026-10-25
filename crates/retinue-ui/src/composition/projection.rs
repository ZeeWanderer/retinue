use super::*;
use std::ops::Range;

pub(super) struct Placement {
    pub parent: Option<usize>,
    pub end: usize,
    pub local: Bounds,
    pub offset: (f32, f32),
    pub clipper: bool,
    pub clip: Rect,
    pub own_paint: Rect,
    pub chunk: Option<usize>,
}

impl Scene {
    pub(super) fn project(&mut self, mode: RecordingMode, resized: bool) -> Damage {
        self.stats = UpdateStats {
            layout_computations: self.tree.computed,
            ..UpdateStats::default()
        };
        self.tree.changed.sort_unstable();
        self.tree.changed.dedup();
        let paint_topology = self.tree.changed.iter().any(|&slot| {
            let Some(node) = &self.tree.nodes[slot] else {
                return true;
            };
            self.geometry_index
                .get(&node.element.key)
                .is_some_and(|&index| {
                    self.projection[index].chunk.is_some() != node.element.paint.is_some()
                })
        });
        let damage = if resized
            || self.tree.topology_changed
            || paint_topology
            || mode == RecordingMode::Always
        {
            self.rebuild_projection(mode, resized)
        } else {
            self.project_changes()
        };
        self.stats.visible_chunks = self
            .chunks
            .iter()
            .filter(|chunk| !chunk.visible.is_empty())
            .count();
        self.tree.changed.clear();
        damage
    }

    fn project_changes(&mut self) -> Damage {
        let mut ranges = Vec::new();
        let mut damage = Damage::None;
        for i in 0..self.tree.changed.len() {
            let slot = self.tree.changed[i];
            let node = self.tree.nodes[slot].as_mut().unwrap();
            let Some(&index) = self.geometry_index.get(&node.element.key) else {
                continue;
            };
            let placement = &mut self.projection[index];
            let local = Bounds::new(
                node.layout.location.x,
                node.layout.location.y,
                node.layout.size.width,
                node.layout.size.height,
            );
            let moved = placement.local != local
                || placement.offset != node.element.offset
                || placement.clipper != node.element.clip;
            placement.local = local;
            placement.offset = node.element.offset;
            placement.clipper = node.element.clip;
            self.geometry[index].interactive = node.element.interactive;
            let mut painted = false;
            if let (Some(paint), Some(chunk)) = (&node.element.paint, placement.chunk) {
                let signature = paint::Signature {
                    content: paint.key,
                    width: local.width.to_bits(),
                    height: local.height.to_bits(),
                    scale: self.scale.to_bits(),
                };
                if node
                    .recorded
                    .as_ref()
                    .is_none_or(|recorded| recorded.signature != signature)
                {
                    node.recorded = Some(paint::Recorded::new(paint, node.layout.size, self.scale));
                    self.stats.recordings += 1;
                    self.chunks[chunk].recorded = node.recorded.as_ref().unwrap().clone();
                    painted = true;
                }
                painted |= self.chunks[chunk].cache != paint.cache;
                self.chunks[chunk].cache = paint.cache;
            }
            if moved || painted {
                damage = damage.union(Damage::rect(self.geometry[index].paint));
                ranges.push(index..if moved { placement.end } else { index + 1 });
            }
        }
        if ranges.is_empty() {
            return Damage::None;
        }
        self.generation += 1;
        ranges.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(last) = merged.last_mut().filter(|last| range.start < last.end) {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        for range in &merged {
            for index in range.clone() {
                self.project_one(index);
            }
        }
        for range in merged.iter().rev() {
            for index in range.clone().rev() {
                self.aggregate(index);
            }
            let mut parent = self.projection[range.start].parent;
            while let Some(index) = parent {
                if !self.aggregate(index) {
                    break;
                }
                parent = self.projection[index].parent;
            }
        }
        for range in merged {
            damage = damage.union(Damage::rect(self.geometry[range.start].paint));
        }
        damage
    }

    fn project_one(&mut self, index: usize) {
        let placement = &self.projection[index];
        let (parent, clip) = placement.parent.map_or(
            (
                Bounds::default(),
                Rect {
                    width: self.size.0,
                    height: self.size.1,
                    ..Rect::default()
                },
            ),
            |parent| (self.geometry[parent].layout, self.projection[parent].clip),
        );
        let bounds = Bounds::new(
            parent.x + placement.local.x + placement.offset.0,
            parent.y + placement.local.y + placement.offset.1,
            placement.local.width,
            placement.local.height,
        );
        let pixels = bounds.pixels(self.scale);
        let hit = pixels.visible((0, 0), clip);
        let clip = if placement.clipper {
            clip.intersection(hit)
        } else {
            clip
        };
        let paint = if let Some(chunk) = placement.chunk {
            let chunk = &mut self.chunks[chunk];
            chunk.origin = (pixels.x, pixels.y);
            chunk.clip = clip;
            chunk.visible = chunk.recorded.bounds.visible(chunk.origin, clip);
            chunk.visible
        } else {
            Rect::default()
        };
        self.projection[index].clip = clip;
        self.projection[index].own_paint = paint;
        self.geometry[index].layout = bounds;
        self.geometry[index].hit = hit;
        self.stats.nodes += 1;
    }

    fn aggregate(&mut self, index: usize) -> bool {
        let mut paint = self.projection[index].own_paint;
        let mut child = index + 1;
        while child < self.projection[index].end {
            paint = paint.union(self.geometry[child].paint);
            child = self.projection[child].end;
        }
        let changed = paint != self.geometry[index].paint;
        self.geometry[index].paint = paint;
        changed
    }
}
