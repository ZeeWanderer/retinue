#[cfg(test)]
mod batch_tests;
mod invalidation;
#[cfg(test)]
mod invalidation_tests;
mod job;
pub(crate) mod solver;
#[cfg(test)]
mod storage_tests;
mod style;

pub use job::LayoutExecution;
pub(super) use job::Pending;
pub(crate) use solver::LayoutContext;

use super::layout_protocol::{LayoutAlgorithm, LayoutCache, LayoutFailure, SharedLayout};
use super::work::{Changes, Work};
use super::{Element, Key, Paint, PaintProgram, Role, WindowAction, values::LayoutValues};
use retinue_render::CacheHint;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use style::NumericStyle;
use taffy::{Cache, Layout, Size, Style};

#[derive(Clone, Default, PartialEq)]
pub enum Algorithm {
    #[default]
    Flex,
    Grid,
    Custom(LayoutAlgorithm),
}

impl Algorithm {
    fn split(self) -> (Kind, Option<SharedLayout>, Option<LayoutAlgorithm>) {
        match self {
            Self::Flex => (Kind::Flex, None, None),
            Self::Grid => (Kind::Grid, None, None),
            Self::Custom(layout) => match layout.split() {
                Ok(layout) => (Kind::Shared, Some(layout), None),
                Err(layout) => (Kind::Owner, None, Some(layout)),
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Flex,
    Grid,
    Shared,
    Owner,
}

// All three stores share slot identities; the solver borrows inputs read-only.
pub(super) struct Input {
    pub key: Key,
    pub layout_key: Option<Key>,
    pub style: NumericStyle,
    kind: Kind,
    custom: Option<SharedLayout>,
    pub records_layout: bool,
    pub layout_values: LayoutValues,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}

pub(super) struct Owner {
    pub custom: Option<LayoutAlgorithm>,
    pub custom_cache: Option<LayoutCache>,
    pub paint: Option<Paint>,
    pub recorded: Option<PaintProgram>,
    pub recorded_layout: u64,
    pub offset: (f32, f32),
    pub clip: bool,
    pub interactive: bool,
    pub hit_testing: bool,
    pub role: Role,
    pub window_action: Option<WindowAction>,
    pub cache_hint: CacheHint,
}

pub(super) struct Geometry {
    pub layout: Layout,
    pub revision: u64,
    cache: Cache,
    shared_cache: Option<LayoutCache<dyn std::any::Any + Send>>,
    dependencies: invalidation::Dependencies,
    reset_child_queries: bool,
    validating: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            revision: 0,
            cache: Cache::new(),
            shared_cache: None,
            dependencies: Default::default(),
            reset_child_queries: false,
            validating: false,
        }
    }
}

impl Geometry {
    fn invalidate(&mut self) {
        self.cache.clear();
        self.reset_child_queries = true;
        if let Some(cache) = &mut self.shared_cache {
            cache.dirty = true;
        }
    }

