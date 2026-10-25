use super::{Element, Key};
use std::collections::{HashMap, HashSet};
use taffy::{
    AvailableSpace, Cache, CacheTree, Layout, LayoutFlexboxContainer, LayoutGridContainer,
    LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, Point, RunMode, Size, Style,
    TraversePartialTree, TraverseTree, compute_cached_layout, compute_flexbox_layout,
    compute_grid_layout, compute_hidden_layout, compute_leaf_layout, compute_root_layout,
};

#[derive(Clone, Copy, Default)]
pub enum Algorithm {
    #[default]
    Flex,
    Grid,
    Custom(fn(&mut LayoutContext<'_>) -> Size<f32>),
}

impl PartialEq for Algorithm {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flex, Self::Flex) | (Self::Grid, Self::Grid) => true,
            (Self::Custom(a), Self::Custom(b)) => std::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
}

pub struct LayoutContext<'a> {
    tree: &'a mut Tree,
    node: usize,
    input: LayoutInput,
}

impl LayoutContext<'_> {
    pub fn style(&self) -> &Style {
        &self.tree.nodes[self.node].as_ref().unwrap().element.style
    }

    pub fn child_count(&self) -> usize {
        self.tree.nodes[self.node].as_ref().unwrap().children.len()
    }

    pub fn container_size(&self, intrinsic: Size<f32>) -> Size<f32> {
        compute_leaf_layout(
            self.input,
            &self.tree.nodes[self.node].as_ref().unwrap().element.style,
            |_, _| 0.0,
            |_, _| intrinsic,
        )
        .size
    }

    pub fn measure(&mut self, child: usize, available: Size<f32>) -> Size<f32> {
        let id = self.tree.get_child_id(self.node.into(), child);
        self.tree
            .compute_child_layout(
                id,
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: taffy::SizingMode::InherentSize,
                    axis: taffy::RequestedAxis::Both,
                    known_dimensions: Size::NONE,
                    parent_size: available.map(Some),
                    available_space: available.map(AvailableSpace::Definite),
                    ..self.input
                },
            )
            .size
    }

    pub fn place(&mut self, child: usize, location: Point<f32>, size: Size<f32>) {
        if self.input.run_mode != RunMode::PerformLayout {
            return;
        }
        let id = self.tree.get_child_id(self.node.into(), child);
        self.tree.compute_child_layout(
            id,
            LayoutInput {
                sizing_mode: taffy::SizingMode::InherentSize,
                axis: taffy::RequestedAxis::Both,
                known_dimensions: size.map(Some),
                parent_size: self.input.known_dimensions,
                available_space: size.map(AvailableSpace::Definite),
                ..self.input
            },
        );
        self.tree.set_unrounded_layout(
            id,
            &Layout {
                order: child as u32,
                location,
                size,
                ..Layout::default()
            },
        );
    }

    pub fn anchor(
        &mut self,
        child: usize,
        frame: super::Bounds,
        alignment: Point<f32>,
        offset: Point<f32>,
    ) {
        let size = self.measure(
            child,
            Size {
                width: frame.width,
                height: frame.height,
            },
        );
        self.place(
            child,
            Point {
                x: frame.x + (frame.width - size.width) * alignment.x + offset.x,
                y: frame.y + (frame.height - size.height) * alignment.y + offset.y,
            },
            size,
        );
    }
}

pub(super) struct Node {
    pub element: Element,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub layout: Layout,
    cache: Cache,
    pub recorded: Option<super::paint::Recorded>,
}

#[derive(Default)]
pub(super) struct Tree {
    pub nodes: Vec<Option<Node>>,
    keys: HashMap<Key, usize>,
    free: Vec<usize>,
    pub computed: usize,
    pub changed: Vec<usize>,
    pub topology_changed: bool,
}

impl Tree {
    pub fn slot(&self, key: Key) -> Option<usize> {
        self.keys.get(&key).copied()
    }

