//! UI composition, layout, motion and recording into the render pipeline.

#[cfg(test)]
mod batch_tests;
pub(crate) mod layout;
#[cfg(test)]
mod layout_tests;
mod loading;
mod motion;
mod paint;
mod projection;
mod rendering;
mod window;

pub use layout::Algorithm;
pub use loading::LoadingFrame;
pub use motion::SelectionMotion;
pub use paint::Paint;

use crate::{Bounds, Key, Rect};
use crate::{
    layout as layout_protocol,
    layout::{alignment, values},
    semantics,
};
use retinue_render::{damage::Damage, *};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use semantics::{Role, WindowAction};
use std::rc::Rc;
use taffy::{FlexDirection, Size, Style};

#[derive(Clone)]
pub struct Element {
    pub key: Key,
    pub layout_key: Option<Key>,
    pub style: Style,
    pub algorithm: Algorithm,
    pub layout_values: values::LayoutValues,
    pub paint: Option<Paint>,
    pub children: Vec<Element>,
    pub offset: (f32, f32),
    pub clip: bool,
    pub interactive: bool,
    /// Gates this subtree's input without changing its paint.
    pub hit_testing: bool,
    pub role: Role,
    pub window_action: Option<WindowAction>,
    pub cache_hint: CacheHint,
}