    pub fn result<'a>(&'a self, owner: Option<&'a LayoutCache>) -> crate::layout::LayoutResult<'a> {
        if let Some(cache) = &self.shared_cache {
            cache.result()
        } else {
            owner.map(|cache| cache.result()).unwrap_or_default()
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    #[cfg(feature = "layout-profiling")]
    batch_cpu: crate::profiling::layout::BatchCpu,
    pub nodes: Vec<Option<Geometry>>,
    pub computed: usize,
    pub changed: Changes,
    pub failure: Option<LayoutFailure>,
    invalidated: Vec<usize>,
    ancestor_marks: Vec<u64>,
    full_layout: bool,
    pending: std::collections::BinaryHeap<(usize, usize)>,
}

impl State {
    fn invalidate_caches(&mut self) {
        self.full_layout = true;
        for node in self.nodes.iter_mut().flatten() {
            node.invalidate();
            node.validating = false;
        }
    }
}

pub(crate) enum Subtree {
    Retained(Key),
    Children(Key, Vec<Subtree>),
    Replace(Box<Element>, Vec<Subtree>),
}

impl Subtree {
    pub fn key(&self) -> Key {
        match self {
            Self::Retained(key) | Self::Children(key, _) => *key,
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

pub(super) struct Replacement {
    roots: Vec<(Subtree, usize)>,
    removed: Vec<usize>,
    inserted: HashSet<Key>,
}

impl Replacement {
    pub fn contains(&self, tree: &Tree, key: Key) -> bool {
        self.inserted.contains(&key)
            || tree
                .slot(key)
                .is_some_and(|slot| self.removed.binary_search(&slot).is_err())
    }
}

#[derive(Default)]
pub(super) struct Tree {
    pub inputs: Vec<Option<Input>>,
    pub nodes: Vec<Option<Owner>>,
    pub state: State,
    keys: HashMap<Key, usize>,
    free: Vec<usize>,
    pub topology_changed: bool,
}

impl Tree {
    pub(super) fn supports_style(style: &Style) -> bool {
        NumericStyle::supports(style)
    }

    pub fn slot(&self, key: Key) -> Option<usize> {
        self.keys.get(&key).copied()
    }

    pub fn reconcile(&mut self, root: Element) -> Result<usize, &'static str> {
        fn validate(element: &Element, seen: &mut HashSet<Key>) -> Result<(), &'static str> {
            if !seen.insert(element.key) {
                return Err("duplicate composition key");
            }
            if !Tree::supports_style(&element.style) {
                return Err("unsupported layout calc handle");
            }
            for child in &element.children {
                validate(child, seen)?;
            }
            Ok(())
        }
        let mut seen = HashSet::default();
        validate(&root, &mut seen)?;
        self.state.changed.clear();
        self.topology_changed = false;
        let root = self.insert(root, None);
        self.keys.retain(|key, slot| {
            if seen.contains(key) {
                true
            } else {
                self.inputs[*slot] = None;
                self.nodes[*slot] = None;
                self.state.nodes[*slot] = None;
                self.free.push(*slot);
                false
            }
        });
        Ok(root)
    }

    pub fn replace_many(&mut self, roots: Vec<Subtree>) -> Result<(), &'static str> {
        let replacement = self.prepare_replacement(roots)?;
        self.apply_replacement(replacement);
        Ok(())
    }

    pub fn prepare_replacement(&self, roots: Vec<Subtree>) -> Result<Replacement, &'static str> {
        #[derive(Default)]
        struct Claimed {
            keys: HashSet<Key>,
            retained: HashSet<usize>,
            ancestors: HashSet<usize>,
        }
        fn validate(
            tree: &Tree,
            patch: &Subtree,
            owner: usize,
            claimed: &mut Claimed,
        ) -> Result<(), &'static str> {
            if !claimed.keys.insert(patch.key()) {
                return Err("duplicate or out-of-scope composition key");
            }
            let retained = matches!(patch, Subtree::Retained(_));
            if let Some(slot) = tree.slot(patch.key()) {
                if retained && claimed.ancestors.contains(&slot) {
                    return Err("duplicate or out-of-scope composition key");
                }
                let mut current = Some(slot);
                let mut in_scope = false;
                while let Some(index) = current {
                    if claimed.retained.contains(&index) {
                        return Err("duplicate or out-of-scope composition key");
                    }
                    in_scope |= index == owner;
                    claimed.ancestors.insert(index);
                    current = tree.inputs[index].as_ref().unwrap().parent;
                }
                if !in_scope {
                    return Err("duplicate or out-of-scope composition key");
                }
                if retained {
                    claimed.retained.insert(slot);
                }
            } else if !matches!(patch, Subtree::Replace(..)) {
                return Err("duplicate or out-of-scope composition key");
            }
            match patch {
                Subtree::Retained(_) => Ok(()),
                Subtree::Children(_, children) => children
                    .iter()
                    .try_for_each(|child| validate(tree, child, owner, claimed)),
                Subtree::Replace(element, children) => {
                    if !Tree::supports_style(&element.style) {
                        return Err("unsupported layout calc handle");
                    }
                    if !element.offset.0.is_finite() || !element.offset.1.is_finite() {
                        return Err("composition offset must be finite");
                    }
                    children
                        .iter()
                        .try_for_each(|child| validate(tree, child, owner, claimed))
                }
            }
        }
        fn obsolete(tree: &Tree, slot: usize, claimed: &Claimed, removed: &mut Vec<usize>) {
            if claimed.retained.contains(&slot) {
                return;
            }
            let node = tree.inputs[slot].as_ref().unwrap();
            if !claimed.keys.contains(&node.key) {
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
            let mut parent = self.inputs[*slot].as_ref().unwrap().parent;
            while let Some(index) = parent {
                if scopes.contains(&index) {
                    return Err("overlapping composition subtrees");
                }
                parent = self.inputs[index].as_ref().unwrap().parent;
            }
            validate(self, root, *slot, &mut claimed)?;
        }
        let mut removed = Vec::new();
        for (_, slot) in &prepared {
            obsolete(self, *slot, &claimed, &mut removed);
        }
        removed.sort_unstable();
        Ok(Replacement {
            roots: prepared,
            removed,
            inserted: claimed.keys,
        })
    }

    pub fn apply_replacement(&mut self, replacement: Replacement) {
        self.state.changed.clear();
        self.topology_changed = false;
        for (root, slot) in replacement.roots {
            let parent = self.inputs[slot].as_ref().unwrap().parent;
            self.insert_subtree(root, parent);
        }
        for slot in replacement.removed {
            let input = self.inputs[slot].take().unwrap();
            self.nodes[slot] = None;
            self.state.nodes[slot] = None;
            self.keys.remove(&input.key);
            self.free.push(slot);
        }
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
            Subtree::Retained(key) => self.reparent(key, parent),
            Subtree::Children(key, children) => {
                let slot = self.reparent(key, parent);
                self.replace_children(slot, children);
                slot
            }
            Subtree::Replace(element, children) => self.insert_node(*element, children, parent),
        }
    }