    pub fn reconcile(&mut self, root: Element) -> Result<usize, &'static str> {
        fn unique(element: &Element, seen: &mut HashSet<Key>) -> bool {
            seen.insert(element.key) && element.children.iter().all(|child| unique(child, seen))
        }
        let mut seen = HashSet::new();
        if !unique(&root, &mut seen) {
            return Err("duplicate composition key");
        }
        self.changed.clear();
        self.topology_changed = false;
        let root = self.insert(root, None);
        self.keys.retain(|key, slot| {
            if seen.contains(key) {
                true
            } else {
                self.nodes[*slot] = None;
                self.free.push(*slot);
                false
            }
        });
        Ok(root)
    }

    pub fn replace(&mut self, root: Element) -> Result<(), &'static str> {
        let slot = self.slot(root.key).ok_or("unknown composition key")?;
        let mut previous = HashSet::new();
        fn descendants(tree: &Tree, slot: usize, keys: &mut HashSet<Key>) {
            let node = tree.nodes[slot].as_ref().unwrap();
            keys.insert(node.element.key);
            for &child in &node.children {
                descendants(tree, child, keys);
            }
        }
        descendants(self, slot, &mut previous);
        let mut seen = HashSet::new();
        fn validate(
            tree: &Tree,
            element: &Element,
            previous: &HashSet<Key>,
            seen: &mut HashSet<Key>,
        ) -> bool {
            seen.insert(element.key)
                && (!tree.keys.contains_key(&element.key) || previous.contains(&element.key))
                && element
                    .children
                    .iter()
                    .all(|child| validate(tree, child, previous, seen))
        }
        if !validate(self, &root, &previous, &mut seen) {
            return Err("duplicate or out-of-scope composition key");
        }
        self.changed.clear();
        self.topology_changed = false;
        let parent = self.nodes[slot].as_ref().unwrap().parent;
        self.insert(root, parent);
        for key in previous.difference(&seen) {
            let slot = self.keys.remove(key).unwrap();
            self.nodes[slot] = None;
            self.free.push(slot);
        }
        Ok(())
    }

    fn insert(&mut self, mut element: Element, parent: Option<usize>) -> usize {
        let descriptions = std::mem::take(&mut element.children);
        let slot = *self.keys.entry(element.key).or_insert_with(|| {
            self.free.pop().unwrap_or_else(|| {
                self.nodes.push(None);
                self.nodes.len() - 1
            })
        });
        let changed = self.nodes[slot].as_ref().is_none_or(|old| {
            old.element.style != element.style || old.element.algorithm != element.algorithm
        });
        let projected = self.nodes[slot].as_ref().is_none_or(|old| {
            changed
                || old.element.offset != element.offset
                || old.element.clip != element.clip
                || old.element.interactive != element.interactive
                || old
                    .element
                    .paint
                    .as_ref()
                    .map(|paint| (paint.key, paint.cache))
                    != element.paint.as_ref().map(|paint| (paint.key, paint.cache))
        });
        self.topology_changed |= self.nodes[slot].as_ref().is_none_or(|old| {
            old.parent != parent
                || (old.element.style.display == taffy::Display::None)
                    != (element.style.display == taffy::Display::None)
        });
        if projected {
            self.changed.push(slot);
        }
        if let Some(node) = &mut self.nodes[slot] {
            node.element = element;
            node.parent = parent;
        } else {
            self.nodes[slot] = Some(Node {
                element,
                children: Vec::new(),
                parent,
                layout: Layout::default(),
                cache: Cache::new(),
                recorded: None,
            });
        }
        let children: Vec<_> = descriptions
            .into_iter()
            .map(|child| self.insert(child, Some(slot)))
            .collect();
        let node = self.nodes[slot].as_mut().unwrap();
        let reordered = node.children != children;
        self.topology_changed |= reordered;
        let changed = changed || reordered;
        node.children = children;
        if changed {
            self.invalidate(slot);
        }
        slot
    }

    fn invalidate(&mut self, mut slot: usize) {
        loop {
            let node = self.nodes[slot].as_mut().unwrap();
            node.cache.clear();
            let Some(parent) = node.parent else { break };
            slot = parent;
        }
    }

    pub fn compute(&mut self, root: usize, size: Size<f32>) {
        self.computed = 0;
        compute_root_layout(self, root.into(), size.map(AvailableSpace::Definite));
    }
}

