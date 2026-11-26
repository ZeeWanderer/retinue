use super::*;
use std::ops::Range;

#[derive(Clone)]
pub(super) struct Placement {
    pub node: usize,
    pub parent: Option<usize>,
    pub end: usize,
    pub local: Bounds,
    pub content_insets: taffy::Rect<f32>,
    pub offset: (f32, f32),
    pub clipper: bool,
    pub clip: Rect,
    pub own_paint: Rect,
    pub chunk: Option<usize>,
    pub group: Option<usize>,
    pub instances: Range<usize>,
    pub interactive: bool,
    pub hit_testing: bool,
    pub hit_enabled: bool,
}

impl Scene {
    pub(super) fn project(&mut self, mode: RecordingMode, size: (u32, u32), scale: f32) -> Damage {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Projection);
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let resized = self.frame.size() != size || self.frame.scale() != scale;
        self.stats = UpdateStats {
            layout_computations: self.tree.state.computed,
            ..UpdateStats::default()
        };
        self.tree.state.changed.coalesce();
        let paint_topology = self.tree.state.changed.iter().any(|(slot, work)| {
            if !work.intersects(
                Work::LAYOUT
                    | Work::PAINT
                    | Work::GEOMETRY
                    | Work::STRUCTURE
                    | Work::RENDER_CONTEXT,
            ) {
                return false;
            }
            let Some(node) = &self.tree.nodes[slot] else {
                return true;
            };
            self.geometry_index
                .get(&self.tree.inputs[slot].as_ref().unwrap().key)
                .is_some_and(|&index| {
                    self.projection[index].chunk.is_some() != node.paint.is_some()
                        || self.projection[index].clipper != node.clip
                        || self.projection[index].group.is_some()
                            != (node.cache_hint != CacheHint::Dynamic
                                || self.tree.render_context(slot) != OptimizationContext::default())
                })
        });
        let damage = if resized
            || self.tree.topology_changed
            || paint_topology
            || mode == RecordingMode::Always
        {
            self.rebuild_projection(mode, size, scale)
        } else {
            self.project_changes()
        };
        self.stats.visible_chunks = self
            .frame
            .instances()
            .iter()
            .filter(|chunk| !chunk.visible_bounds().is_empty())
            .count();
        self.tree.state.changed.clear();
        #[cfg(feature = "profiling")]
        {
            self.profile.projection += started.elapsed();
        }
        damage
    }

    fn project_changes(&mut self) -> Damage {
        let mut ranges = Vec::new();
        let mut input_ranges = Vec::new();
        let mut geometry_moved = false;
        let mut hit_policy_changed = false;
        let mut updates = Vec::new();
        let mut groups = HashSet::default();
        for i in 0..self.tree.state.changed.len() {
            let (slot, work) = self.tree.state.changed.get(i);
            let node = self.tree.nodes[slot].as_mut().unwrap();
            let input = self.tree.inputs[slot].as_ref().unwrap();
            let layout = self.tree.state.nodes[slot].as_ref().unwrap().layout;
            let Some(&index) = self.geometry_index.get(&input.key) else {
                continue;
            };
            let placement = &mut self.projection[index];
            if placement.group.is_some()
                && work.intersects(
                    Work::LAYOUT
                        | Work::PAINT
                        | Work::GEOMETRY
                        | Work::STRUCTURE
                        | Work::RENDER_CONTEXT,
                )
            {
                groups.insert(index);
            }
            let local = Bounds::new(
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            );
            let moved = placement.local != local
                || placement.offset != node.offset
                || placement.clipper != node.clip;
            hit_policy_changed |= placement.hit_testing != node.hit_testing
                || placement.interactive != node.interactive
                || self.geometry[index].role != node.role
                || self.geometry[index].window_action != node.window_action;
            geometry_moved |= moved;
            placement.local = local;
            placement.content_insets = layout.border + layout.padding;
            placement.offset = node.offset;
            placement.clipper = node.clip;
            if placement.hit_testing != node.hit_testing {
                input_ranges.push(index..placement.end);
            } else if placement.interactive != node.interactive {
                input_ranges.push(index..index + 1);
            }
            placement.interactive = node.interactive;
            placement.hit_testing = node.hit_testing;
            self.geometry[index].role = node.role;
            self.geometry[index].window_action = node.window_action;
            let mut painted = false;
            if work.intersects(Work::PAINT | Work::GEOMETRY | Work::STRUCTURE)
                && let (Some(paint), Some(chunk)) = (&node.paint, placement.chunk)
            {
                let size = (local.width, local.height);
                let revision = self.tree.state.nodes[slot].as_ref().unwrap().revision;
                if paint.uses_layout && node.recorded_layout != revision
                    || node.recorded.as_ref().is_none_or(|recorded| {
                        !recorded.matches(paint.key, size, self.frame.scale())
                    })
                {
                    let result = self.tree.state.nodes[slot]
                        .as_ref()
                        .unwrap()
                        .result(node.custom_cache.as_ref());
                    #[cfg(feature = "profiling")]
                    let started = std::time::Instant::now();
                    node.recorded = Some(paint.compile(
                        &mut self.programs,
                        layout.size,
                        self.frame.scale(),
                        result,
                    ));
                    #[cfg(feature = "profiling")]
                    {
                        self.profile.recording += started.elapsed();
                    }
                    node.recorded_layout = revision;
                    self.stats.recordings += 1;
                    #[cfg(feature = "geometry-census")]
                    census::count(|c| c.recordings += 1);
                    painted = true;
                }
                painted |= self.frame.instances()[chunk].cache != paint.cache;
                if painted {
                    updates.push(PaintUpdate {
                        index: chunk,
                        program: node.recorded.as_ref().unwrap().clone(),
                        cache: paint.cache,
                    });
                }
            }
            if moved || painted {
                ranges.push(index..if moved { placement.end } else { index + 1 });
            }
        }
        let previous_revision = self.input_revision;
        if geometry_moved || hit_policy_changed {
            self.input_revision = self.input_revision.wrapping_add(1);
            self.input_motion = None;
        }
        for range in coalesce(input_ranges) {
            for index in range {
                let parent_enabled = self.projection[index]
                    .parent
                    .is_none_or(|parent| self.projection[parent].hit_enabled);
                let placement = &mut self.projection[index];
                placement.hit_enabled = placement.hit_testing && parent_enabled;
                self.geometry[index].interactive = placement.interactive && placement.hit_enabled;
            }
        }
        if ranges.is_empty() {
            return self
                .frame
                .patch_resolved(
                    [],
                    [],
                    groups
                        .into_iter()
                        .map(|index| GroupUpdate {
                            index: self.projection[index].group.unwrap(),
                            context: self.render_context(index, self.frame.scale()),
                        })
                        .collect::<Vec<_>>(),
                )
                .expect("valid scene context");
        }
        let merged = coalesce(ranges);
        let mut placements = Vec::new();
        for range in &merged {
            for index in range.clone() {
                self.project_one(index);
                if self.projection[index].group.is_some() {
                    groups.insert(index);
                }
                let geometry = &self.geometry[index];
                if let Some(chunk) = self.projection[index].chunk {
                    let pixels = geometry.layout.pixels(self.frame.scale());
                    placements.push(PlacementUpdate {
                        index: chunk,
                        origin: (pixels.x, pixels.y),
                        clip: self.projection[index].clip,
                    });
                }
            }
        }
        let groups = groups
            .into_iter()
            .map(|index| GroupUpdate {
                index: self.projection[index].group.unwrap(),
                context: self.render_context(index, self.frame.scale()),
            })
            .collect::<Vec<_>>();
        #[cfg(feature = "geometry-census")]
        census::count(|c| c.paint_placements += placements.len());
        let damage = self
            .frame
            .patch_resolved(updates, placements, groups)
            .expect("valid scene patch");
        let mut ancestors = std::collections::BTreeSet::new();
        for range in merged.iter().rev() {
            for index in range.clone().rev() {
                if self.aggregate(index)
                    && index == range.start
                    && let Some(parent) = self.projection[index].parent
                {
                    ancestors.insert(parent);
                }
            }
        }
        // Projection indices are in preorder. Process shared parents only after
        // their changed descendants; a recolor does not change ancestor bounds.
        while let Some(index) = ancestors.pop_last() {
            if self.aggregate(index)
                && let Some(parent) = self.projection[index].parent
            {
                ancestors.insert(parent);
            }
        }
        if geometry_moved && !hit_policy_changed {
            self.input_motion = Some((previous_revision, InputMotion::Ranges(merged)));
        }
        damage
    }

    fn project_one(&mut self, index: usize) {
        let placement = &self.projection[index];
        let paint = placement.chunk.map(|_| {
            self.tree.nodes[placement.node]
                .as_ref()
                .unwrap()
                .recorded
                .as_ref()
                .unwrap()
                .bounds()
        });
        self.project_geometry(index, paint);
    }

    pub(super) fn project_geometry(&mut self, index: usize, paint: Option<PaintExtent>) {
        #[cfg(feature = "geometry-census")]
        census::count(|c| c.world_resolves += 1);
        let placement = &self.projection[index];
        let (parent, clip) = placement.parent.map_or(
            (
                Bounds::default(),
                Rect {
                    width: self.frame.size().0,
                    height: self.frame.size().1,
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
        let input_clip = placement.parent.map_or_else(
            || {
                Bounds::new(
                    0.0,
                    0.0,
                    self.frame.size().0 as f32 / self.frame.scale(),
                    self.frame.size().1 as f32 / self.frame.scale(),
                )
            },
            |parent| self.geometry[parent].clip,
        );
        let input_clip = if placement.clipper {
            input_clip.intersection(bounds)
        } else {
            input_clip
        };
        let pixels = bounds.pixels(self.frame.scale());
        let hit = layout_pixels(bounds, self.frame.scale()).visible((0, 0), clip);
        let clip = if placement.clipper {
            clip.intersection(hit)
        } else {
            clip
        };
        let paint = paint.map_or(Rect::default(), |paint| {
            paint.visible((pixels.x, pixels.y), clip)
        });
        self.projection[index].clip = clip;
        self.projection[index].own_paint = paint;
        self.geometry[index].layout = bounds;
        self.geometry[index].clip = input_clip;
        self.geometry[index].hit = hit;
        self.stats.nodes += 1;
    }

    pub(super) fn aggregate(&mut self, index: usize) -> bool {
        let mut paint = self.projection[index].own_paint;
        let mut child = index + 1;
        while child < self.projection[index].end {
            #[cfg(test)]
            {
                self.stats.bounds_child_visits += 1;
            }
            paint = paint.union(self.geometry[child].paint);
            child = self.projection[child].end;
        }
        let changed = paint != self.geometry[index].paint;
        self.geometry[index].paint = paint;
        changed
    }
}

pub(super) fn layout_pixels(bounds: Bounds, scale: f32) -> retinue_render::PaintExtent {
    let x = Bounds::pixel_position(bounds.x, scale);
    let y = Bounds::pixel_position(bounds.y, scale);
    // Shared edges must convert identically; rounding width separately can overlap neighbors.
    retinue_render::PaintExtent {
        x,
        y,
        width: Bounds::pixel_position(bounds.x + bounds.width, scale)
            .saturating_sub(x)
            .max(0) as u32,
        height: Bounds::pixel_position(bounds.y + bounds.height, scale)
            .saturating_sub(y)
            .max(0) as u32,
    }
}

fn coalesce(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| range.start < last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}