    fn reparent(&mut self, key: Key, parent: Option<usize>) -> usize {
        let slot = self.keys[&key];
        let node = self.inputs[slot].as_mut().unwrap();
        if node.parent != parent {
            node.parent = parent;
            self.topology_changed = true;
            self.state.changed.mark(slot, Work::STRUCTURE);
            self.invalidate(slot);
        }
        slot
    }

    fn insert_node(
        &mut self,
        element: Element,
        descriptions: Vec<Subtree>,
        parent: Option<usize>,
    ) -> usize {
        let slot = *self.keys.entry(element.key).or_insert_with(|| {
            self.free.pop().unwrap_or_else(|| {
                self.inputs.push(None);
                self.nodes.push(None);
                self.state.nodes.push(None);
                self.nodes.len() - 1
            })
        });
        self.replace_element(slot, element, parent);
        self.replace_children(slot, descriptions);
        slot
    }

    fn replace_children(&mut self, slot: usize, descriptions: Vec<Subtree>) {
        let children: Vec<_> = descriptions
            .into_iter()
            .map(|child| self.insert_subtree(child, Some(slot)))
            .collect();
        let node = self.inputs[slot].as_mut().unwrap();
        if node.children != children {
            self.topology_changed = true;
            self.state
                .changed
                .mark(slot, Work::STRUCTURE | Work::LAYOUT);
            node.children = children;
            self.invalidate(slot);
        }
    }

    pub(super) fn matches_layout(&self, slot: usize, style: &Style, algorithm: &Algorithm) -> bool {
        let input = self.inputs[slot].as_ref().unwrap();
        let owner = self.nodes[slot].as_ref().unwrap();
        *input.style == *style
            && match (input.kind, algorithm) {
                (Kind::Flex, Algorithm::Flex) | (Kind::Grid, Algorithm::Grid) => true,
                (Kind::Shared, Algorithm::Custom(layout)) => {
                    input.custom.as_ref() == layout.shared()
                }
                (Kind::Owner, Algorithm::Custom(layout)) => owner.custom.as_ref() == Some(layout),
                _ => false,
            }
    }

    pub(super) fn set_layout(&mut self, slot: usize, style: Style, algorithm: Algorithm) {
        let input = self.inputs[slot].as_mut().unwrap();
        let style_changed = *input.style != style;
        let visibility_changed = (input.style.display == taffy::Display::None)
            != (style.display == taffy::Display::None);
        self.topology_changed |= visibility_changed;
        let (kind, shared, custom) = algorithm.split();
        input.style = NumericStyle::new(style);
        input.kind = kind;
        input.custom = shared;
        self.nodes[slot].as_mut().unwrap().custom = custom;
        if visibility_changed {
            self.invalidate(slot);
        } else {
            self.invalidate_layout(slot, style_changed);
        }
    }

