//! UI composition, layout, motion and recording into the render pipeline.

#[cfg(test)]
mod batch_tests;
pub(crate) mod execution;
pub(crate) mod layout;
#[cfg(test)]
mod layout_tests;
mod loading;
mod motion;
mod paint;
mod projection;
mod rendering;
mod retention;
pub(crate) mod transaction;
mod window;
mod work;

pub use layout::{Algorithm, LayoutExecution};
pub use loading::LoadingFrame;
pub use motion::SelectionMotion;
pub use paint::Paint;

use crate::{Bounds, Key, Rect};
use crate::{layout as layout_protocol, layout::values, semantics};
use retinue_render::{damage::Damage, *};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use semantics::{Role, WindowAction};
#[cfg(test)]
use std::rc::Rc;
use std::task::Poll;
use taffy::{FlexDirection, Size, Style};
use work::Work;

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
    pub recording: std::time::Duration,
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
    Placement(Key, Option<crate::Bounds>),
    LayoutKey(Key, Option<Key>),
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
    input_motion: Option<(u64, InputMotion)>,
    layout_unpublished: bool,
    pending_layout: Option<execution::PendingLayout>,
}

struct FrameDraft {
    scale: f32,
    instances: Vec<PaintInstance>,
    groups: Vec<PaintGroup>,
}

enum ProjectionSlot {
    Tree(usize),
    Published(usize),
}

pub(crate) enum InputMotion {
    All,
    Ranges(Vec<std::ops::Range<usize>>),
}