impl TraversePartialTree for Tree {
    type ChildIter<'a> = std::iter::Map<std::slice::Iter<'a, usize>, fn(&usize) -> NodeId>;

    fn child_ids(&self, node: NodeId) -> Self::ChildIter<'_> {
        self.nodes[usize::from(node)]
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|id| (*id).into())
    }

    fn child_count(&self, node: NodeId) -> usize {
        self.nodes[usize::from(node)]
            .as_ref()
            .unwrap()
            .children
            .len()
    }

    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        self.nodes[usize::from(node)].as_ref().unwrap().children[index].into()
    }
}

impl TraverseTree for Tree {}

impl LayoutPartialTree for Tree {
    type CustomIdent = String;
    type CoreContainerStyle<'a> = &'a Style;

    fn get_core_container_style(&self, node: NodeId) -> &Style {
        &self.nodes[usize::from(node)]
            .as_ref()
            .unwrap()
            .element
            .style
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        let slot = usize::from(node);
        let old = self.nodes[slot].as_mut().unwrap();
        if old.layout.size != layout.size || old.layout.location != layout.location {
            self.changed.push(slot);
        }
        old.layout = *layout;
    }

    fn resolve_calc_value(&self, _: *const (), _: f32) -> f32 {
        0.0
    }

    fn compute_child_layout(&mut self, node: NodeId, input: LayoutInput) -> LayoutOutput {
        compute_cached_layout(self, node, input, |tree, node, input| {
            tree.computed += 1;
            if input.run_mode == RunMode::PerformHiddenLayout {
                return compute_hidden_layout(tree, node);
            }
            let slot = usize::from(node);
            if tree.nodes[slot].as_ref().unwrap().children.is_empty()
                && !matches!(
                    tree.nodes[slot].as_ref().unwrap().element.algorithm,
                    Algorithm::Custom(_)
                )
            {
                return compute_leaf_layout(
                    input,
                    &tree.nodes[slot].as_ref().unwrap().element.style,
                    |_, _| 0.0,
                    |_, _| Size::ZERO,
                );
            }
            match tree.nodes[slot].as_ref().unwrap().element.algorithm {
                Algorithm::Flex => compute_flexbox_layout(tree, node, input),
                Algorithm::Grid => compute_grid_layout(tree, node, input),
                Algorithm::Custom(layout) => {
                    let size = layout(&mut LayoutContext {
                        tree,
                        node: slot,
                        input,
                    });
                    LayoutOutput::from_sizes(size, size)
                }
            }
        })
    }
}

impl CacheTree for Tree {
    fn cache_get(&self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.nodes[usize::from(node)]
            .as_ref()
            .unwrap()
            .cache
            .get(input)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.nodes[usize::from(node)]
            .as_mut()
            .unwrap()
            .cache
            .store(input, output);
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.nodes[usize::from(node)]
            .as_mut()
            .unwrap()
            .cache
            .clear();
    }
}

impl LayoutFlexboxContainer for Tree {
    type FlexboxContainerStyle<'a> = &'a Style;
    type FlexboxItemStyle<'a> = &'a Style;

    fn get_flexbox_container_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }

    fn get_flexbox_child_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
}

impl LayoutGridContainer for Tree {
    type GridContainerStyle<'a> = &'a Style;
    type GridItemStyle<'a> = &'a Style;

    fn get_grid_container_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }

    fn get_grid_child_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
}