    pub(super) fn replace_element(
        &mut self,
        slot: usize,
        element: Element,
        parent: Option<usize>,
    ) -> bool {
        let style_changed = self.inputs[slot]
            .as_ref()
            .is_none_or(|old| *old.style != element.style);
        let structure_changed = self.inputs[slot].as_ref().is_none_or(|old| {
            old.parent != parent
                || (old.style.display == taffy::Display::None)
                    != (element.style.display == taffy::Display::None)
        });
        let changed = self.inputs[slot].is_none()
            || !self.matches_layout(slot, &element.style, &element.algorithm);
        let values_changed = self.inputs[slot].as_ref().is_none_or(|old| {
            old.layout_values != element.layout_values || old.layout_key != element.layout_key
        });
        let mut work = Work::default();
        if changed || values_changed {
            work |= Work::LAYOUT;
        }
        if let Some(old) = &self.nodes[slot] {
            if old.offset != element.offset || old.clip != element.clip {
                work |= Work::GEOMETRY;
            }
            if old.interactive != element.interactive
                || old.hit_testing != element.hit_testing
                || old.role != element.role
                || old.window_action != element.window_action
            {
                work |= Work::INPUT;
            }
            if old.paint != element.paint {
                work |= Work::PAINT;
            }
        } else {
            work = Work::ALL;
        }
        let topology_changed = self.inputs[slot].as_ref().is_none_or(|old| {
            old.parent != parent
                || self.nodes[slot].as_ref().unwrap().cache_hint != element.cache_hint
                || (old.style.display == taffy::Display::None)
                    != (element.style.display == taffy::Display::None)
        });
        self.topology_changed |= topology_changed;
        if topology_changed {
            work |= Work::STRUCTURE;
        }
        self.state.changed.mark(slot, work);
        let records_layout = element
            .paint
            .as_ref()
            .is_some_and(|paint| paint.uses_layout);
        let (kind, shared, custom) = element.algorithm.split();
        if let Some(input) = &mut self.inputs[slot] {
            if changed {
                input.style = NumericStyle::new(element.style);
            }
            input.kind = kind;
            input.custom = shared;
            input.records_layout = records_layout;
            input.layout_values = element.layout_values;
            input.layout_key = element.layout_key;
            input.parent = parent;
        } else {
            self.inputs[slot] = Some(Input {
                key: element.key,
                layout_key: element.layout_key,
                style: NumericStyle::new(element.style),
                kind,
                custom: shared,
                records_layout,
                layout_values: element.layout_values,
                children: Vec::new(),
                parent,
            });
            self.state.nodes[slot] = Some(Geometry::default());
        }
        let old = self.nodes[slot].take();
        let (custom_cache, recorded, recorded_layout) = old.map_or((None, None, 0), |old| {
            (old.custom_cache, old.recorded, old.recorded_layout)
        });
        self.nodes[slot] = Some(Owner {
            custom,
            custom_cache,
            paint: element.paint,
            recorded,
            recorded_layout,
            offset: element.offset,
            clip: element.clip,
            interactive: element.interactive,
            hit_testing: element.hit_testing,
            role: element.role,
            window_action: element.window_action,
            cache_hint: element.cache_hint,
        });
        if changed {
            if structure_changed {
                self.invalidate(slot);
            } else {
                self.invalidate_layout(slot, style_changed);
            }
        }
        if values_changed && let Some(parent) = parent {
            self.invalidate_content(parent);
        }
        changed || values_changed && parent.is_some()
    }

    pub(super) fn invalidate(&mut self, mut slot: usize) {
        self.state.full_layout = true;
        loop {
            self.invalidate_node(slot);
            let Some(parent) = self.inputs[slot].as_ref().unwrap().parent else {
                break;
            };
            slot = parent;
        }
    }

    pub(super) fn invalidate_arranger(&mut self, slot: usize) -> bool {
        if let Some(parent) = self.inputs[slot].as_ref().unwrap().parent {
            self.invalidate_content(parent);
            true
        } else {
            false
        }
    }

    pub(super) fn invalidate_content(&mut self, slot: usize) {
        self.invalidate_node(slot);
        self.state.invalidated.push(slot);
    }

    fn invalidate_layout(&mut self, slot: usize, style_changed: bool) {
        self.invalidate_content(slot);
        // Flex/grid and custom parents can read child style independently of its size.
        if style_changed && let Some(parent) = self.inputs[slot].as_ref().unwrap().parent {
            self.invalidate_content(parent);
        }
    }

    fn invalidate_node(&mut self, slot: usize) {
        let geometry = self.state.nodes[slot].as_mut().unwrap();
        geometry.invalidate();
        if self.inputs[slot].as_ref().unwrap().custom.is_none() {
            geometry.shared_cache = None;
        }
        let node = self.nodes[slot].as_mut().unwrap();
        if node.custom.is_none() {
            node.custom_cache = None;
        } else if let Some(cache) = &mut node.custom_cache {
            cache.dirty = true;
        }
    }

    pub fn compute(&mut self, root: usize, size: Size<f32>) -> Result<(), &'static str> {
        solver::Computation::new(&self.inputs, &mut self.state, Some(&mut self.nodes))
            .compute(root, size)
    }

    pub(super) fn render_context(&self, slot: usize) -> retinue_render::OptimizationContext {
        self.state.nodes[slot]
            .as_ref()
            .unwrap()
            .shared_cache
            .as_ref()
            .map(|cache| &cache.rendering)
            .or_else(|| {
                self.nodes[slot]
                    .as_ref()
                    .unwrap()
                    .custom_cache
                    .as_ref()
                    .map(|cache| &cache.rendering)
            })
            .cloned()
            .unwrap_or_default()
    }
}
