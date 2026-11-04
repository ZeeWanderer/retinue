use super::layout_protocol::{
    Available, LayoutAlgorithm, LayoutCache, LayoutError, LayoutFailure, LayoutSize, Proposal,
};
use super::{Element, Key};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use taffy::{
    AvailableSpace, Cache, CacheTree, Layout, LayoutFlexboxContainer, LayoutGridContainer,
    LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, Point, RunMode, Size, Style,
    TraversePartialTree, TraverseTree, compute_cached_layout, compute_flexbox_layout,
    compute_grid_layout, compute_hidden_layout, compute_leaf_layout, compute_root_layout,
    util::ResolveOrZero,
};

#[derive(Clone, Default)]
pub enum Algorithm {
    #[default]
    Flex,
    Grid,
    Custom(LayoutAlgorithm),
}

impl PartialEq for Algorithm {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flex, Self::Flex) | (Self::Grid, Self::Grid) => true,
            (Self::Custom(a), Self::Custom(b)) => a == b,
            _ => false,
        }
    }
}

pub(crate) struct LayoutContext<'a> {
    tree: &'a mut Tree,
    node: usize,
    input: LayoutInput,
    parent_size: Size<Option<f32>>,
    placed: Option<Vec<bool>>,
}

pub(crate) enum Subtree {
    Retained(Key),
    Replace(Box<Element>, Vec<Subtree>),
}

impl Subtree {
    pub fn key(&self) -> Key {
        match self {
            Self::Retained(key) => *key,
            Self::Replace(element, _) => element.key,
        }
    }
}

impl From<Element> for Subtree {
    fn from(mut element: Element) -> Self {
        let children = std::mem::take(&mut element.children)
            .into_iter()
            .map(Self::from)
            .collect();
        Self::Replace(Box::new(element), children)
    }
}

impl LayoutContext<'_> {
    pub(crate) fn invalidate_projection(&mut self) {
        self.tree.changed.push(self.node);
    }

    pub fn child_count(&self) -> usize {
        self.tree.nodes[self.node].as_ref().unwrap().children.len()
    }

    pub fn child_key(&self, child: usize) -> Option<Key> {
        let slot = *self.tree.nodes[self.node].as_ref()?.children.get(child)?;
        let element = &self.tree.nodes[slot].as_ref()?.element;
        Some(element.layout_key.unwrap_or(element.key))
    }

    pub fn child_values(&self, child: usize) -> Result<&super::values::LayoutValues, LayoutError> {
        let slot = *self.tree.nodes[self.node]
            .as_ref()
            .unwrap()
            .children
            .get(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        Ok(&self.tree.nodes[slot]
            .as_ref()
            .unwrap()
            .element
            .layout_values)
    }

    pub fn child_alignment(
        &mut self,
        child: usize,
        guide: super::alignment::AlignmentGuide,
        proposal: Proposal,
    ) -> Result<f32, LayoutError> {
        let size = self.measure_child(child, proposal)?;
        let slot = self.tree.nodes[self.node].as_ref().unwrap().children[child];
        let node = self.tree.nodes[slot].as_mut().unwrap();
        let result = if let Some(value) = guide.explicit(&node.element.layout_values, size) {
            Some(value)
        } else if let Algorithm::Custom(algorithm) = node.element.algorithm.clone() {
            let key = node.element.key;
            let mut cache = algorithm.cache(node.custom_cache.take());
            let parent_width = match proposal.width {
                Available::Limit(width) => Some(width),
                _ => None,
            };
            let insets = node
                .element
                .style
                .padding
                .resolve_or_zero(parent_width, &|_, _| 0.0)
                + node
                    .element
                    .style
                    .border
                    .resolve_or_zero(parent_width, &|_, _| 0.0);
            let bounds = super::Bounds::new(
                insets.left,
                insets.top,
                (size.width - insets.left - insets.right).max(0.0),
                (size.height - insets.top - insets.bottom).max(0.0),
            );
            let mut context = LayoutContext {
                tree: self.tree,
                node: slot,
                input: LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    ..self.input
                },
                parent_size: Size {
                    width: Some(bounds.width),
                    height: Some(bounds.height),
                },
                placed: None,
            };
            let result = algorithm.refresh(&mut cache, &mut context).and_then(|()| {
                let value =
                    algorithm.explicit_alignment(guide, bounds, &mut context, &mut cache)?;
                if value.is_some_and(|value| !value.is_finite()) {
                    Err(LayoutError::InvalidGeometry)
                } else {
                    Ok(value)
                }
            });
            context.tree.nodes[slot].as_mut().unwrap().custom_cache = Some(cache);
            match result {
                Ok(value) => value,
                Err(error) => {
                    context.tree.failure.get_or_insert(LayoutFailure {
                        container: key,
                        error,
                    });
                    return Err(LayoutError::ChildFailed(key));
                }
            }
        } else {
            None
        };
        let value = result.unwrap_or_else(|| guide.fallback(size));
        if value.is_finite() {
            Ok(value)
        } else {
            Err(LayoutError::InvalidGeometry)
        }
    }

    pub fn measure_child(
        &mut self,
        child: usize,
        proposal: Proposal,
    ) -> Result<LayoutSize, LayoutError> {
        let key = self
            .child_key(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        let available = Size {
            width: space(proposal.width)?,
            height: space(proposal.height)?,
        };
        let id = self.tree.get_child_id(self.node.into(), child);
        let size = self
            .tree
            .compute_child_layout(
                id,
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: taffy::SizingMode::InherentSize,
                    axis: taffy::RequestedAxis::Both,
                    known_dimensions: Size::NONE,
                    parent_size: available.map(|space| match space {
                        AvailableSpace::Definite(value) => Some(value),
                        _ => None,
                    }),
                    available_space: available,
                    ..self.input
                },
            )
            .size;
        if self.tree.failure.is_some() {
            return Err(LayoutError::ChildFailed(key));
        }
        Ok(LayoutSize::new(size.width, size.height))
    }

    pub fn place_child(&mut self, child: usize, bounds: super::Bounds) -> Result<(), LayoutError> {
        let key = self
            .child_key(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        if self.placed.is_none() {
            return Err(LayoutError::PlacementDuringMeasurement);
        }
        if !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !LayoutSize::new(bounds.width, bounds.height).valid()
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let size = Size {
            width: bounds.width,
            height: bounds.height,
        };
        let id = self.tree.get_child_id(self.node.into(), child);
        let output = self.tree.compute_child_layout(
            id,
            LayoutInput {
                sizing_mode: taffy::SizingMode::InherentSize,
                axis: taffy::RequestedAxis::Both,
                known_dimensions: size.map(Some),
                parent_size: self.parent_size,
                available_space: size.map(AvailableSpace::Definite),
                ..self.input
            },
        );
        if self.tree.failure.is_some() {
            return Err(LayoutError::ChildFailed(key));
        }
        let child_style = &self.tree.nodes[usize::from(id)]
            .as_ref()
            .unwrap()
            .element
            .style;
        let padding = child_style
            .padding
            .resolve_or_zero(self.parent_size.width, &|_, _| 0.0);
        let border = child_style
            .border
            .resolve_or_zero(self.parent_size.width, &|_, _| 0.0);
        self.tree.set_unrounded_layout(
            id,
            &Layout {
                order: child as u32,
                location: Point {
                    x: bounds.x,
                    y: bounds.y,
                },
                size: output.size,
                padding,
                border,
                ..Layout::default()
            },
        );
        self.placed.as_mut().unwrap()[child] = true;
        Ok(())
    }
}