impl InputMotion {
    pub(crate) fn touches(&self, sources: &[usize]) -> bool {
        match self {
            Self::All => !sources.is_empty(),
            Self::Ranges(ranges) => sources.iter().any(|&i| {
                ranges
                    .get(ranges.partition_point(|r| r.end <= i))
                    .is_some_and(|r| r.start <= i)
            }),
        }
    }
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
        self.ensure_idle()?;
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
                    .paint
                    .as_ref()
                    .is_some_and(|paint| paint.cache == cache)
            {
                continue;
            }
            self.command_revision += 1;
            let mut paint = Paint::commands(
                PaintKey::new(key, self.command_revision),
                std::sync::Arc::new(commands),
            );
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
        execution::inline(self.update_with(root, size, scale, mode, None))
    }

    pub(crate) fn update_with(
        &mut self,
        root: Element,
        size: (u32, u32),
        scale: f32,
        mode: RecordingMode,
        execution: Option<&LayoutExecution>,
    ) -> execution::Update {
        self.ensure_idle()?;
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
        self.resolve_with(root, size, scale, mode, execution)
    }

    pub fn resize(&mut self, size: (u32, u32), scale: f32) -> Result<Damage, &'static str> {
        execution::inline(self.resize_with(size, scale, None))
    }

    pub(crate) fn resize_with(
        &mut self,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> execution::Update {
        self.ensure_idle()?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let root = self.root.ok_or("composition has no root")?;
        self.tree.state.changed.clear();
        self.tree.topology_changed = false;
        self.resolve_with(root, size, scale, RecordingMode::Retained, execution)
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
        execution::inline(self.update_retained_subtrees_with(roots, size, scale, None))
    }

    pub(crate) fn update_retained_subtrees_with(
        &mut self,
        roots: Vec<layout::Subtree>,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> execution::Update {
        self.ensure_idle()?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let root = self.root.ok_or("composition has no root")?;
        if roots.is_empty() {
            return if self.frame.size() == size && self.frame.scale() == scale {
                Ok(Poll::Ready(Damage::default()))
            } else {
                self.resize_with(size, scale, execution)
            };
        }
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        self.tree.replace_many(roots)?;
        #[cfg(feature = "profiling")]
        {
            self.profile.reconcile += started.elapsed();
        }
        self.resolve_with(root, size, scale, RecordingMode::Retained, execution)
    }

    pub fn contains(&self, key: Key) -> bool {
        self.tree.slot(key).is_some()
    }

    pub fn layout_failure(&self) -> Option<&layout_protocol::LayoutFailure> {
        self.tree.state.failure.as_ref()
    }

    fn rebuild_projection(&mut self, mode: RecordingMode, size: (u32, u32), scale: f32) -> Damage {
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let root = self.root.unwrap();
        let reuse_slots = mode == RecordingMode::Retained
            && (self.frame.size() != size || self.frame.scale() != scale)
            && !self.tree.topology_changed
            && !self.layout_unpublished
            && self.projection.first().is_some_and(|p| p.node == root);
        self.input_motion = reuse_slots.then_some((self.input_revision, InputMotion::All));
        self.input_revision = self.input_revision.wrapping_add(1);
        let retained = (mode == RecordingMode::Retained
            && self.frame.size() == size
            && self.frame.scale() == scale
            && !self.layout_unpublished
            && !self.geometry.is_empty())
        .then(|| retention::Snapshot::take(self))
        .flatten();
        let mut draft = FrameDraft {
            scale,
            instances: Vec::new(),
            groups: Vec::new(),
        };
        let start = if reuse_slots {
            ProjectionSlot::Published(0)
        } else {
            self.geometry.clear();
            self.geometry_index.clear();
            self.projection.clear();
            ProjectionSlot::Tree(root)
        };
        self.collect(
            start,
            None,
            Rect {
                width: size.0,
                height: size.1,
                ..Rect::default()
            },
            Bounds::new(0.0, 0.0, size.0 as f32 / scale, size.1 as f32 / scale),
            mode,
            retained.as_ref(),
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
        source: ProjectionSlot,
        parent_index: Option<usize>,
        clip: Rect,
        input_clip: Bounds,
        mode: RecordingMode,
        retained: Option<&retention::Snapshot>,
        draft: &mut FrameDraft,
    ) -> Rect {
        let (slot, published) = match source {
            ProjectionSlot::Tree(slot) => (slot, None),
            ProjectionSlot::Published(index) => {
                let placement = &self.projection[index];
                (placement.node, Some((index, placement.end)))
            }
        };
        if let Some(retained) = retained
            && let Some(index) = retained.clean(self.tree.inputs[slot].as_ref().unwrap().key)
        {
            return self.restore_projection(retained, index, parent_index, draft);
        }
        self.stats.nodes += 1;
        let parent_key = parent_index.map(|index| self.geometry[index].key);
        let parent = parent_index.map_or((0.0, 0.0), |index| {
            let bounds = self.geometry[index].layout;
            (bounds.x, bounds.y)
        });
        let rendering = self.tree.render_context(slot);
        let node = self.tree.nodes[slot].as_mut().unwrap();
        let input = self.tree.inputs[slot].as_ref().unwrap();
        let layout = self.tree.state.nodes[slot].as_ref().unwrap().layout;
        if input.style.display == taffy::Display::None {
            return Rect::default();
        }
        let origin = (
            parent.0 + layout.location.x + node.offset.0,
            parent.1 + layout.location.y + node.offset.1,
        );
        let bounds = Bounds::new(origin.0, origin.1, layout.size.width, layout.size.height);
        let pixels = bounds.pixels(draft.scale);
        let input_clip = if node.clip {
            input_clip.intersection(bounds)
        } else {
            input_clip
        };
        let hit = projection::layout_pixels(bounds, draft.scale).visible((0, 0), clip);
        let clip = if node.clip {
            clip.intersection(hit)
        } else {
            clip
        };
        let mut paint_bounds = Rect::default();
        let mut chunk_index = None;
        let group_start = draft.instances.len();
        if let Some(paint) = &node.paint {
            let size = (layout.size.width, layout.size.height);
            let revision = self.tree.state.nodes[slot].as_ref().unwrap().revision;
            if mode == RecordingMode::Always
                || paint.uses_layout && node.recorded_layout != revision
                || node
                    .recorded
                    .as_ref()
                    .is_none_or(|recorded| !recorded.matches(paint.key, size, draft.scale))
            {
                let result = self.tree.state.nodes[slot]
                    .as_ref()
                    .unwrap()
                    .result(node.custom_cache.as_ref());
                #[cfg(feature = "profiling")]
                let started = std::time::Instant::now();
                node.recorded =
                    Some(paint.compile(&mut self.programs, layout.size, draft.scale, result));
                #[cfg(feature = "profiling")]
                {
                    self.profile.recording += started.elapsed();
                }
                node.recorded_layout = revision;
                self.stats.recordings += 1;
            }
            let recorded = node.recorded.as_ref().unwrap();
            paint_bounds = recorded.bounds().visible((pixels.x, pixels.y), clip);
            chunk_index = Some(draft.instances.len());
            let mut instance =
                PaintInstance::new(input.key, recorded.clone(), (pixels.x, pixels.y), clip);
            instance.cache = paint.cache;
            draft.instances.push(instance);
        } else {
            node.recorded = None;
        }
        let geometry_index = published.map_or(self.geometry.len(), |(index, _)| index);
        let group = (node.cache_hint != CacheHint::Dynamic
            || rendering != OptimizationContext::default())
        .then(|| {
            let index = draft.groups.len();
            draft.groups.push(PaintGroup {
                key: input.key,
                instances: group_start..group_start,
                context: retinue_render::OptimizationContext::default().with(node.cache_hint),
            });
            index
        });
        let hit_enabled =
            node.hit_testing && parent_index.is_none_or(|index| self.projection[index].hit_enabled);
        let geometry = Geometry {
            key: input.key,
            parent: parent_key,
            layout: bounds,
            clip: input_clip,
            hit,
            paint: paint_bounds,
            interactive: node.interactive && hit_enabled,
            role: node.role,
            window_action: node.window_action,
        };
        let placement = projection::Placement {
            node: slot,
            parent: parent_index,
            end: geometry_index + 1,
            local: Bounds::new(
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
            content_insets: layout.border + layout.padding,
            offset: node.offset,
            clipper: node.clip,
            clip,
            own_paint: paint_bounds,
            chunk: chunk_index,
            group,
            instances: group_start..group_start,
            interactive: node.interactive,
            hit_testing: node.hit_testing,
            hit_enabled,
        };
        let child_count = input.children.len();
        let (mut index, end) = if let Some((index, end)) = published {
            debug_assert_eq!(self.geometry[index].key, input.key);
            if self.projection[index].hit_enabled != hit_enabled
                || self.geometry[index].interactive != geometry.interactive
                || self.geometry[index].role != geometry.role
                || self.geometry[index].window_action != geometry.window_action
            {
                self.input_motion = None;
            }
            self.geometry[index] = geometry;
            self.projection[index] = placement;
            (index + 1, end)
        } else {
            self.geometry_index.insert(input.key, geometry_index);
            self.geometry.push(geometry);
            self.projection.push(placement);
            (0, child_count)
        };
        while index < end {
            let child = if published.is_some() {
                let child = index;
                index = self.projection[child].end;
                ProjectionSlot::Published(child)
            } else {
                let child = self.tree.inputs[slot].as_ref().unwrap().children[index];
                index += 1;
                ProjectionSlot::Tree(child)
            };
            let child_paint = self.collect(
                child,
                Some(geometry_index),
                clip,
                input_clip,
                mode,
                retained,
                draft,
            );
            self.geometry[geometry_index].paint =
                self.geometry[geometry_index].paint.union(child_paint);
        }
        if let Some(group) = group {
            draft.groups[group].instances.end = draft.instances.len();
        }
        self.projection[geometry_index].end = published.map_or(self.geometry.len(), |(_, end)| end);
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

    #[cfg(test)]
    pub(crate) fn patch_elements(
        &mut self,
        elements: Vec<Element>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Damage, &'static str> {
        execution::inline(self.patch_elements_with(elements, size, scale, None))
    }

    #[cfg(any(test, feature = "reference-frontend"))]
    pub(crate) fn patch_elements_with(
        &mut self,
        elements: Vec<Element>,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> execution::Update {
        self.ensure_idle()?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let mut slots = Vec::with_capacity(elements.len());
        for element in &elements {
            if !layout::Tree::supports_style(&element.style) {
                return Err("unsupported layout calc handle");
            }
            if !element.children.is_empty() {
                return Err("property update cannot replace children");
            }
            if !element.offset.0.is_finite() || !element.offset.1.is_finite() {
                return Err("composition offset must be finite");
            }
            slots.push(
                self.tree
                    .slot(element.key)
                    .ok_or("unknown composition key")?,
            );
        }
        self.tree.state.changed.clear();
        self.tree.topology_changed = false;
        self.tree.state.computed = 0;
        let mut relayout =
            self.layout_unpublished || self.frame.size() != size || self.frame.scale() != scale;
        for (slot, element) in slots.into_iter().zip(elements) {
            let parent = self.tree.inputs[slot].as_ref().unwrap().parent;
            relayout |= self.tree.replace_element(slot, element, parent);
        }
        self.finish_patch(relayout, size, scale, execution)
    }

    pub(crate) fn patch_at(
        &mut self,
        changes: impl IntoIterator<Item = Change>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Damage, &'static str> {
        let admitted = self.transact(
            Vec::new(),
            &mut changes.into_iter().collect(),
            size,
            scale,
            None,
        )?;
        execution::inline(admitted.completion)
    }

    fn apply_changes(&mut self, changes: impl IntoIterator<Item = Change>) -> bool {
        let mut relayout = false;
        for change in changes {
            let key = change.key();
            let slot = self.tree.slot(key).unwrap();
            let node = self.tree.nodes[slot].as_ref().unwrap();
            let unchanged = match &change {
                Change::Offset(_, offset) => node.offset == *offset,
                Change::Layout(_, style, algorithm) => {
                    self.tree.matches_layout(slot, style, algorithm)
                }
                Change::LayoutValues(_, values) => {
                    self.tree.inputs[slot].as_ref().unwrap().layout_values == *values
                }
                Change::Placement(_, bounds) => self.tree.inputs[slot]
                    .as_ref()
                    .unwrap()
                    .layout_values
                    .is_single::<crate::layout::positioned::Placement>(bounds),
                Change::LayoutKey(_, name) => {
                    self.tree.inputs[slot].as_ref().unwrap().layout_key == *name
                }
                Change::Clip(_, clip) => node.clip == *clip,
                Change::Interactive(_, hit) => node.interactive == *hit,
                Change::HitTesting(_, enabled) => node.hit_testing == *enabled,
                Change::Role(_, role) => node.role == *role,
                Change::WindowAction(_, action) => node.window_action == *action,
                Change::Reuse(_, hint) => node.cache_hint == *hint,
                Change::Paint(_, paint) => node.paint == *paint,
            };
            if unchanged {
                continue;
            }
            self.tree
                .state
                .changed
                .mark(slot, Work::for_change(&change));
            let node = self.tree.nodes[slot].as_mut().unwrap();
            match change {
                Change::Offset(_, offset) => node.offset = offset,
                Change::Paint(_, paint) => {
                    self.tree.inputs[slot].as_mut().unwrap().records_layout =
                        paint.as_ref().is_some_and(|paint| paint.uses_layout);
                    node.paint = paint;
                }
                Change::Layout(_, style, algorithm) => {
                    self.tree.set_layout(slot, style, algorithm);
                    relayout = true;
                }
                Change::LayoutValues(_, values) => {
                    self.tree.inputs[slot].as_mut().unwrap().layout_values = values;
                    relayout |= self.tree.invalidate_arranger(slot);
                }
                Change::Placement(_, bounds) => {
                    self.tree.inputs[slot]
                        .as_mut()
                        .unwrap()
                        .layout_values
                        .replace_single::<crate::layout::positioned::Placement>(bounds);
                    relayout |= self.tree.invalidate_arranger(slot);
                }
                Change::LayoutKey(_, name) => {
                    self.tree.inputs[slot].as_mut().unwrap().layout_key = name;
                    relayout |= self.tree.invalidate_arranger(slot);
                }
                Change::Clip(_, clip) => node.clip = clip,
                Change::Interactive(_, hit) => node.interactive = hit,
                Change::HitTesting(_, enabled) => node.hit_testing = enabled,
                Change::Role(_, role) => node.role = role,
                Change::WindowAction(_, action) => node.window_action = action,
                Change::Reuse(_, hint) => {
                    node.cache_hint = hint;
                    self.tree.topology_changed = true;
                }
            }
        }
        relayout
    }

    fn finish_patch(
        &mut self,
        relayout: bool,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> execution::Update {
        if relayout {
            self.resolve_with(
                self.root.ok_or("composition has no root")?,
                size,
                scale,
                RecordingMode::Retained,
                execution,
            )
        } else {
            Ok(Poll::Ready(self.project(
                RecordingMode::Retained,
                size,
                scale,
            )))
        }
    }

    pub fn geometry(&self) -> &[Geometry] {
        &self.geometry
    }

    pub(crate) fn input_revision(&self) -> u64 {
        self.input_revision
    }

    /// Only the immediately preceding geometry can be patched by projection index.
    pub(crate) fn input_motion_since(&self, revision: u64) -> Option<&InputMotion> {
        let (from, ranges) = self.input_motion.as_ref()?;
        (*from == revision).then_some(ranges)
    }

    pub(crate) fn input_source(&self, key: Key) -> Option<(usize, bool)> {
        let index = *self.geometry_index.get(&key)?;
        Some((index, self.projection[index].hit_enabled))
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
        let insets = self.projection[*self.geometry_index.get(&key)?].content_insets;
        let inset = |length: f32| (length * self.frame.scale()).round().max(0.0) as u32;
        let left = inset(insets.left);
        let top = inset(insets.top);
        let right = inset(insets.right);
        let bottom = inset(insets.bottom);
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
