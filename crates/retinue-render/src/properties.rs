use crate::{Bounds, Key, Rect, resources::ResourceId};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::collections::VecDeque;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransformId(pub Key);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipId(pub Key);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Translation {
    pub id: TransformId,
    pub parent: Option<TransformId>,
    pub offset: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub id: ClipId,
    pub parent: Option<ClipId>,
    pub transform: Option<TransformId>,
    pub bounds: Bounds,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyDefinitions {
    pub translations: Vec<Translation>,
    pub clips: Vec<Clip>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropertyBinding {
    pub transform: Option<TransformId>,
    pub clip: Option<ClipId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropertyChange {
    Translation(TransformId, (f32, f32)),
    ClipBounds(ClipId, Bounds),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyRevision {
    pub identity: ResourceId,
    pub version: u64,
}

impl Default for PropertyRevision {
    fn default() -> Self {
        Self {
            identity: ResourceId::next(),
            version: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PropertyStats {
    pub translations: usize,
    pub clips: usize,
    pub instances: usize,
}

#[derive(Clone)]
struct TransformNode {
    value: Translation,
    logical: (f32, f32),
    pixels: (i32, i32),
    clips: Vec<usize>,
    users: Vec<usize>,
    revision: PropertyRevision,
}

#[derive(Clone)]
struct ClipNode {
    value: Clip,
    pixels: Rect,
    users: Vec<usize>,
    revision: PropertyRevision,
}

#[derive(Clone, Default)]
struct Tree<K> {
    index: HashMap<K, usize>,
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    order: Vec<usize>,
    rank: Vec<usize>,
}

impl<K: Copy + Eq + Hash> Tree<K> {
    fn build(
        keys: impl Iterator<Item = K>,
        parents: impl Iterator<Item = Option<K>>,
    ) -> Result<Self, &'static str> {
        let mut index = HashMap::default();
        for key in keys {
            let next = index.len();
            if index.insert(key, next).is_some() {
                return Err("duplicate property key");
            }
        }
        let mut children = vec![Vec::new(); index.len()];
        let parents: Vec<_> = parents
            .enumerate()
            .map(|(child, key)| {
                let parent = key
                    .map(|key| index.get(&key).copied().ok_or("unknown property parent"))
                    .transpose()?;
                if let Some(parent) = parent {
                    children[parent].push(child);
                }
                Ok(parent)
            })
            .collect::<Result<_, &'static str>>()?;
        let mut queue: VecDeque<_> = parents
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.is_none().then_some(i))
            .collect();
        let mut order = Vec::with_capacity(index.len());
        while let Some(index) = queue.pop_front() {
            order.push(index);
            queue.extend(children[index].iter().copied());
        }
        if order.len() != index.len() {
            return Err("property dependency cycle");
        }
        let mut rank = vec![0; order.len()];
        for (position, index) in order.iter().enumerate() {
            rank[*index] = position;
        }
        Ok(Self {
            index,
            parents,
            children,
            order,
            rank,
        })
    }

    fn affected(&self, roots: impl Iterator<Item = usize>) -> Vec<usize> {
        let mut pending: Vec<_> = roots.collect();
        let mut seen = HashSet::default();
        while let Some(index) = pending.pop() {
            if seen.insert(index) {
                pending.extend(&self.children[index]);
            }
        }
        let mut result: Vec<_> = seen.into_iter().collect();
        result.sort_unstable_by_key(|i| self.rank[*i]);
        result
    }
}

#[derive(Clone)]
pub(crate) struct Properties {
    transforms: Vec<TransformNode>,
    clips: Vec<ClipNode>,
    transform_tree: Tree<TransformId>,
    clip_tree: Tree<ClipId>,
    scale: f32,
    viewport: Rect,
    pub stats: PropertyStats,
}

pub(crate) struct PropertyUpdate {
    pub changed: bool,
    pub instances: Vec<usize>,
}

impl Default for Properties {
    fn default() -> Self {
        Self::build(PropertyDefinitions::default(), (0, 0), 1.0).unwrap()
    }
}

impl Properties {
    pub fn same_values(&self, other: &Self) -> bool {
        self.viewport == other.viewport
            && self.scale == other.scale
            && self.transforms.len() == other.transforms.len()
            && self.clips.len() == other.clips.len()
            && self
                .transforms
                .iter()
                .zip(&other.transforms)
                .all(|(a, b)| a.value == b.value)
            && self
                .clips
                .iter()
                .zip(&other.clips)
                .all(|(a, b)| a.value == b.value)
    }

    pub fn clear_users(&mut self) {
        for node in &mut self.transforms {
            node.users.clear();
        }
        for node in &mut self.clips {
            node.users.clear();
        }
    }

    pub fn build(
        definitions: PropertyDefinitions,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Self, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("property scale must be positive and finite");
        }
        let transform_tree = Tree::build(
            definitions.translations.iter().map(|v| v.id),
            definitions.translations.iter().map(|v| v.parent),
        )?;
        let clip_tree = Tree::build(
            definitions.clips.iter().map(|v| v.id),
            definitions.clips.iter().map(|v| v.parent),
        )?;
        let viewport = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let mut transforms: Vec<_> = definitions
            .translations
            .into_iter()
            .map(|value| TransformNode {
                value,
                logical: (0.0, 0.0),
                pixels: (0, 0),
                clips: Vec::new(),
                users: Vec::new(),
                revision: Default::default(),
            })
            .collect();
        for &index in &transform_tree.order {
            let parent =
                transform_tree.parents[index].map_or((0.0, 0.0), |p| transforms[p].logical);
            let (logical, pixels) = translate(parent, transforms[index].value.offset, scale)?;
            transforms[index].logical = logical;
            transforms[index].pixels = pixels;
        }
        let mut clips: Vec<_> = definitions
            .clips
            .into_iter()
            .map(|value| ClipNode {
                value,
                pixels: Rect::default(),
                users: Vec::new(),
                revision: Default::default(),
            })
            .collect();
        for &index in &clip_tree.order {
            let value = clips[index].value;
            let transform = value
                .transform
                .map(|key| {
                    transform_tree
                        .index
                        .get(&key)
                        .copied()
                        .ok_or("unknown clip transform")
                })
                .transpose()?;
            if let Some(transform) = transform {
                transforms[transform].clips.push(index);
            }
            let origin = transform.map_or((0.0, 0.0), |t| transforms[t].logical);
            let parent = clip_tree.parents[index].map_or(viewport, |p| clips[p].pixels);
            clips[index].pixels = clip(value.bounds, origin, scale, parent)?;
        }
        Ok(Self {
            transforms,
            clips,
            transform_tree,
            clip_tree,
            scale,
            viewport,
            stats: Default::default(),
        })
    }

    pub fn bind(&mut self, binding: PropertyBinding, instance: usize) -> Result<(), &'static str> {
        if let Some(key) = binding.transform {
            let index = *self
                .transform_tree
                .index
                .get(&key)
                .ok_or("unknown instance transform")?;
            self.transforms[index].users.push(instance);
        }
        if let Some(key) = binding.clip {
            let index = *self
                .clip_tree
                .index
                .get(&key)
                .ok_or("unknown instance clip")?;
            self.clips[index].users.push(instance);
        }
        Ok(())
    }

    pub fn resolve(&self, binding: PropertyBinding) -> ((i32, i32), Rect) {
        (
            binding.transform.map_or((0, 0), |key| {
                self.transforms[self.transform_tree.index[&key]].pixels
            }),
            binding.clip.map_or(self.viewport, |key| {
                self.clips[self.clip_tree.index[&key]].pixels
            }),
        )
    }

    pub fn translation_revision(&self, key: TransformId) -> Option<PropertyRevision> {
        self.transform_tree
            .index
            .get(&key)
            .map(|&i| self.transforms[i].revision)
    }

    pub fn clip_revision(&self, key: ClipId) -> Option<PropertyRevision> {
        self.clip_tree
            .index
            .get(&key)
            .map(|&i| self.clips[i].revision)
    }

    pub fn patch(
        &mut self,
        changes: impl IntoIterator<Item = PropertyChange>,
    ) -> Result<PropertyUpdate, &'static str> {
        let mut translations = HashMap::default();
        let mut bounds = HashMap::default();
        for change in changes {
            match change {
                PropertyChange::Translation(key, value) => {
                    let index = *self
                        .transform_tree
                        .index
                        .get(&key)
                        .ok_or("unknown transform property")?;
                    if !value.0.is_finite() || !value.1.is_finite() {
                        return Err("translation must be finite");
                    }
                    translations.insert(index, value);
                }
                PropertyChange::ClipBounds(key, value) => {
                    validate_bounds(value)?;
                    let index = *self
                        .clip_tree
                        .index
                        .get(&key)
                        .ok_or("unknown clip property")?;
                    bounds.insert(index, value);
                }
            }
        }
        translations.retain(|i, value| self.transforms[*i].value.offset != *value);
        bounds.retain(|i, value| self.clips[*i].value.bounds != *value);
        let transforms = self.transform_tree.affected(translations.keys().copied());
        let mut resolved = HashMap::default();
        for &index in &transforms {
            let parent = self.transform_tree.parents[index].map_or((0.0, 0.0), |p| {
                resolved
                    .get(&p)
                    .map_or(self.transforms[p].logical, |&(logical, _)| logical)
            });
            let offset = translations
                .get(&index)
                .copied()
                .unwrap_or(self.transforms[index].value.offset);
            resolved.insert(index, translate(parent, offset, self.scale)?);
        }
        let clips = self.clip_tree.affected(
            bounds.keys().copied().chain(
                transforms
                    .iter()
                    .flat_map(|&i| self.transforms[i].clips.iter().copied()),
            ),
        );
        let mut clipped = HashMap::default();
        for &index in &clips {
            let value = self.clips[index].value;
            let origin = value.transform.map_or((0.0, 0.0), |key| {
                let t = self.transform_tree.index[&key];
                resolved
                    .get(&t)
                    .map_or(self.transforms[t].logical, |&(logical, _)| logical)
            });
            let parent = self.clip_tree.parents[index].map_or(self.viewport, |p| {
                clipped.get(&p).copied().unwrap_or(self.clips[p].pixels)
            });
            clipped.insert(
                index,
                clip(
                    bounds.get(&index).copied().unwrap_or(value.bounds),
                    origin,
                    self.scale,
                    parent,
                )?,
            );
        }
        let mut instances = HashSet::default();
        self.stats = PropertyStats {
            translations: transforms.len(),
            clips: clips.len(),
            instances: 0,
        };
        for index in transforms {
            let node = &mut self.transforms[index];
            let (logical, pixels) = resolved[&index];
            if node.pixels != pixels {
                instances.extend(node.users.iter().copied());
            }
            if node.logical != logical || translations.contains_key(&index) {
                node.revision.version += 1;
            }
            node.logical = logical;
            node.pixels = pixels;
            if let Some(offset) = translations.get(&index) {
                node.value.offset = *offset;
            }
        }
        for index in clips {
            let node = &mut self.clips[index];
            let pixels = clipped[&index];
            if node.pixels != pixels {
                instances.extend(node.users.iter().copied());
            }
            if node.pixels != pixels || bounds.contains_key(&index) {
                node.revision.version += 1;
            }
            node.pixels = pixels;
            if let Some(bounds) = bounds.get(&index) {
                node.value.bounds = *bounds;
            }
        }
        let mut instances: Vec<_> = instances.into_iter().collect();
        instances.sort_unstable();
        self.stats.instances = instances.len();
        Ok(PropertyUpdate {
            changed: !translations.is_empty() || !bounds.is_empty(),
            instances,
        })
    }
}

fn translate(
    parent: (f32, f32),
    offset: (f32, f32),
    scale: f32,
) -> Result<((f32, f32), (i32, i32)), &'static str> {
    let value = (parent.0 + offset.0, parent.1 + offset.1);
    if !value.0.is_finite()
        || !value.1.is_finite()
        || !(value.0 * scale).is_finite()
        || !(value.1 * scale).is_finite()
    {
        return Err("translation must remain finite after composition");
    }
    if [value.0 * scale, value.1 * scale].iter().any(|v| {
        f64::from(v.round()) < f64::from(i32::MIN) || f64::from(v.round()) > f64::from(i32::MAX)
    }) {
        return Err("translation is outside the pixel coordinate range");
    }
    Ok((
        value,
        (
            (value.0 * scale).round() as i32,
            (value.1 * scale).round() as i32,
        ),
    ))
}

fn validate_bounds(value: Bounds) -> Result<(), &'static str> {
    if ![value.x, value.y, value.width, value.height]
        .iter()
        .all(|v| v.is_finite())
        || value.width < 0.0
        || value.height < 0.0
    {
        return Err("clip bounds must be finite and nonnegative in size");
    }
    Ok(())
}

fn clip(
    bounds: Bounds,
    origin: (f32, f32),
    scale: f32,
    parent: Rect,
) -> Result<Rect, &'static str> {
    validate_bounds(bounds)?;
    let (origin, _) = translate(origin, (bounds.x, bounds.y), scale)?;
    if !(bounds.width * scale).is_finite() || !(bounds.height * scale).is_finite() {
        return Err("clip size must remain finite after scaling");
    }
    Ok(Bounds::new(origin.0, origin.1, bounds.width, bounds.height)
        .pixels(scale)
        .visible((0, 0), parent))
}
