//! Retained layout and paint composition over the shared queued renderer.

mod canvas;
mod command;
mod identity;
mod layout;
mod motion;
mod paint;
mod projection;
mod recording;

pub use canvas::Canvas;
pub use identity::{Key, PaintKey};
pub use layout::{Algorithm, LayoutContext};
pub use motion::SelectionMotion;
pub use paint::{
    CachePolicy, Color, Command, ImageFit, ImageSource, Paint, PaintStats, RasterMode, Renderer,
    TextAlignment,
};
pub use recording::{Recording, Surface};

use crate::{Rect, damage::Damage};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};
use taffy::{FlexDirection, Size, Style};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn pixels(self, scale: f32) -> paint::Pixels {
        paint::Pixels {
            x: (self.x * scale).round() as i32,
            y: (self.y * scale).round() as i32,
            width: (self.width * scale).round().max(0.0) as u32,
            height: (self.height * scale).round().max(0.0) as u32,
        }
    }
}

pub struct Element {
    pub key: Key,
    pub style: Style,
    pub algorithm: Algorithm,
    pub paint: Option<Paint>,
    pub children: Vec<Element>,
    pub offset: (f32, f32),
    pub clip: bool,
    pub interactive: bool,
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
            style,
            algorithm: Algorithm::Flex,
            paint: None,
            children: Vec::new(),
            offset: (0.0, 0.0),
            clip: false,
            interactive: false,
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

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub key: Key,
    pub parent: Option<Key>,
    pub layout: Bounds,
    pub hit: Rect,
    pub paint: Rect,
    pub interactive: bool,
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
}

#[derive(Default)]
pub struct Scene {
    identity: Rc<()>,
    generation: u64,
    tree: layout::Tree,
    root: Option<usize>,
    chunks: Vec<paint::Chunk>,
    geometry: Vec<Geometry>,
    geometry_index: HashMap<Key, usize>,
    projection: Vec<projection::Placement>,
    size: (u32, u32),
    scale: f32,
    pub stats: UpdateStats,
    command_revision: u64,
}

impl Scene {
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
        let mut keys = HashSet::new();
        for (key, commands, cache) in nodes {
            if !keys.insert(key) {
                return Err("duplicate paint node");
            }
            let slot = self.tree.slot(key).ok_or("unknown paint node")?;
            let node = self.tree.nodes[slot].as_ref().unwrap();
            if node
                .recorded
                .as_ref()
                .is_some_and(|recorded| *recorded.commands == commands)
                && node
                    .element
                    .paint
                    .as_ref()
                    .is_some_and(|paint| paint.cache == cache)
            {
                continue;
            }
            self.command_revision += 1;
            let mut paint = Paint::new(PaintKey::new(key, self.command_revision), move |_| {
                commands.clone()
            });
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
        let root = self.tree.reconcile(root)?;
        self.root = Some(root);
        Ok(self.resolve(root, size, scale, mode))
    }

