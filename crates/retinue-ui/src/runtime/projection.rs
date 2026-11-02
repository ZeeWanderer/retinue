use super::{ViewId, ViewTree, profile::timed};
use crate::{
    Bounds,
    scene::{self, Change, Paint, RecordingMode},
    view::Kind,
};
use retinue_render::{Command, PaintKey, damage::Damage};
use rustc_hash::FxHashSet as HashSet;
use taffy::{Dimension, Size, Style};

impl ViewTree {
    pub(super) fn project(&mut self, size: (u32, u32), scale: f32) -> Result<Damage, &'static str> {
        let resized = self.frame().size() != size || self.frame().scale() != scale;
        let result = if self.full_projection {
            let root = timed!(self, lowering, self.project_root());
            timed!(
                self,
                scene,
                self.scene
                    .update(root, size, scale, RecordingMode::Retained)
            )
        } else if self.changed.is_empty() {
            if resized {
                timed!(self, scene, self.scene.resize(size, scale))
            } else {
                self.scene.stats = scene::UpdateStats {
                    visible_chunks: self.scene.stats.visible_chunks,
                    ..scene::UpdateStats::default()
                };
                Ok(Damage::default())
            }
        } else if let Some(owners) = self.property_owners() {
            let patches = timed!(
                self,
                lowering,
                owners
                    .into_iter()
                    .flat_map(|id| {
                        let element = self.lower_patch(id, false);
                        let key = element.key;
                        [
                            Change::Layout(key, element.style, element.algorithm),
                            Change::LayoutValues(key, element.layout_values),
                            Change::Paint(key, element.paint),
                            Change::Offset(key, element.offset),
                            Change::Clip(key, element.clip),
                            Change::Interactive(key, element.interactive),
                            Change::HitTesting(key, element.hit_testing),
                            Change::Role(key, element.role),
                            Change::WindowAction(key, element.window_action),
                            Change::Reuse(key, element.cache_hint),
                        ]
                    })
                    .collect::<Vec<_>>()
            );
            timed!(self, scene, self.scene.patch_at(patches, size, scale))
        } else {
            let mut dirty = HashSet::default();
            for &id in &self.changed {
                let mut current = Some(id);
                while let Some(id) = current {
                    if !dirty.insert(id) {
                        break;
                    }
                    current = self.nodes[&id].borrow().parent;
                }
            }
            let mut roots = HashSet::default();
            for id in &self.changed {
                let mut owner = self.projection_owner(*id);
                while !self.scene.contains(owner.key()) || !self.projects_single(owner) {
                    owner = self.nodes[&owner]
                        .borrow()
                        .parent
                        .map(|parent| self.projection_owner(parent))
                        .unwrap_or(ViewId(0));
                }
                roots.insert(owner);
            }
            let candidates = roots.clone();
            roots.retain(|id| {
                let mut parent = if id.0 == 0 {
                    None
                } else {
                    self.nodes[id].borrow().parent.or(Some(ViewId(0)))
                };
                while let Some(id) = parent {
                    if candidates.contains(&id) {
                        return false;
                    }
                    parent = if id.0 == 0 {
                        None
                    } else {
                        self.nodes[&id].borrow().parent.or(Some(ViewId(0)))
                    };
                }
                true
            });
            let mut roots: Vec<_> = roots.into_iter().collect();
            roots.sort_unstable();
            let patches: Vec<_> = timed!(
                self,
                lowering,
                roots
                    .into_iter()
                    .map(|id| {
                        if id.0 == 0 {
                            self.project_root().into()
                        } else {
                            self.lower_retained_patch(id, &dirty)
                        }
                    })
                    .collect()
            );
            timed!(
                self,
                scene,
                self.scene.update_retained_subtrees_at(patches, size, scale)
            )
        };
        match result {
            Ok(damage) => {
                self.changed.clear();
                self.structural.clear();
                self.full_projection = false;
                Ok(damage)
            }
            Err(error) => {
                self.full_projection = true;
                Err(error)
            }
        }
    }

    fn property_owners(&self) -> Option<HashSet<ViewId>> {
        if !self.structural.is_empty() {
            return None;
        }
        let mut owners = HashSet::default();
        for id in &self.changed {
            let owner = self.projection_owner(*id);
            if !self.scene.contains(owner.key()) || !self.projects_single(owner) {
                return None;
            }
            owners.insert(owner);
        }
        Some(owners)
    }

    pub(super) fn projection_owner(&self, mut id: ViewId) -> ViewId {
        while let Some(parent) = self.nodes[&id].borrow().parent {
            if !self.nodes[&parent].borrow().input.kind.transparent() {
                break;
            }
            id = parent;
        }
        id
    }

    fn projects_single(&self, mut id: ViewId) -> bool {
        if id.0 == 0 {
            return true;
        }
        loop {
            let node = self.nodes[&id].borrow();
            match &*node.input.kind {
                Kind::Fragment(_) => return false,
                Kind::View(_) | Kind::Environment(..) => id = node.children[0],
                _ => return true,
            }
        }
    }

    fn project_root(&mut self) -> scene::Element {
        let mut root = scene::Element::new(
            ViewId(0).key(),
            Style {
                align_items: Some(taffy::AlignItems::START),
                size: Size {
                    width: Dimension::percent(1.0),
                    height: Dimension::percent(1.0),
                },
                ..Style::default()
            },
        )
        .with_children(self.lower_many(self.root.unwrap(), self.root.unwrap()));
        root.algorithm = scene::Algorithm::Custom(crate::layout::LayoutAlgorithm::new(
            crate::view::stack_layout::StackLayout {
                alignment: crate::view::Alignment::Start,
            },
        ));
        root
    }

    fn lower_many(&mut self, id: ViewId, owner: ViewId) -> Vec<scene::Element> {
        let node = self.nodes[&id].clone();
        let node = node.borrow();
        match &*node.input.kind {
            Kind::Fragment(_) => {
                let mut elements: Vec<_> = node
                    .children
                    .iter()
                    .flat_map(|&child| self.lower_many(child, child))
                    .collect();
                for element in &mut elements {
                    element.layout_values = element
                        .layout_values
                        .clone()
                        .overlaid(&node.input.layout_values);
                    node.input.attributes.apply(element);
                }
                elements
            }
            Kind::View(_) | Kind::Environment(..) => {
                let mut elements = self.lower_many(node.children[0], owner);
                for element in &mut elements {
                    element.layout_values = element
                        .layout_values
                        .clone()
                        .overlaid(&node.input.layout_values);
                    node.input.attributes.apply(element);
                }
                elements
            }
            _ => vec![self.lower(id, owner)],
        }
    }

    fn lower(&mut self, id: ViewId, owner: ViewId) -> scene::Element {
        self.lower_node(id, owner, true)
    }

    fn lower_patch(&mut self, id: ViewId, descendants: bool) -> scene::Element {
        let mut element = self.lower_node(id, id, descendants);
        self.apply_ancestors(id, &mut element);
        element
    }

    fn apply_ancestors(&self, id: ViewId, element: &mut scene::Element) {
        let mut parent = self.nodes[&id].borrow().parent;
        // Flattened ancestors are absent from the repaired scene subtree.
        while let Some(id) = parent {
            let node = self.nodes[&id].borrow();
            if !matches!(
                &*node.input.kind,
                Kind::View(_) | Kind::Environment(..) | Kind::Fragment(_)
            ) {
                break;
            }
            element.layout_values = element
                .layout_values
                .clone()
                .overlaid(&node.input.layout_values);
            node.input.attributes.apply(element);
            parent = node.parent;
        }
    }

    fn lower_retained_patch(
        &mut self,
        id: ViewId,
        dirty: &HashSet<ViewId>,
    ) -> scene::layout::Subtree {
        let mut roots = self.lower_retained(id, id, dirty, false);
        assert_eq!(roots.len(), 1);
        let mut root = roots.pop().unwrap();
        if let scene::layout::Subtree::Replace(element, _) = &mut root {
            self.apply_ancestors(id, element);
        }
        root
    }

    fn lower_retained(
        &mut self,
        id: ViewId,
        owner: ViewId,
        dirty: &HashSet<ViewId>,
        force: bool,
    ) -> Vec<scene::layout::Subtree> {
        use scene::layout::Subtree;
        let node = self.nodes[&id].clone();
        let node = node.borrow();
        let force = force || self.changed.contains(&id);
        match &*node.input.kind {
            Kind::View(_) | Kind::Environment(..) | Kind::Fragment(_) => {
                let fragment = matches!(&*node.input.kind, Kind::Fragment(_));
                let mut children = node
                    .children
                    .iter()
                    .flat_map(|&child| {
                        self.lower_retained(
                            child,
                            if fragment { child } else { owner },
                            dirty,
                            force,
                        )
                    })
                    .collect::<Vec<_>>();
                for child in &mut children {
                    if let Subtree::Replace(element, _) = child {
                        element.layout_values = element
                            .layout_values
                            .clone()
                            .overlaid(&node.input.layout_values);
                        node.input.attributes.apply(element);
                    }
                }
                children
            }
            _ if !force && !dirty.contains(&id) && self.scene.contains(owner.key()) => {
                vec![Subtree::Retained(owner.key())]
            }
            _ => {
                let element = self.lower_node(id, owner, false);
                let children = node
                    .children
                    .iter()
                    .flat_map(|&child| self.lower_retained(child, child, dirty, false))
                    .collect();
                vec![Subtree::Replace(Box::new(element), children)]
            }
        }
    }

    fn lower_node(&mut self, id: ViewId, owner: ViewId, descendants: bool) -> scene::Element {
        let node = self.nodes[&id].clone();
        let node = node.borrow();
        let presentation = self.presentations.get(&id).map(|track| track.value.clone());
        let mut element = match &*node.input.kind {
            Kind::View(_) | Kind::Environment(..) => {
                let mut element = self.lower_node(node.children[0], owner, descendants);
                element.layout_values = element.layout_values.overlaid(&node.input.layout_values);
                node.input.attributes.apply(&mut element);
                return element;
            }
            Kind::Layout(layout) => {
                self.stats.projected += 1;
                let children = if descendants {
                    node.children
                        .iter()
                        .flat_map(|&child| self.lower_many(child, child))
                        .collect()
                } else {
                    Vec::new()
                };
                let style = presentation
                    .as_ref()
                    .and_then(|value| value.style.clone())
                    .unwrap_or_else(|| layout.style.clone());
                let mut element = scene::Element::new(owner.key(), style).with_children(children);
                element.algorithm = layout.algorithm.clone();
                layout.properties.apply(&mut element);
                if let Some(value) = &presentation {
                    element.offset = value.offset;
                }
                element
            }
            Kind::Primitive(_) => {
                self.stats.projected += 1;
                let output = node.primitive.as_ref().expect("resolved primitive");
                let style = presentation
                    .as_ref()
                    .and_then(|value| value.style.clone())
                    .unwrap_or_else(|| output.style.clone());
                let mut element = scene::Element::new(owner.key(), style);
                element.algorithm = output.algorithm.clone();
                element.paint = Some(crate::view::primitive::paint(
                    output,
                    PaintKey::new(id.key(), node.revision),
                ));
                element
            }
            Kind::Rectangle(rectangle) => {
                self.stats.projected += 1;
                let rectangle = presentation
                    .as_ref()
                    .and_then(|value| value.rectangle)
                    .unwrap_or(*rectangle);
                let mut element = scene::Element::new(
                    owner.key(),
                    Style {
                        size: Size {
                            width: Dimension::percent(1.0),
                            height: Dimension::percent(1.0),
                        },
                        ..Style::default()
                    },
                );
                element.paint = Some(Paint::new(
                    PaintKey::new(id.key(), node.revision),
                    move |size| {
                        vec![Command::Fill {
                            bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                            radius: rectangle.radius,
                            color: rectangle.color,
                        }]
                    },
                ));
                element
            }
            Kind::Empty => {
                self.stats.projected += 1;
                scene::Element::new(
                    owner.key(),
                    Style {
                        display: taffy::Display::None,
                        ..Style::default()
                    },
                )
            }
            Kind::Fragment(_) => unreachable!("fragments are flattened by their arranging parent"),
        };
        element.layout_key = self.nodes[&owner].borrow().input.key;
        element.layout_values = element.layout_values.overlaid(&node.input.layout_values);
        node.input.attributes.apply(&mut element);
        element
    }
}
