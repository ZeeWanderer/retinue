use super::{ViewId, ViewTree};
use crate::{Key, scene::Geometry, view::Kind};
use std::{fmt, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmbiguousKey(pub Key);
impl fmt::Display for AmbiguousKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "multiple retained views have key {:?}; use a ViewId",
            self.0
        )
    }
}
impl std::error::Error for AmbiguousKey {}

pub struct ViewInfo {
    pub id: ViewId,
    pub parent: Option<ViewId>,
    pub key: Option<Key>,
    pub kind: &'static str,
    pub scene_key: Option<Key>,
    pub dependencies: usize,
    pub next_deadline: Option<Instant>,
    pub animating: bool,
}

impl ViewTree {
    pub fn named_geometry(&self) -> Vec<Geometry> {
        let mut result = Vec::new();
        for geometry in self.scene.geometry() {
            let mut id = ViewId(geometry.key.value);
            while let Some(node) = self.nodes.get(&id) {
                let node = node.borrow();
                if let Some(key) = node.input.key {
                    result.push(Geometry {
                        key,
                        parent: geometry.parent.and_then(|key| self.authored_key(key)),
                        ..*geometry
                    });
                }
                if !node.input.kind.transparent() {
                    break;
                }
                id = node.children[0];
            }
        }
        result
    }
    pub fn views(&self, key: impl Into<Key>) -> &[ViewId] {
        self.names.get(&key.into()).map_or(&[], Vec::as_slice)
    }

    pub fn unique(&self, key: impl Into<Key>) -> Result<Option<ViewId>, AmbiguousKey> {
        let key = key.into();
        match self.views(key) {
            [] => Ok(None),
            [id] => Ok(Some(*id)),
            _ => Err(AmbiguousKey(key)),
        }
    }

    pub fn geometry(&self, id: ViewId) -> Option<Geometry> {
        self.nodes.get(&id)?;
        self.scene.bounds(self.projection_owner(id).key())
    }

    pub fn authored_key(&self, scene_key: Key) -> Option<Key> {
        if scene_key.namespace != ViewId(0).key().namespace {
            return None;
        }
        let mut id = ViewId(scene_key.value);
        let mut child = id;
        while let Some(node) = self.nodes.get(&child) {
            let node = node.borrow();
            if let Some(key) = node.input.key {
                return Some(key);
            }
            if !node.input.kind.transparent() {
                break;
            }
            child = node.children[0];
        }
        loop {
            let node = self.nodes.get(&id)?.borrow();
            if let Some(key) = node.input.key {
                return Some(key);
            }
            id = node.parent?;
        }
    }

    pub fn inspect(&self) -> Vec<ViewInfo> {
        let mut result = Vec::with_capacity(self.nodes.len());
        let mut pending = self.root.into_iter().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            let node = self.nodes[&id].borrow();
            result.push(ViewInfo {
                id,
                parent: node.parent,
                key: node.input.key,
                kind: match &*node.input.kind {
                    Kind::View(view) => view.name(),
                    Kind::Primitive(primitive) => primitive.name(),
                    Kind::Layout(layout) => layout.kind.name(),
                    Kind::Environment(..) => "Environment",
                    Kind::Rectangle(_) => "Rectangle",
                    Kind::Empty => "Empty",
                    Kind::Fragment(_) => "Fragment",
                },
                scene_key: self.geometry(id).map(|geometry| geometry.key),
                dependencies: node
                    .body
                    .as_ref()
                    .map_or(0, |body| body.observation.dependency_count()),
                next_deadline: node.body.as_ref().and_then(|body| body.deadlines.next),
                animating: self.animating(id),
            });
            pending.extend(node.children.iter().rev().copied());
        }
        result
    }
}
