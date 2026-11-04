use super::*;

#[cfg(test)]
mod tests;

pub(super) struct Snapshot {
    geometry: Vec<Geometry>,
    projection: Vec<projection::Placement>,
    indices: HashMap<Key, usize>,
    dirty: Vec<bool>,
}

impl Snapshot {
    pub fn take(scene: &mut Scene) -> Option<Self> {
        let mut dirty = vec![false; scene.geometry.len()];
        let mut dirty_count = 0;
        // Parent movement does not dirty descendants; changes inside a subtree do.
        for &slot in &scene.tree.changed {
            let mut current = Some(slot);
            while let Some(slot) = current {
                let Some(node) = &scene.tree.nodes[slot] else {
                    break;
                };
                if let Some(&index) = scene.geometry_index.get(&node.element.key) {
                    if std::mem::replace(&mut dirty[index], true) {
                        break;
                    }
                    dirty_count += 1;
                    if dirty_count == dirty.len() {
                        return None;
                    }
                }
                current = node.parent;
            }
        }
        let snapshot = Self {
            geometry: std::mem::take(&mut scene.geometry),
            projection: std::mem::take(&mut scene.projection),
            indices: std::mem::take(&mut scene.geometry_index),
            dirty,
        };
        scene.geometry.reserve(snapshot.geometry.capacity());
        scene.projection.reserve(snapshot.projection.capacity());
        scene.geometry_index.reserve(snapshot.indices.capacity());
        Some(snapshot)
    }

    pub fn clean(&self, key: Key) -> Option<usize> {
        self.indices
            .get(&key)
            .copied()
            .filter(|&index| !self.dirty[index])
    }
}

impl Scene {
    pub(super) fn restore_projection(
        &mut self,
        snapshot: &Snapshot,
        start: usize,
        parent: Option<usize>,
        draft: &mut FrameDraft,
    ) -> Rect {
        let end = snapshot.projection[start].end;
        let base = self.geometry.len();
        // Published arrays stay dense and ordered, even for unchanged subtrees.
        let first_paint = snapshot.projection[start].instances.start;
        let paint_base = draft.instances.len();
        let paint_index = |index: usize| paint_base + (index - first_paint);
        let mut moved = true;
        for old_index in start..end {
            let index = self.geometry.len();
            let old = &snapshot.projection[old_index];
            let mut placement = old.clone();
            placement.parent = if old_index == start {
                parent
            } else {
                old.parent.map(|index| base + (index - start))
            };
            placement.end = base + (old.end - start);
            placement.instances = paint_index(old.instances.start)..paint_index(old.instances.end);
            placement.hit_enabled = placement.hit_testing
                && placement
                    .parent
                    .is_none_or(|parent| self.projection[parent].hit_enabled);
            let mut geometry = snapshot.geometry[old_index];
            geometry.parent = placement.parent.map(|parent| self.geometry[parent].key);
            geometry.interactive = placement.interactive && placement.hit_enabled;
            let paint = old.chunk.map(|chunk| {
                let instance = self.frame.instances()[chunk].clone();
                let bounds = instance.program.bounds();
                placement.chunk = Some(draft.instances.len());
                draft.instances.push(instance);
                bounds
            });
            if let Some(group) = old.group {
                let mut group = self.frame.groups()[group].clone();
                group.instances = placement.instances.clone();
                placement.group = Some(draft.groups.len());
                draft.groups.push(group);
            }
            self.geometry_index.insert(geometry.key, index);
            self.geometry.push(geometry);
            self.projection.push(placement);
            if moved {
                self.project_geometry(index, paint);
                if let Some(chunk) = self.projection[index].chunk {
                    let pixels = self.geometry[index].layout.pixels(draft.scale);
                    draft.instances[chunk]
                        .set_pixels((pixels.x, pixels.y), self.projection[index].clip);
                }
            }
            if old_index == start {
                moved = self.geometry[index].layout != geometry.layout
                    || self.geometry[index].clip != geometry.clip
                    || self.projection[index].clip != old.clip
                    || self.projection[index].hit_enabled != old.hit_enabled;
            }
            if let Some(group) = self.projection[index].group {
                let group = &mut draft.groups[group];
                if let Some(collection) = group.context.get::<CollectionContext>() {
                    let mut collection = collection.clone();
                    let bounds = self.geometry[index].layout.pixels(draft.scale);
                    collection.origin = (bounds.x, bounds.y);
                    for row in &mut collection.rows {
                        row.instances =
                            paint_index(row.instances.start)..paint_index(row.instances.end);
                    }
                    group.context = group.context.clone().with(collection);
                }
            }
        }
        if moved {
            for index in (base..self.geometry.len()).rev() {
                self.aggregate(index);
            }
        }
        self.geometry[base].paint
    }
}
