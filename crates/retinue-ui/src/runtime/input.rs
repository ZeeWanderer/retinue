use super::{ViewId, ViewTree};
use crate::input::{Activation, Dispatch, Event, InputOptions, Router, Target};
use rustc_hash::FxHashMap;

impl ViewTree {
    pub fn input(&self) -> &Router {
        &self.input
    }

    pub fn dispatch(&mut self, event: &Event) -> Dispatch {
        self.input.dispatch(event)
    }

    pub fn focus(&mut self, id: Option<ViewId>) -> bool {
        self.input.focus(id)
    }

    pub fn input_key(&self, id: ViewId) -> Option<crate::Key> {
        self.authored_key(self.geometry(id)?.key)
    }

    pub fn hit_test(&self, position: (f32, f32)) -> Option<crate::Key> {
        self.input_key(self.input.hit_test(position)?)
    }

    pub(super) fn sync_input(&mut self) {
        let revision = self.scene.input_revision();
        if !self.input_dirty && self.input_revision == revision {
            return;
        }
        let mut explicit = FxHashMap::default();
        for &id in &self.input_nodes {
            let node = self.nodes[&id].borrow();
            let input = node.input.input.clone().unwrap();
            let key = self.projection_owner(id).key();
            explicit.insert(key, (id, input));
        }
        let mut targets = Vec::with_capacity(self.input.targets().len());
        let mut ancestors = Vec::new();
        // Scene geometry is preorder; carry the nearest input ancestor in one pass.
        for geometry in self.scene.geometry() {
            while ancestors
                .last()
                .is_some_and(|(key, _)| Some(*key) != geometry.parent)
            {
                ancestors.pop();
            }
            let parent = ancestors.last().and_then(|(_, target)| *target);
            let target = explicit.remove(&geometry.key).or_else(|| {
                geometry.interactive.then(|| {
                    (
                        ViewId(geometry.key.value),
                        InputOptions {
                            activation: Activation::Release,
                            ..Default::default()
                        },
                    )
                })
            });
            let nearest = if let Some((id, input)) = target {
                targets.push(Target {
                    id,
                    parent,
                    bounds: geometry.layout,
                    clip: geometry.clip,
                    enabled: geometry.interactive,
                    input,
                });
                Some(id)
            } else {
                parent
            };
            ancestors.push((geometry.key, nearest));
        }
        self.input.update(targets);
        self.input_revision = revision;
        self.input_dirty = false;
        self.stats.input_updates += 1;
    }
}
