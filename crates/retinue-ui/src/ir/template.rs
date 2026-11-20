use super::edits::{Edits, Input};
use super::{Kind, Local, Result, Structure, Work};
use crate::{
    Key,
    input::InputOptions,
    layout::values::LayoutValues,
    scene::{self, Algorithm, Paint},
    semantics::{Role, WindowAction},
};
use retinue_render::CacheHint;
use std::{rc::Rc, sync::Arc};

mod schedule;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Layout,
    Paint,
    Content,
    Offset,
    HitTesting,
    LayoutValues,
    Clip,
    Interactive,
    Role,
    WindowAction,
    Reuse,
    Input,
    Name,
}

impl Property {
    fn writes(self) -> u16 {
        match self {
            Self::Layout => 1,
            Self::Paint => 2,
            Self::Content => 3,
            Self::Offset => 4,
            Self::HitTesting => 8,
            Self::LayoutValues => 16,
            Self::Clip => 32,
            Self::Interactive => 64,
            Self::Role => 128,
            Self::WindowAction => 256,
            Self::Reuse => 512,
            Self::Input => 1024,
            Self::Name => 2048,
        }
    }
}

pub enum Value {
    Layout(taffy::Style, Algorithm),
    Paint(Option<Paint>),
    Content(Recipe),
    Offset((f32, f32)),
    HitTesting(bool),
    LayoutValues(LayoutValues),
    Placement(Option<crate::Bounds>),
    Clip(bool),
    Interactive(bool),
    Role(Role),
    WindowAction(Option<WindowAction>),
    Reuse(CacheHint),
    Name(Option<Key>),
    Input {
        enabled: bool,
        options: Option<InputOptions>,
    },
}

impl Value {
    fn property(&self) -> Property {
        match self {
            Self::Layout(..) => Property::Layout,
            Self::Paint(..) => Property::Paint,
            Self::Content(..) => Property::Content,
            Self::Offset(..) => Property::Offset,
            Self::HitTesting(..) => Property::HitTesting,
            Self::LayoutValues(..) | Self::Placement(..) => Property::LayoutValues,
            Self::Clip(..) => Property::Clip,
            Self::Interactive(..) => Property::Interactive,
            Self::Role(..) => Property::Role,
            Self::WindowAction(..) => Property::WindowAction,
            Self::Reuse(..) => Property::Reuse,
            Self::Input { .. } => Property::Input,
            Self::Name(..) => Property::Name,
        }
    }

    fn changes(self, key: Key, out: &mut Edits) {
        let change = match self {
            Self::Layout(style, algorithm) => scene::Change::Layout(key, style, algorithm),
            Self::Paint(paint) => scene::Change::Paint(key, paint),
            Self::Content(recipe) => {
                out.scene
                    .push(scene::Change::Layout(key, recipe.style, recipe.algorithm));
                out.scene.push(scene::Change::Paint(key, recipe.paint));
                return;
            }
            Self::Offset(offset) => scene::Change::Offset(key, offset),
            Self::HitTesting(enabled) => scene::Change::HitTesting(key, enabled),
            Self::LayoutValues(values) => scene::Change::LayoutValues(key, values),
            Self::Placement(bounds) => scene::Change::Placement(key, bounds),
            Self::Clip(clip) => scene::Change::Clip(key, clip),
            Self::Interactive(interactive) => scene::Change::Interactive(key, interactive),
            Self::Role(role) => scene::Change::Role(key, role),
            Self::WindowAction(action) => scene::Change::WindowAction(key, action),
            Self::Reuse(hint) => scene::Change::Reuse(key, hint),
            Self::Name(name) => {
                out.names.push((key, name));
                scene::Change::LayoutKey(key, name)
            }
            Self::Input { enabled, options } => {
                out.inputs
                    .push((key, options.map(|options| Input { enabled, options })));
                return;
            }
        };
        out.scene.push(change);
    }