impl Element {
    pub fn at(key: impl Into<Key>, bounds: Bounds) -> Self {
        use taffy::{Dimension, LengthPercentageAuto, Position};
        Self::new(
            key,
            Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(bounds.x),
                    top: LengthPercentageAuto::length(bounds.y),
                    ..taffy::Rect::auto()
                },
                size: Size {
                    width: Dimension::length(bounds.width),
                    height: Dimension::length(bounds.height),
                },
                flex_shrink: 0.0,
                ..Style::default()
            },
        )
    }

    pub fn new(key: impl Into<Key>, style: Style) -> Self {
        Self {
            key: key.into(),
            layout_key: None,
            style,
            algorithm: Algorithm::Flex,
            layout_values: values::LayoutValues::default(),
            paint: None,
            children: Vec::new(),
            offset: (0.0, 0.0),
            clip: false,
            interactive: false,
            hit_testing: true,
            role: Role::None,
            window_action: None,
            cache_hint: CacheHint::Dynamic,
        }
    }

    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    pub fn row(key: impl Into<Key>, gap: f32, children: Vec<Self>) -> Self {
        let mut element = Self::new(
            key,
            Style {
                gap: Size {
                    width: taffy::LengthPercentage::length(gap),
                    height: taffy::LengthPercentage::length(gap),
                },
                ..Style::default()
            },
        );
        element.children = children;
        element
    }

    pub fn column(key: impl Into<Key>, gap: f32, children: Vec<Self>) -> Self {
        let mut element = Self::row(key, gap, children);
        element.style.flex_direction = FlexDirection::Column;
        element
    }

    pub fn grid(key: impl Into<Key>, mut style: Style, children: Vec<Self>) -> Self {
        style.display = taffy::Display::Grid;
        let mut element = Self::new(key, style);
        element.algorithm = Algorithm::Grid;
        element.children = children;
        element
    }

    pub fn stack(key: impl Into<Key>, mut style: Style, mut children: Vec<Self>) -> Self {
        use taffy::style_helpers::{fr, line};
        style.grid_template_columns = vec![fr(1.0_f32)];
        style.grid_template_rows = vec![fr(1.0_f32)];
        for child in &mut children {
            child.style.grid_column = line(1);
            child.style.grid_row = line(1);
        }
        Self::grid(key, style, children)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateStats {
    pub nodes: usize,
    pub layout_computations: usize,
    pub recordings: usize,
    pub visible_chunks: usize,
}

#[cfg(feature = "profiling")]
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateProfile {
    pub reconcile: std::time::Duration,
    pub layout: std::time::Duration,
    pub projection: std::time::Duration,
    pub geometry: std::time::Duration,
    pub frame: std::time::Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub key: Key,
    pub parent: Option<Key>,
    pub layout: Bounds,
    /// Logical clipping, independent of raster edge rounding.
    pub clip: Bounds,
    pub hit: Rect,
    pub paint: Rect,
    pub interactive: bool,
    pub role: Role,
    pub window_action: Option<WindowAction>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RecordingMode {
    #[default]
    Retained,
    Always,
}

pub enum Change {
    Offset(Key, (f32, f32)),
    Paint(Key, Option<Paint>),
    Layout(Key, Style, Algorithm),
    LayoutValues(Key, values::LayoutValues),
    Clip(Key, bool),
    Interactive(Key, bool),
    HitTesting(Key, bool),
    Role(Key, Role),
    WindowAction(Key, Option<WindowAction>),
    Reuse(Key, CacheHint),
}

#[derive(Default)]
pub struct Scene {
    frame: PaintFrame,
    programs: ProgramStore,
    tree: layout::Tree,
    root: Option<usize>,
    geometry: Vec<Geometry>,
    geometry_index: HashMap<Key, usize>,
    projection: Vec<projection::Placement>,
    pub stats: UpdateStats,
    #[cfg(feature = "profiling")]
    pub profile: UpdateProfile,
    command_revision: u64,
    input_revision: u64,
    layout_failed: bool,
}

struct FrameDraft {
    scale: f32,
    instances: Vec<PaintInstance>,
    groups: Vec<PaintGroup>,
}

impl Scene {
    pub fn frame(&self) -> &PaintFrame {
        &self.frame
    }

    pub fn record(
        &mut self,
        key: Key,
        commands: Vec<Command>,
        cache: CachePolicy,
    ) -> Result<Damage, &'static str> {
        self.record_many([(key, commands, cache)])
    }

    pub fn record_many(
        &mut self,
        nodes: impl IntoIterator<Item = (Key, Vec<Command>, CachePolicy)>,
    ) -> Result<Damage, &'static str> {
        let mut changes = Vec::new();
        let mut keys = HashSet::default();
        for (key, commands, cache) in nodes {
            if !keys.insert(key) {
                return Err("duplicate paint node");
            }
            let slot = self.tree.slot(key).ok_or("unknown paint node")?;
            let node = self.tree.nodes[slot].as_ref().unwrap();
            if node
                .recorded
                .as_ref()
                .is_some_and(|recorded| recorded.commands().as_ref() == &commands)
                && node
                    .element
                    .paint
                    .as_ref()
                    .is_some_and(|paint| paint.cache == cache)
            {
                continue;
            }
            self.command_revision += 1;
            let mut paint =
                Paint::commands(PaintKey::new(key, self.command_revision), Rc::new(commands));
            paint.cache = cache;
            changes.push(Change::Paint(key, Some(paint)));
        }
        self.patch(changes)
    }
    pub fn update(
        &mut self,
        root: Element,
        size: (u32, u32),
        scale: f32,
        mode: RecordingMode,
    ) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let root = self.tree.reconcile(root)?;
        #[cfg(feature = "profiling")]
        {
            self.profile.reconcile += started.elapsed();
        }
        self.root = Some(root);
        self.resolve(root, size, scale, mode)
    }

    pub fn resize(&mut self, size: (u32, u32), scale: f32) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let root = self.root.ok_or("composition has no root")?;
        self.tree.changed.clear();
        self.tree.topology_changed = false;
        self.resolve(root, size, scale, RecordingMode::Retained)
    }

    fn resolve(
        &mut self,
        root: usize,
        size: (u32, u32),
        scale: f32,
        mode: RecordingMode,
    ) -> Result<Damage, &'static str> {
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        if let Err(error) = self.tree.compute(
            root,
            Size {
                width: size.0 as f32 / scale,
                height: size.1 as f32 / scale,
            },
        ) {
            self.layout_failed = true;
            return Err(error);
        }
        #[cfg(feature = "profiling")]
        {
            self.profile.layout += started.elapsed();
        }
        self.tree.topology_changed |= std::mem::take(&mut self.layout_failed);
        Ok(self.project(mode, size, scale))
    }

    pub fn update_subtree(&mut self, root: Element) -> Result<Damage, &'static str> {
        self.update_subtrees([root])
    }

    pub fn update_subtrees(
        &mut self,
        roots: impl IntoIterator<Item = Element>,
    ) -> Result<Damage, &'static str> {
        self.update_subtrees_at(roots, self.frame.size(), self.frame.scale())
    }

    pub(crate) fn update_subtrees_at(
        &mut self,
        roots: impl IntoIterator<Item = Element>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Damage, &'static str> {
        self.update_retained_subtrees_at(
            roots.into_iter().map(layout::Subtree::from).collect(),
            size,
            scale,
        )
    }

    pub(crate) fn update_retained_subtrees_at(
        &mut self,
        roots: Vec<layout::Subtree>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let root = self.root.ok_or("composition has no root")?;
        if roots.is_empty() {
            return if self.frame.size() == size && self.frame.scale() == scale {
                Ok(Damage::default())
            } else {
                self.resize(size, scale)
            };
        }
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        self.tree.replace_many(roots)?;
        #[cfg(feature = "profiling")]
        {
            self.profile.reconcile += started.elapsed();
        }
        self.resolve(root, size, scale, RecordingMode::Retained)
    }

    pub fn contains(&self, key: Key) -> bool {
        self.tree.slot(key).is_some()
    }

    pub fn layout_failure(&self) -> Option<&layout_protocol::LayoutFailure> {
        self.tree.failure.as_ref()
    }

    fn rebuild_projection(&mut self, mode: RecordingMode, size: (u32, u32), scale: f32) -> Damage {
        self.input_revision = self.input_revision.wrapping_add(1);
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let mut draft = FrameDraft {
            scale,
            instances: Vec::new(),
            groups: Vec::new(),
        };
        self.geometry.clear();
        self.geometry_index.clear();
        self.projection.clear();
        self.collect(
            self.root.unwrap(),
            (0.0, 0.0),
            Rect {
                width: size.0,
                height: size.1,
                ..Rect::default()
            },
            Bounds::new(0.0, 0.0, size.0 as f32 / scale, size.1 as f32 / scale),
            mode,
            &mut draft,
        );
        #[cfg(feature = "profiling")]
        {
            self.profile.geometry += started.elapsed();
        }
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let damage = self
            .frame
            .replace(size, scale, draft.instances, draft.groups)
            .expect("valid scene frame");
        #[cfg(feature = "profiling")]
        {
            self.profile.frame += started.elapsed();
        }
        damage
    }

    fn collect(
        &mut self,
        slot: usize,
        parent: (f32, f32),
        clip: Rect,
        input_clip: Bounds,
        mode: RecordingMode,
        draft: &mut FrameDraft,
    ) -> Rect {
        self.stats.nodes += 1;
        let parent_key = self.tree.nodes[slot]
            .as_ref()
            .unwrap()
            .parent
            .map(|slot| self.tree.nodes[slot].as_ref().unwrap().element.key);
        let parent_index = parent_key.and_then(|key| self.geometry_index.get(&key).copied());
        let node = self.tree.nodes[slot].as_mut().unwrap();
        if node.element.style.display == taffy::Display::None {
            return Rect::default();
        }
        let origin = (
            parent.0 + node.layout.location.x + node.element.offset.0,
            parent.1 + node.layout.location.y + node.element.offset.1,
        );
        let bounds = Bounds::new(
            origin.0,
            origin.1,
            node.layout.size.width,
            node.layout.size.height,
        );
        let pixels = bounds.pixels(draft.scale);
        let input_clip = if node.element.clip {
            input_clip.intersection(bounds)
        } else {
            input_clip
        };
        let hit = projection::layout_pixels(bounds, draft.scale).visible((0, 0), clip);
        let clip = if node.element.clip {
            clip.intersection(hit)
        } else {
            clip
        };
        let mut paint_bounds = Rect::default();
        let mut chunk_index = None;
        let group_start = draft.instances.len();
        if let Some(paint) = &node.element.paint {
            let size = (node.layout.size.width, node.layout.size.height);
            if mode == RecordingMode::Always
                || node
                    .recorded
                    .as_ref()
                    .is_none_or(|recorded| !recorded.matches(paint.key, size, draft.scale))
            {
                node.recorded =
                    Some(paint.compile(&mut self.programs, node.layout.size, draft.scale));
                self.stats.recordings += 1;
            }
            let recorded = node.recorded.as_ref().unwrap();
            paint_bounds = recorded.bounds().visible((pixels.x, pixels.y), clip);
            chunk_index = Some(draft.instances.len());
            let mut instance = PaintInstance::new(
                node.element.key,
                recorded.clone(),
                (pixels.x, pixels.y),
                clip,
            );
            instance.cache = paint.cache;
            draft.instances.push(instance);
        } else {
            node.recorded = None;
        }
        let geometry_index = self.geometry.len();
        let rendering = node
            .custom_cache
            .as_ref()
            .map(|cache| cache.rendering.clone())
            .unwrap_or_default();
        let group = (node.element.cache_hint != CacheHint::Dynamic
            || rendering != OptimizationContext::default())
        .then(|| {
            let index = draft.groups.len();
            draft.groups.push(PaintGroup {
                key: node.element.key,
                instances: group_start..group_start,
                context: retinue_render::OptimizationContext::default()
                    .with(node.element.cache_hint),
            });
            index
        });
        self.geometry_index.insert(node.element.key, geometry_index);
        let hit_enabled = node.element.hit_testing
            && parent_index.is_none_or(|index| self.projection[index].hit_enabled);
        self.geometry.push(Geometry {
            key: node.element.key,
            parent: parent_key,
            layout: bounds,
            clip: input_clip,
            hit,
            paint: paint_bounds,
            interactive: node.element.interactive && hit_enabled,
            role: node.element.role,
            window_action: node.element.window_action,
        });
        self.projection.push(projection::Placement {
            node: slot,
            parent: parent_index,
            end: geometry_index + 1,
            local: Bounds::new(
                node.layout.location.x,
                node.layout.location.y,
                node.layout.size.width,
                node.layout.size.height,
            ),
            offset: node.element.offset,
            clipper: node.element.clip,
            clip,
            own_paint: paint_bounds,
            chunk: chunk_index,
            group,
            instances: group_start..group_start,
            interactive: node.element.interactive,
            hit_testing: node.element.hit_testing,
            hit_enabled,
        });
        let child_count = node.children.len();
        for index in 0..child_count {
            let child = self.tree.nodes[slot].as_ref().unwrap().children[index];
            let child_paint = self.collect(child, origin, clip, input_clip, mode, draft);
            self.geometry[geometry_index].paint =
                self.geometry[geometry_index].paint.union(child_paint);
        }
        if let Some(group) = group {
            draft.groups[group].instances.end = draft.instances.len();
        }
        self.projection[geometry_index].end = self.geometry.len();
        self.projection[geometry_index].instances.end = draft.instances.len();
        if let Some(group) = group {
            draft.groups[group].context = self.render_context(geometry_index, draft.scale);
        }
        self.geometry[geometry_index].paint
    }

    pub fn patch(
        &mut self,
        changes: impl IntoIterator<Item = Change>,
    ) -> Result<Damage, &'static str> {
        self.patch_at(changes, self.frame.size(), self.frame.scale())
    }

    pub(crate) fn patch_at(
        &mut self,
        changes: impl IntoIterator<Item = Change>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let changes: Vec<_> = changes.into_iter().collect();
        let keys: Vec<_> = changes
            .iter()
            .map(|change| match change {
                Change::Offset(key, _)
                | Change::Paint(key, _)
                | Change::Layout(key, _, _)
                | Change::LayoutValues(key, _)
                | Change::Clip(key, _)
                | Change::Interactive(key, _)
                | Change::HitTesting(key, _)
                | Change::Role(key, _)
                | Change::WindowAction(key, _)
                | Change::Reuse(key, _) => *key,
            })
            .collect();
        if keys.iter().any(|key| self.tree.slot(*key).is_none()) {
            return Err("unknown composition key");
        }
        if changes.iter().any(
            |change| matches!(change,Change::Offset(_, (x,y)) if !x.is_finite() || !y.is_finite()),
        ) {
            return Err("composition offset must be finite");
        }
        self.tree.changed.clear();
        self.tree.topology_changed = false;
        self.tree.computed = 0;
        let mut relayout =
            self.layout_failed || self.frame.size() != size || self.frame.scale() != scale;
        for (key, change) in keys.into_iter().zip(changes) {
            let slot = self.tree.slot(key).unwrap();
            let node = self.tree.nodes[slot].as_ref().unwrap();
            let unchanged = match &change {
                Change::Offset(_, offset) => node.element.offset == *offset,
                Change::Layout(_, style, algorithm) => {
                    node.element.style == *style && node.element.algorithm == *algorithm
                }
                Change::LayoutValues(_, values) => node.element.layout_values == *values,
                Change::Clip(_, clip) => node.element.clip == *clip,
                Change::Interactive(_, hit) => node.element.interactive == *hit,
                Change::HitTesting(_, enabled) => node.element.hit_testing == *enabled,
                Change::Role(_, role) => node.element.role == *role,
                Change::WindowAction(_, action) => node.element.window_action == *action,
                Change::Reuse(_, hint) => node.element.cache_hint == *hint,
                Change::Paint(_, paint) => {
                    node.element
                        .paint
                        .as_ref()
                        .map(|paint| (paint.key, paint.cache))
                        == paint.as_ref().map(|paint| (paint.key, paint.cache))
                }
            };
            if unchanged {
                continue;
            }
            self.tree.changed.push(slot);
            let node = self.tree.nodes[slot].as_mut().unwrap();
            match change {
                Change::Offset(_, offset) => node.element.offset = offset,
                Change::Paint(_, paint) => node.element.paint = paint,
                Change::Layout(_, style, algorithm) => {
                    self.tree.topology_changed |= (node.element.style.display
                        == taffy::Display::None)
                        != (style.display == taffy::Display::None);
                    node.element.style = style;
                    node.element.algorithm = algorithm;
                    self.tree.invalidate(slot);
                    relayout = true;
                }
                Change::LayoutValues(_, values) => {
                    node.element.layout_values = values;
                    if let Some(parent) = node.parent {
                        self.tree.invalidate(parent);
                        relayout = true;
                    }
                }
                Change::Clip(_, clip) => node.element.clip = clip,
                Change::Interactive(_, hit) => node.element.interactive = hit,
                Change::HitTesting(_, enabled) => node.element.hit_testing = enabled,
                Change::Role(_, role) => node.element.role = role,
                Change::WindowAction(_, action) => node.element.window_action = action,
                Change::Reuse(_, hint) => {
                    node.element.cache_hint = hint;
                    self.tree.topology_changed = true;
                }
            }
        }
        if relayout {
            self.resolve(
                self.root.ok_or("composition has no root")?,
                size,
                scale,
                RecordingMode::Retained,
            )
        } else {
            Ok(self.project(RecordingMode::Retained, size, scale))
        }
    }

    pub fn geometry(&self) -> &[Geometry] {
        &self.geometry
    }

    pub(crate) fn input_revision(&self) -> u64 {
        self.input_revision
    }

    pub fn bounds(&self, key: Key) -> Option<Geometry> {
        self.geometry_index
            .get(&key)
            .map(|&index| self.geometry[index])
    }

    pub fn layout_rect(&self, key: Key) -> Option<Rect> {
        let pixels = projection::layout_pixels(self.bounds(key)?.layout, self.frame.scale());
        Some(Rect {
            x: pixels.x.max(0) as u32,
            y: pixels.y.max(0) as u32,
            width: pixels.width,
            height: pixels.height,
        })
    }

    pub fn content_rect(&self, key: Key) -> Option<Rect> {
        let bounds = self.layout_rect(key)?;
        let layout = &self.tree.nodes[self.tree.slot(key)?].as_ref()?.layout;
        let inset = |border: f32, padding: f32| {
            ((border + padding) * self.frame.scale()).round().max(0.0) as u32
        };
        let left = inset(layout.border.left, layout.padding.left);
        let top = inset(layout.border.top, layout.padding.top);
        let right = inset(layout.border.right, layout.padding.right);
        let bottom = inset(layout.border.bottom, layout.padding.bottom);
        Some(Rect {
            x: bounds.x + left.min(bounds.width),
            y: bounds.y + top.min(bounds.height),
            width: bounds.width.saturating_sub(left.saturating_add(right)),
            height: bounds.height.saturating_sub(top.saturating_add(bottom)),
        })
    }

    /// Preserve a paint binding when rebuilding only its layout description.
    pub fn paint(&self, key: Key) -> Option<&Paint> {
        self.tree.nodes[self.tree.slot(key)?]
            .as_ref()?
            .element
            .paint
            .as_ref()
    }

    pub fn invalidate(&self, key: Key, damage: Damage) -> Damage {
        let Some(node) = self.bounds(key) else {
            return Damage::None;
        };
        match damage {
            Damage::None => Damage::None,
            Damage::Full => Damage::rect(node.paint),
            damage => {
                let origin = node.layout.pixels(self.frame.scale());
                let region = damage.regions((u32::MAX, u32::MAX));
                Damage::region(retinue_render::damage::Region::new(
                    region.rects().iter().map(|rect| {
                        PaintExtent {
                            x: rect.x as i32,
                            y: rect.y as i32,
                            width: rect.width,
                            height: rect.height,
                        }
                        .visible((origin.x, origin.y), node.paint)
                    }),
                ))
            }
        }
    }

    pub fn hit_test(&self, point: (f64, f64)) -> Option<Key> {
        let point = (
            (point.0 / self.frame.scale() as f64) as f32,
            (point.1 / self.frame.scale() as f64) as f32,
        );
        self.geometry
            .iter()
            .rev()
            .find(|node| {
                node.interactive
                    && crate::input::HitShape::Rectangle.contains(node.layout, point)
                    && crate::input::HitShape::Rectangle.contains(node.clip, point)
            })
            .map(|node| node.key)
    }
}

#[cfg(test)]
mod tests;
