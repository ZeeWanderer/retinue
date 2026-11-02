use super::*;
use crate::layout::rendering::Collection;

impl Scene {
    pub(super) fn render_context(&self, index: usize, scale: f32) -> OptimizationContext {
        let placement = &self.projection[index];
        let node = self.tree.nodes[placement.node].as_ref().unwrap();
        let mut context = node
            .custom_cache
            .as_ref()
            .map(|cache| cache.rendering.clone())
            .unwrap_or_default();
        if let Some(collection) = context.get::<Collection>() {
            let resolved = self.collection_context(index, collection, scale);
            context = context.without::<Collection>();
            if let Some(resolved) = resolved {
                context = context.with(resolved);
            }
        }
        context.with(node.element.cache_hint)
    }

    fn collection_context(
        &self,
        index: usize,
        collection: &Collection,
        scale: f32,
    ) -> Option<CollectionContext> {
        let placement = &self.projection[index];
        let node = self.tree.nodes[placement.node].as_ref()?;
        if placement.chunk.is_some() {
            return None;
        }
        let mut rows = Vec::new();
        let mut child_end = 0;
        let mut paint_end = placement.instances.start;
        let mut keys = HashSet::default();
        for partition in &collection.partitions {
            if partition.children.start != child_end
                || partition.children.is_empty()
                || partition.children.end > node.children.len()
                || !keys.insert(partition.key)
            {
                return None;
            }
            let start = paint_end;
            for &child in &node.children[partition.children.clone()] {
                let key = self.tree.nodes[child].as_ref()?.element.key;
                if let Some(&child_index) = self.geometry_index.get(&key) {
                    let range = &self.projection[child_index].instances;
                    if range.start != paint_end {
                        return None;
                    }
                    paint_end = range.end;
                }
            }
            if paint_end > start {
                rows.push(CollectionRow {
                    key: partition.key,
                    instances: start..paint_end,
                });
            }
            child_end = partition.children.end;
        }
        if child_end != node.children.len() || paint_end != placement.instances.end {
            return None;
        }
        let bounds = self.geometry[index].layout.pixels(scale);
        Some(CollectionContext {
            origin: (bounds.x, bounds.y),
            rows,
            motion: collection.motion,
        })
    }
}