    fn initialize(self, element: &mut scene::Element) {
        match self {
            Self::Layout(style, algorithm) => {
                element.style = style;
                element.algorithm = algorithm;
            }
            Self::Paint(paint) => element.paint = paint,
            Self::Content(recipe) => {
                element.style = recipe.style;
                element.algorithm = recipe.algorithm;
                element.paint = recipe.paint;
            }
            Self::Offset(offset) => element.offset = offset,
            Self::HitTesting(enabled) => element.hit_testing = enabled,
            Self::LayoutValues(values) => element.layout_values = values,
            Self::Placement(bounds) => element
                .layout_values
                .replace_single::<crate::layout::positioned::Placement>(bounds),
            Self::Clip(clip) => element.clip = clip,
            Self::Interactive(interactive) => element.interactive = interactive,
            Self::Role(role) => element.role = role,
            Self::WindowAction(action) => element.window_action = action,
            Self::Reuse(hint) => element.cache_hint = hint,
            Self::Name(name) => element.layout_key = name,
            Self::Input { .. } => unreachable!("input belongs to the semantic owner"),
        }
    }
}

pub struct Binding<P> {
    pub node: Local,
    pub property: Property,
    pub evaluate: Rc<dyn Fn(&P, Key, u64) -> Value>,
}

impl<P> Binding<P> {
    fn value(&self, input: &P, key: Key, revision: u64) -> Result<Value> {
        let value = (self.evaluate)(input, key, revision);
        if value.property() != self.property {
            return Err("binding emitted a different property");
        }
        Ok(value)
    }
}

pub(crate) struct SourceGroup<P> {
    pub range: std::ops::Range<usize>,
    pub unchanged: fn(&P, &P) -> bool,
}

pub struct Source<P> {
    /// Compare the input fields read by this source's bindings.
    pub equal: fn(&P, &P) -> bool,
    pub bindings: Box<[usize]>,
}

pub struct Recipe {
    pub style: taffy::Style,
    pub algorithm: Algorithm,
    pub paint: Option<Paint>,
}

pub struct Attributes {
    pub name: Option<Key>,
    pub layout_values: LayoutValues,
    pub offset: (f32, f32),
    pub clip: bool,
    pub interactive: bool,
    pub hit_testing: bool,
    pub role: Role,
    pub window_action: Option<WindowAction>,
    pub cache_hint: CacheHint,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            name: None,
            layout_values: LayoutValues::default(),
            offset: (0.0, 0.0),
            clip: false,
            interactive: false,
            hit_testing: true,
            role: Role::None,
            window_action: None,
            cache_hint: CacheHint::Dynamic,
        }
    }
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            style: taffy::Style::default(),
            algorithm: Algorithm::Flex,
            paint: None,
        }
    }
}

/// Static structure and typed binding code, shared by instance boundaries.
pub struct Template<P> {
    pub(super) structure: Arc<Structure>,
    pub(super) outlets: Box<[Local]>,
    recipes: Box<[Recipe]>,
    sources: Box<[Source<P>]>,
    groups: Box<[SourceGroup<P>]>,
    bindings: Box<[Binding<P>]>,
    attributes: Box<[(Local, Attributes)]>,
    inputs_equal: Option<fn(&P, &P) -> bool>,
}

pub(super) fn node_key(base: u64, local: Local) -> Key {
    Key::new("ui.ir.node", base + local.0 as u64)
}