fn space(value: Available) -> Result<AvailableSpace, LayoutError> {
    match value {
        Available::MinContent => Ok(AvailableSpace::MinContent),
        Available::Ideal => Ok(AvailableSpace::MaxContent),
        Available::Limit(value) if value.is_finite() && value >= 0.0 => {
            Ok(AvailableSpace::Definite(value))
        }
        _ => Err(LayoutError::InvalidGeometry),
    }
}

fn available(space: AvailableSpace) -> Available {
    match space {
        AvailableSpace::MinContent => Available::MinContent,
        AvailableSpace::MaxContent => Available::Ideal,
        AvailableSpace::Definite(value) => Available::Limit(value.max(0.0)),
    }
}

pub(super) struct Node {
    pub element: Element,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub layout: Layout,
    cache: Cache,
    pub(super) custom_cache: Option<LayoutCache>,
    pub recorded: Option<super::PaintProgram>,
}

#[derive(Default)]
pub(super) struct Tree {
    pub nodes: Vec<Option<Node>>,
    keys: HashMap<Key, usize>,
    free: Vec<usize>,
    pub computed: usize,
    pub changed: Vec<usize>,
    pub topology_changed: bool,
    pub failure: Option<LayoutFailure>,
}

impl Tree {
    pub fn slot(&self, key: Key) -> Option<usize> {
        self.keys.get(&key).copied()
    }

    pub fn reconcile(&mut self, root: Element) -> Result<usize, &'static str> {
        fn unique(element: &Element, seen: &mut HashSet<Key>) -> bool {
            seen.insert(element.key) && element.children.iter().all(|child| unique(child, seen))
        }
        let mut seen = HashSet::default();
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