    pub fn resize(&mut self, size: (u32, u32), scale: f32) -> Result<Damage, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        let root = self.root.ok_or("composition has no root")?;
        self.tree.changed.clear();
        self.tree.topology_changed = false;
        Ok(self.resolve(root, size, scale, RecordingMode::Retained))
    }

    fn resolve(
        &mut self,
        root: usize,
        size: (u32, u32),
        scale: f32,
        mode: RecordingMode,
    ) -> Damage {
        let resized = self.size != size || self.scale != scale;
        self.size = size;
        self.scale = scale;
        self.tree.compute(
            root,
            Size {
                width: size.0 as f32 / scale,
                height: size.1 as f32 / scale,
            },
        );
        self.project(mode, resized)
    }

    pub fn update_subtree(&mut self, root: Element) -> Result<Damage, &'static str> {
        self.tree.replace(root)?;
        self.tree.compute(
            self.root.unwrap(),
            Size {
                width: self.size.0 as f32 / self.scale,
                height: self.size.1 as f32 / self.scale,
            },
        );
        Ok(self.project(RecordingMode::Retained, false))
    }

    fn rebuild_projection(&mut self, mode: RecordingMode, resized: bool) -> Damage {
        self.generation += 1;
        let old = std::mem::take(&mut self.chunks);
        self.geometry.clear();
        self.geometry_index.clear();
        self.projection.clear();
        self.collect(
            self.root.unwrap(),
            (0.0, 0.0),
            Rect {
                width: self.size.0,
                height: self.size.1,
                ..Rect::default()
            },
            mode,
        );
        if resized {
            return Damage::Full;
        }
        let current: HashSet<_> = self.chunks.iter().map(|chunk| chunk.key).collect();
        let mut previous: HashMap<_, _> = old
            .iter()
            .filter(|chunk| current.contains(&chunk.key))
            .enumerate()
            .map(|(order, chunk)| (chunk.key, (order, chunk)))
            .collect();
        let mut damage = Damage::None;
        let mut order = 0;
        for chunk in &self.chunks {
            let before = previous.remove(&chunk.key);
            let changed = before.is_none_or(|(old_order, old)| {
                old_order != order
                    || old.origin != chunk.origin
                    || old.clip != chunk.clip
                    || old.recorded.signature != chunk.recorded.signature
            });
            if before.is_some() {
                order += 1;
            }
            if changed {
                damage = damage.union(Damage::rect(chunk.visible));
                if let Some((_, old)) = before {
                    damage = damage.union(Damage::rect(old.visible));
                }
            }
        }
        for old in &old {
            if !current.contains(&old.key) {
                damage = damage.union(Damage::rect(old.visible));
            }
        }
        damage
    }

    fn collect(
        &mut self,
        slot: usize,
        parent: (f32, f32),
        clip: Rect,
        mode: RecordingMode,
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
        let pixels = bounds.pixels(self.scale);
        let hit = pixels.visible((0, 0), clip);
        let clip = if node.element.clip {
            clip.intersection(hit)
        } else {
            clip
        };
        let mut paint_bounds = Rect::default();
        let mut chunk_index = None;
        if let Some(paint) = &node.element.paint {
            let signature = paint::Signature {
                content: paint.key,
                width: node.layout.size.width.to_bits(),
                height: node.layout.size.height.to_bits(),
                scale: self.scale.to_bits(),
            };
            if mode == RecordingMode::Always
                || node
                    .recorded
                    .as_ref()
                    .is_none_or(|recorded| recorded.signature != signature)
            {
                node.recorded = Some(paint::Recorded::new(paint, node.layout.size, self.scale));
                self.stats.recordings += 1;
            }
            let recorded = node.recorded.as_ref().unwrap();
            paint_bounds = recorded.bounds.visible((pixels.x, pixels.y), clip);
            chunk_index = Some(self.chunks.len());
            self.chunks.push(paint::Chunk {
                key: node.element.key,
                recorded: recorded.clone(),
                origin: (pixels.x, pixels.y),
                clip,
                visible: paint_bounds,
                cache: paint.cache,
            });
        } else {
            node.recorded = None;
        }
        let geometry_index = self.geometry.len();
        self.geometry_index.insert(node.element.key, geometry_index);
        self.geometry.push(Geometry {
            key: node.element.key,
            parent: parent_key,
            layout: bounds,
            hit,
            paint: paint_bounds,
            interactive: node.element.interactive,
        });
        self.projection.push(projection::Placement {
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
        });
        let child_count = node.children.len();
        for index in 0..child_count {
            let child = self.tree.nodes[slot].as_ref().unwrap().children[index];
            let child_paint = self.collect(child, origin, clip, mode);
            self.geometry[geometry_index].paint =
                self.geometry[geometry_index].paint.union(child_paint);
        }
        self.projection[geometry_index].end = self.geometry.len();
        self.geometry[geometry_index].paint
    }

    pub fn patch(
        &mut self,
        changes: impl IntoIterator<Item = Change>,
    ) -> Result<Damage, &'static str> {
        let changes: Vec<_> = changes.into_iter().collect();
        let keys: Vec<_> = changes
            .iter()
            .map(|change| match change {
                Change::Offset(key, _) | Change::Paint(key, _) => *key,
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
        for (key, change) in keys.into_iter().zip(changes) {
            let slot = self.tree.slot(key).unwrap();
            let node = self.tree.nodes[slot].as_ref().unwrap();
            let unchanged = match &change {
                Change::Offset(_, offset) => node.element.offset == *offset,
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
            }
        }
        Ok(self.project(RecordingMode::Retained, false))
    }

    pub fn geometry(&self) -> &[Geometry] {
        &self.geometry
    }

    pub fn bounds(&self, key: Key) -> Option<Geometry> {
        self.geometry_index
            .get(&key)
            .map(|&index| self.geometry[index])
    }

    pub fn layout_rect(&self, key: Key) -> Option<Rect> {
        let pixels = self.bounds(key)?.layout.pixels(self.scale);
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
        let inset =
            |border: f32, padding: f32| ((border + padding) * self.scale).round().max(0.0) as u32;
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
            Damage::Rect(rect) => {
                let origin = node.layout.pixels(self.scale);
                let pixels = paint::Pixels {
                    x: rect.x as i32,
                    y: rect.y as i32,
                    width: rect.width,
                    height: rect.height,
                };
                Damage::rect(pixels.visible((origin.x, origin.y), node.paint))
            }
        }
    }

    pub fn hit_test(&self, point: (f64, f64)) -> Option<Key> {
        self.geometry
            .iter()
            .rev()
            .find(|node| node.interactive && node.hit.contains(point.0, point.1))
            .map(|node| node.key)
    }
}

#[cfg(test)]
mod tests;