impl<P> Template<P> {
    pub fn compile(
        structure: Arc<Structure>,
        recipes: Vec<Recipe>,
        sources: Vec<Source<P>>,
        bindings: Vec<Binding<P>>,
    ) -> Result<Self> {
        if recipes.len() != structure.nodes.len() {
            return Err("recipe count differs from topology");
        }
        for (node, recipe) in structure.nodes.iter().zip(&recipes) {
            if !node.kind.materialized()
                && (recipe.style != taffy::Style::default()
                    || recipe.algorithm != Algorithm::default()
                    || recipe.paint.is_some())
            {
                return Err("transparent nodes cannot own layout or paint");
            }
        }
        let mut targets = Vec::new();
        for binding in &bindings {
            if binding.node.0 as usize >= structure.nodes.len() {
                return Err("binding target is outside template");
            }
            if !structure.nodes[binding.node.0 as usize].kind.materialized()
                && binding.property != Property::Input
            {
                return Err("property binding targets a transparent node");
            }
            if targets.iter().any(|&(node, property): &(Local, Property)| {
                node == binding.node && property.writes() & binding.property.writes() != 0
            }) {
                return Err("bindings write overlapping properties");
            }
            targets.push((binding.node, binding.property));
        }
        let mut used = vec![false; bindings.len()];
        for source in &sources {
            for &index in &source.bindings {
                *used
                    .get_mut(index)
                    .ok_or("source refers to unknown binding")? = true;
            }
        }
        if used.iter().any(|used| !used) {
            return Err("dynamic binding has no source");
        }
        Ok(Self {
            outlets: structure
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| node.kind.dynamic().then_some(Local(index as u32)))
                .collect(),
            structure,
            recipes: recipes.into_boxed_slice(),
            sources: sources.into_boxed_slice(),
            groups: Box::default(),
            bindings: bindings.into_boxed_slice(),
            attributes: Box::default(),
            inputs_equal: None,
        })
    }

    /// Reject unchanged inputs before per-source dispatch. Equality must cover all
    /// owner inputs, not just rendered sources; normally use `PartialEq::eq`.
    /// Pending edits are compared against the latest staged value.
    pub fn with_input_equality(mut self, equal: fn(&P, &P) -> bool) -> Self {
        self.inputs_equal = Some(equal);
        self
    }

    pub(super) fn same_input(&self, before: &P, after: &P) -> bool {
        self.inputs_equal.is_some_and(|equal| equal(before, after))
    }

    pub fn with_attributes(mut self, mut attributes: Vec<(Local, Attributes)>) -> Result<Self> {
        attributes.sort_unstable_by_key(|(local, _)| local.0);
        if attributes
            .iter()
            .any(|(local, _)| local.0 as usize >= self.structure.nodes.len())
        {
            return Err("attributes target outside template");
        }
        if attributes.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate node attributes");
        }
        if attributes
            .iter()
            .any(|(local, _)| !self.structure.nodes[local.0 as usize].kind.materialized())
        {
            return Err("attributes target a transparent node");
        }
        self.attributes = attributes.into_boxed_slice();
        Ok(self)
    }

    pub(super) fn compare(&self, before: &P, after: &P) -> Work {
        let mut work = Work::default();
        for source in self
            .sources
            .iter()
            .filter(|source| !source.bindings.is_empty())
        {
            work.sources_compared += 1;
            if !(source.equal)(before, after) {
                work.instances_changed = 1;
                return work;
            }
        }
        work
    }

    pub(super) fn diff(
        &self,
        before: &P,
        after: &P,
        base: u64,
        revision: u64,
        out: &mut Edits,
        affected: &mut Vec<usize>,
    ) -> Result<Work> {
        let mut work = Work::default();
        affected.clear();
        let mut compare = |sources: &[Source<P>]| {
            for source in sources {
                if !(source.equal)(before, after) {
                    affected.extend_from_slice(&source.bindings);
                }
            }
            sources.len()
        };
        let mut cursor = 0;
        for group in &self.groups {
            work.sources_compared += 1 + compare(&self.sources[cursor..group.range.start]);
            if !(group.unchanged)(before, after) {
                work.sources_compared += compare(&self.sources[group.range.clone()]);
            }
            cursor = group.range.end;
        }
        work.sources_compared += compare(&self.sources[cursor..]);
        affected.sort_unstable();
        affected.dedup();
        for &index in affected.iter() {
            let binding = &self.bindings[index];
            let key = node_key(base, binding.node);
            binding.value(after, key, revision)?.changes(key, out);
            work.bindings_emitted += 1;
        }
        work.instances_changed = usize::from(work.bindings_emitted != 0);
        Ok(work)
    }

    fn elements(
        &self,
        input: &P,
        base: u64,
        revision: u64,
        initialize_inputs: bool,
        edits: &mut Edits,
    ) -> Result<Vec<Option<scene::Element>>> {
        let mut elements: Vec<_> = self
            .recipes
            .iter()
            .enumerate()
            .map(|(index, recipe)| {
                if !self.structure.nodes[index].kind.materialized() {
                    return None;
                }
                let mut element =
                    scene::Element::new(node_key(base, Local(index as u32)), recipe.style.clone());
                element.algorithm = recipe.algorithm.clone();
                element.paint = recipe.paint.clone();
                Some(element)
            })
            .collect();
        for (local, attributes) in &self.attributes {
            let element = elements[local.0 as usize].as_mut().unwrap();
            element.layout_key = attributes.name;
            edits.names.push((element.key, attributes.name));
            element.layout_values = attributes.layout_values.clone();
            element.offset = attributes.offset;
            element.clip = attributes.clip;
            element.interactive = attributes.interactive;
            element.hit_testing = attributes.hit_testing;
            element.role = attributes.role;
            element.window_action = attributes.window_action;
            element.cache_hint = attributes.cache_hint;
        }
        for binding in &self.bindings {
            if binding.property == Property::Input {
                if initialize_inputs {
                    let key = node_key(base, binding.node);
                    binding.value(input, key, revision)?.changes(key, edits);
                }
                continue;
            }
            let element = elements[binding.node.0 as usize].as_mut().unwrap();
            let value = binding.value(input, element.key, revision)?;
            if let Value::Name(name) = &value {
                edits.names.push((element.key, *name));
            }
            value.initialize(element);
        }
        Ok(elements)
    }

    #[cfg(test)]
    pub(super) fn mount(&self, input: &P, base: u64, revision: u64) -> Result<scene::Element> {
        fn element(root: scene::layout::Subtree) -> scene::Element {
            let scene::layout::Subtree::Replace(mut root, children) = root else {
                unreachable!("fresh mount retained an existing node")
            };
            root.children = children.into_iter().map(element).collect();
            *root
        }
        let mut roots = Vec::new();
        self.project(
            input,
            base,
            revision,
            Vec::new(),
            true,
            &mut roots,
            &mut Edits::default(),
        )?;
        if roots.len() != 1 {
            return Err("single-element mount requires exactly one layout root");
        }
        Ok(element(roots.remove(0)))
    }

    pub(super) fn project(
        &self,
        input: &P,
        base: u64,
        revision: u64,
        outlets: Vec<(Local, Vec<scene::layout::Subtree>)>,
        initialize_inputs: bool,
        out: &mut Vec<scene::layout::Subtree>,
        edits: &mut Edits,
    ) -> Result<()> {
        use scene::layout::Subtree;
        let mut dynamic = rustc_hash::FxHashMap::default();
        for (local, children) in outlets {
            if self
                .structure
                .nodes
                .get(local.0 as usize)
                .is_none_or(|node| !node.kind.dynamic())
            {
                return Err("dynamic children require an outlet");
            }
            if dynamic.insert(local.0, children).is_some() {
                return Err("duplicate outlet update");
            }
        }
        let mut elements = if initialize_inputs {
            self.elements(input, base, revision, true, edits)?
        } else {
            Vec::new()
        };
        let mut lowered: Vec<Option<Subtree>> =
            (0..self.structure.nodes.len()).map(|_| None).collect();
        for index in (0..self.structure.nodes.len()).rev() {
            if !self.structure.nodes[index].kind.materialized() {
                continue;
            }
            let children = if self.structure.nodes[index].kind == Kind::DynamicLayout {
                dynamic.remove(&(index as u32)).unwrap_or_default()
            } else {
                let parts = self.structure.layout_children(Local(index as u32));
                let mut children = Vec::with_capacity(parts.len());
                self.append_children(parts, &mut lowered, &mut dynamic, &mut children);
                children
            };
            lowered[index] = Some(if initialize_inputs {
                Subtree::Replace(Box::new(elements[index].take().unwrap()), children)
            } else {
                Subtree::Children(node_key(base, Local(index as u32)), children)
            });
        }
        self.append_children(self.structure.roots(), &mut lowered, &mut dynamic, out);
        Ok(())
    }

    fn append_children(
        &self,
        parts: &[Local],
        lowered: &mut [Option<scene::layout::Subtree>],
        dynamic: &mut rustc_hash::FxHashMap<u32, Vec<scene::layout::Subtree>>,
        out: &mut Vec<scene::layout::Subtree>,
    ) {
        for &part in parts {
            if self.structure.nodes[part.0 as usize].kind == Kind::Slot {
                out.extend(dynamic.remove(&part.0).unwrap_or_default());
            } else {
                out.push(lowered[part.0 as usize].take().unwrap());
            }
        }
    }
}