    pub fn replace_many(&mut self, roots: Vec<Subtree>) -> Result<(), &'static str> {
        #[derive(Default)]
        struct Claimed {
            keys: HashSet<Key>,
            retained: HashSet<usize>,
            ancestors: HashSet<usize>,
        }
        fn validate(tree: &Tree, patch: &Subtree, owner: usize, claimed: &mut Claimed) -> bool {
            if !claimed.keys.insert(patch.key()) {
                return false;
            }
            let retained = matches!(patch, Subtree::Retained(_));
            if let Some(slot) = tree.slot(patch.key()) {
                if retained && claimed.ancestors.contains(&slot) {
                    return false;
                }
                let mut current = Some(slot);
                let mut in_scope = false;
                while let Some(index) = current {
                    if claimed.retained.contains(&index) {
                        return false;
                    }
                    in_scope |= index == owner;
                    claimed.ancestors.insert(index);
                    current = tree.nodes[index].as_ref().unwrap().parent;
                }
                if !in_scope {
                    return false;
                }
                if retained {
                    claimed.retained.insert(slot);
                }
            } else if retained {
                return false;
            }
            match patch {
                Subtree::Retained(_) => true,
                Subtree::Replace(_, children) => children
                    .iter()
                    .all(|child| validate(tree, child, owner, claimed)),
            }
        }
        fn obsolete(tree: &Tree, slot: usize, claimed: &Claimed, removed: &mut Vec<usize>) {
            if claimed.retained.contains(&slot) {
                return;
            }
            let node = tree.nodes[slot].as_ref().unwrap();
            if !claimed.keys.contains(&node.element.key) {
                removed.push(slot);
            }
            for &child in &node.children {
                obsolete(tree, child, claimed, removed);
            }
        }
        let mut prepared = Vec::with_capacity(roots.len());
        let mut scopes = HashSet::default();
        for root in roots {
            let slot = self.slot(root.key()).ok_or("unknown composition key")?;
            if !scopes.insert(slot) {
                return Err("overlapping composition subtrees");
            }
            prepared.push((root, slot));
        }
        let mut claimed = Claimed::default();
        for (root, slot) in &prepared {
            let mut parent = self.nodes[*slot].as_ref().unwrap().parent;
            while let Some(index) = parent {
                if scopes.contains(&index) {
                    return Err("overlapping composition subtrees");
                }
                parent = self.nodes[index].as_ref().unwrap().parent;
            }
            if !validate(self, root, *slot, &mut claimed) {
                return Err("duplicate or out-of-scope composition key");
            }
        }
        let mut removed = Vec::new();
        for (_, slot) in &prepared {
            obsolete(self, *slot, &claimed, &mut removed);
        }
        self.changed.clear();
        self.topology_changed = false;
        for (root, slot) in prepared {
            let parent = self.nodes[slot].as_ref().unwrap().parent;
            self.insert_subtree(root, parent);
        }
        for slot in removed {
            let node = self.nodes[slot].take().unwrap();
            self.keys.remove(&node.element.key);
            self.free.push(slot);
        }
        Ok(())
    }

    fn insert(&mut self, mut element: Element, parent: Option<usize>) -> usize {
        let children = std::mem::take(&mut element.children)
            .into_iter()
            .map(Subtree::from)
            .collect();
        self.insert_node(element, children, parent)
    }

    fn insert_subtree(&mut self, patch: Subtree, parent: Option<usize>) -> usize {
        match patch {
            Subtree::Retained(key) => {
                let slot = self.keys[&key];
                let node = self.nodes[slot].as_mut().unwrap();
                if node.parent != parent {
                    node.parent = parent;
                    self.topology_changed = true;
                    self.changed.push(slot);
                    self.invalidate(slot);
                }
                slot
            }
            Subtree::Replace(element, children) => self.insert_node(*element, children, parent),
        }
    }

    fn insert_node(
        &mut self,
        element: Element,
        descriptions: Vec<Subtree>,
        parent: Option<usize>,
    ) -> usize {
        let slot = *self.keys.entry(element.key).or_insert_with(|| {
            self.free.pop().unwrap_or_else(|| {
                self.nodes.push(None);
                self.nodes.len() - 1
            })
        });
        let changed = self.nodes[slot].as_ref().is_none_or(|old| {
            old.element.style != element.style || old.element.algorithm != element.algorithm
        });
        let values_changed = self.nodes[slot].as_ref().is_none_or(|old| {
            old.element.layout_values != element.layout_values
                || old.element.layout_key != element.layout_key
        });
        let projected = self.nodes[slot].as_ref().is_none_or(|old| {
            changed
                || old.element.offset != element.offset
                || old.element.clip != element.clip
                || old.element.interactive != element.interactive
                || old.element.hit_testing != element.hit_testing
                || old.element.role != element.role
                || old.element.window_action != element.window_action
                || old
                    .element
                    .paint
                    .as_ref()
                    .map(|paint| (paint.key, paint.cache))
                    != element.paint.as_ref().map(|paint| (paint.key, paint.cache))
        });
        let topology_changed = self.nodes[slot].as_ref().is_none_or(|old| {
            old.parent != parent
                || old.element.cache_hint != element.cache_hint
                || (old.element.style.display == taffy::Display::None)
                    != (element.style.display == taffy::Display::None)
        });
        self.topology_changed |= topology_changed;
        if projected || topology_changed {
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
                custom_cache: None,
                recorded: None,
            });
        }
        let children: Vec<_> = descriptions
            .into_iter()
            .map(|child| self.insert_subtree(child, Some(slot)))
            .collect();
        let node = self.nodes[slot].as_mut().unwrap();
        let reordered = node.children != children;
        self.topology_changed |= reordered;
        if reordered {
            self.changed.push(slot);
        }
        let changed = changed || reordered;
        node.children = children;
        if changed {
            self.invalidate(slot);
        } else if values_changed {
            if let Some(parent) = parent {
                self.invalidate(parent);
            }
        }
        slot
    }

    pub(super) fn invalidate(&mut self, mut slot: usize) {
        loop {
            let node = self.nodes[slot].as_mut().unwrap();
            node.cache.clear();
            if matches!(node.element.algorithm, Algorithm::Custom(_)) {
                if let Some(cache) = &mut node.custom_cache {
                    cache.dirty = true;
                }
            } else {
                node.custom_cache = None;
            }
            let Some(parent) = node.parent else { break };
            slot = parent;
        }
    }

    pub fn compute(&mut self, root: usize, size: Size<f32>) -> Result<(), &'static str> {
        self.computed = 0;
        self.failure = None;
        compute_root_layout(self, root.into(), size.map(AvailableSpace::Definite));
        if self.failure.is_some() {
            // Failed computations must not become valid cached layout on retry.
            for node in self.nodes.iter_mut().flatten() {
                node.cache.clear();
                if let Some(cache) = &mut node.custom_cache {
                    cache.dirty = true;
                }
            }
            Err("custom layout failed")
        } else {
            Ok(())
        }
    }

    fn custom(&mut self, node: usize, input: LayoutInput, layout: LayoutAlgorithm) -> LayoutOutput {
        let entry = self.nodes[node].as_mut().unwrap();
        let key = entry.element.key;
        let style = entry.element.style.clone();
        let mut cache = layout.cache(entry.custom_cache.take());
        let mut context = LayoutContext {
            tree: self,
            node,
            input,
            parent_size: input.parent_size,
            placed: None,
        };
        if let Err(error) = layout.refresh(&mut cache, &mut context) {
            context.tree.failure.get_or_insert(LayoutFailure {
                container: key,
                error,
            });
            context.tree.nodes[node].as_mut().unwrap().custom_cache = Some(cache);
            return LayoutOutput::DEFAULT;
        }
        let output = compute_leaf_layout(
            input,
            &style,
            |_, _| 0.0,
            |_, space| match layout.measure(
                Proposal {
                    width: available(space.width),
                    height: available(space.height),
                },
                &mut context,
                &mut cache,
            ) {
                Ok(size) if size.valid() => Size {
                    width: size.width,
                    height: size.height,
                },
                result => {
                    context.tree.failure.get_or_insert(LayoutFailure {
                        container: key,
                        error: result.err().unwrap_or(LayoutError::InvalidGeometry),
                    });
                    Size::ZERO
                }
            },
        );
        if input.run_mode == RunMode::PerformLayout && context.tree.failure.is_none() {
            let insets = style
                .padding
                .resolve_or_zero(input.parent_size.width, &|_, _| 0.0)
                + style
                    .border
                    .resolve_or_zero(input.parent_size.width, &|_, _| 0.0);
            let bounds = super::Bounds::new(
                insets.left,
                insets.top,
                (output.size.width - insets.left - insets.right).max(0.0),
                (output.size.height - insets.top - insets.bottom).max(0.0),
            );
            context.parent_size = Size {
                width: Some(bounds.width),
                height: Some(bounds.height),
            };
            context.placed = Some(vec![false; context.child_count()]);
            if let Err(error) = layout.place(bounds, &mut context, &mut cache) {
                context.tree.failure.get_or_insert(LayoutFailure {
                    container: key,
                    error,
                });
            } else if let Some(index) = context
                .placed
                .as_ref()
                .unwrap()
                .iter()
                .position(|placed| !placed)
            {
                let child = context.child_key(index).unwrap();
                context.tree.failure.get_or_insert(LayoutFailure {
                    container: key,
                    error: LayoutError::UnplacedChild(child),
                });
            }
        }
        context.tree.nodes[node].as_mut().unwrap().custom_cache = Some(cache);
        output
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
            match tree.nodes[slot].as_ref().unwrap().element.algorithm.clone() {
                Algorithm::Flex => compute_flexbox_layout(tree, node, input),
                Algorithm::Grid => compute_grid_layout(tree, node, input),
                Algorithm::Custom(layout) => tree.custom(slot, input, layout),
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
