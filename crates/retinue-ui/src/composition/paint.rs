use super::{Bounds, Key, PaintKey};
use crate::{
    Rect,
    painter::{CachedFont, Painter, RasterImage},
    resources::{CacheStats, Resource, ResourceCache},
};
use std::rc::Rc;
use taffy::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color([u8; 4]);

impl Color {
    pub const fn rgb(rgb: u32) -> Self {
        Self([rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, 255])
    }

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([blue, green, red, alpha])
    }

    pub const fn bgra(self) -> [u8; 4] {
        self.0
    }

    pub const fn from_bgra(value: [u8; 4]) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Top,
    Center,
    WrappedCenter,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImageFit {
    Stretch,
    Contain,
    Circle,
    Cover,
}

#[derive(Clone)]
pub enum ImageSource {
    Shared(Rc<RasterImage>),
    Cached(Resource<RasterImage>),
}

impl std::ops::Deref for ImageSource {
    type Target = RasterImage;
    fn deref(&self) -> &RasterImage {
        match self {
            Self::Shared(image) => image,
            Self::Cached(image) => image,
        }
    }
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(&**self, &**other)
    }
}

impl From<Rc<RasterImage>> for ImageSource {
    fn from(image: Rc<RasterImage>) -> Self {
        Self::Shared(image)
    }
}
impl From<&Rc<RasterImage>> for ImageSource {
    fn from(image: &Rc<RasterImage>) -> Self {
        Self::Shared(image.clone())
    }
}
impl From<Resource<RasterImage>> for ImageSource {
    fn from(image: Resource<RasterImage>) -> Self {
        Self::Cached(image)
    }
}
impl From<&Resource<RasterImage>> for ImageSource {
    fn from(image: &Resource<RasterImage>) -> Self {
        Self::Cached(image.clone())
    }
}
impl From<&ImageSource> for ImageSource {
    fn from(image: &ImageSource) -> Self {
        image.clone()
    }
}

#[derive(Clone)]
pub enum Command {
    Fill {
        bounds: Bounds,
        radius: f32,
        color: Color,
    },
    Outline {
        bounds: Bounds,
        radius: f32,
        thickness: f32,
        color: Color,
    },
    Ring {
        center: (f32, f32),
        radius: f32,
        thickness: f32,
        color: Color,
    },
    Text {
        bounds: Bounds,
        size: f32,
        text: String,
        font: Rc<CachedFont>,
        color: Color,
        align: TextAlignment,
    },
    InsetShadow {
        bounds: Bounds,
        radius: f32,
        blur: f32,
        spread: f32,
        color: Color,
    },
    Image {
        bounds: Bounds,
        image: ImageSource,
        fit: ImageFit,
    },
    Clip {
        bounds: Bounds,
        commands: Rc<Vec<Command>>,
    },
    Translate {
        offset: (f32, f32),
        commands: Rc<Vec<Command>>,
    },
    Line {
        from: (f32, f32),
        to: (f32, f32),
        thickness: f32,
        color: Color,
    },
    Polygon {
        points: Vec<[f32; 2]>,
        color: Color,
    },
    External {
        bounds: Bounds,
        key: Key,
    },
}

impl Command {
    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Fill { bounds, .. }
            | Self::Outline { bounds, .. }
            | Self::Text { bounds, .. }
            | Self::InsetShadow { bounds, .. }
            | Self::Image { bounds, .. }
            | Self::Clip { bounds, .. }
            | Self::External { bounds, .. } => *bounds,
            Self::Ring {
                center,
                radius,
                thickness,
                ..
            } => {
                let extent = radius + thickness / 2.0 + 1.0;
                Bounds::new(
                    center.0 - extent,
                    center.1 - extent,
                    extent * 2.0,
                    extent * 2.0,
                )
            }
            Self::Translate { offset, commands } => {
                let mut bounds = commands
                    .iter()
                    .map(Self::bounds)
                    .reduce(|a, b| {
                        Bounds::new(
                            a.x.min(b.x),
                            a.y.min(b.y),
                            (a.x + a.width).max(b.x + b.width) - a.x.min(b.x),
                            (a.y + a.height).max(b.y + b.height) - a.y.min(b.y),
                        )
                    })
                    .unwrap_or_default();
                bounds.x += offset.0;
                bounds.y += offset.1;
                bounds
            }
            Self::Line {
                from,
                to,
                thickness,
                ..
            } => {
                let pad = thickness / 2.0 + 1.0;
                Bounds::new(
                    from.0.min(to.0) - pad,
                    from.1.min(to.1) - pad,
                    (from.0 - to.0).abs() + 2.0 * pad,
                    (from.1 - to.1).abs() + 2.0 * pad,
                )
            }
            Self::Polygon { points, .. } => {
                let Some(first) = points.first() else {
                    return Bounds::default();
                };
                let (mut min, mut max) = (*first, *first);
                for point in points {
                    for axis in 0..2 {
                        min[axis] = min[axis].min(point[axis]);
                        max[axis] = max[axis].max(point[axis]);
                    }
                }
                Bounds::new(min[0], min[1], max[0] - min[0], max[1] - min[1])
            }
        }
    }

    fn paint(
        &self,
        painter: &mut Painter<'_>,
        scale: f32,
        origin: (i32, i32),
        external: &mut impl FnMut(Key, &mut Painter<'_>),
    ) {
        match self {
            Self::Ring {
                center,
                radius,
                thickness,
                color,
            } => {
                let center = (
                    origin.0 + (center.0 * scale).round() as i32,
                    origin.1 + (center.1 * scale).round() as i32,
                );
                painter.translated(center, |p| {
                    p.draw_ring(
                        0,
                        0,
                        (radius * scale).round() as u32,
                        (thickness * scale).round() as u32,
                        color.bgra(),
                    )
                });
                return;
            }
            Self::Translate { offset, commands } => {
                let origin = (
                    origin.0 + (offset.0 * scale).round() as i32,
                    origin.1 + (offset.1 * scale).round() as i32,
                );
                for command in commands.iter() {
                    command.paint(painter, scale, origin, external);
                }
                return;
            }
            Self::Clip { bounds, commands } => {
                let pixels = bounds.pixels(scale);
                let clip = pixels.visible(origin, painter.visible_bounds());
                painter.viewport(clip, (0, 0), |painter| {
                    for command in commands.iter() {
                        command.paint(painter, scale, origin, external);
                    }
                });
                return;
            }
            Self::Line {
                from,
                to,
                thickness,
                color,
            } => {
                painter.draw_line(
                    (
                        (from.0 * scale) as f64 + origin.0 as f64,
                        (from.1 * scale) as f64 + origin.1 as f64,
                    ),
                    (
                        (to.0 * scale) as f64 + origin.0 as f64,
                        (to.1 * scale) as f64 + origin.1 as f64,
                    ),
                    (thickness * scale) as f64,
                    color.bgra(),
                );
                return;
            }
            Self::Polygon { points, color } => {
                let points: Vec<_> = points
                    .iter()
                    .map(|p| {
                        [
                            (p[0] * scale) as f64 + origin.0 as f64,
                            (p[1] * scale) as f64 + origin.1 as f64,
                        ]
                    })
                    .collect();
                painter.fill_polygon(&points, color.bgra());
                return;
            }
            _ => (),
        }
        let bounds = self.bounds().pixels(scale);
        let position = (origin.0 + bounds.x, origin.1 + bounds.y);
        if position.0 >= 0
            && position.1 >= 0
            && self.paint_primitive(
                painter,
                Rect {
                    x: position.0 as u32,
                    y: position.1 as u32,
                    width: bounds.width,
                    height: bounds.height,
                },
                scale,
            )
        {
            return;
        }
        let rect = Rect {
            x: 0,
            y: 0,
            width: bounds.width,
            height: bounds.height,
        };
        painter.translated(position, |painter| {
            if self.paint_primitive(painter, rect, scale) {
                return;
            }
            match self {
                Self::External { key, .. } => {
                    painter.clip_to_rect(rect);
                    external(*key, painter);
                }
                _ => unreachable!("primitive handled above"),
            }
        });
    }

    fn paint_primitive(&self, p: &mut Painter<'_>, rect: Rect, scale: f32) -> bool {
        match self {
            Self::Text {
                size,
                text,
                font,
                color,
                align,
                ..
            } => {
                p.viewport(rect, (0, 0), |p| match align {
                    TextAlignment::Top => p.draw_text(
                        font.as_ref(),
                        rect.x,
                        rect.y,
                        size * scale,
                        text,
                        color.bgra(),
                    ),
                    TextAlignment::Center => p.draw_text_vertically_centered(
                        font.as_ref(),
                        crate::painter::TextLine::new(rect.x, rect.y, rect.height),
                        size * scale,
                        text,
                        color.bgra(),
                    ),
                    TextAlignment::WrappedCenter => p.draw_centered_text(
                        font.as_ref(),
                        crate::painter::TextBox::new(rect.x, rect.y, rect.width),
                        size * scale,
                        text,
                        color.bgra(),
                    ),
                });
            }
            Self::Fill { radius, color, .. } => p.fill_rounded_rect(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                (*radius * scale).round() as u32,
                color.bgra(),
            ),
            Self::Outline {
                radius,
                thickness,
                color,
                ..
            } => p.outline(
                rect,
                (*radius * scale).round() as u32,
                (*thickness * scale).round().max(1.0) as u32,
                color.bgra(),
            ),
            Self::InsetShadow {
                radius,
                blur,
                spread,
                color,
                ..
            } => p.inset_shadow(
                rect,
                (*radius * scale).round() as u32,
                blur * scale,
                spread * scale,
                color.bgra(),
            ),
            Self::Image { image, fit, .. } => match fit {
                ImageFit::Stretch => p.draw_image(image, rect.x, rect.y, rect.width, rect.height),
                ImageFit::Contain => {
                    p.draw_image_contained(image, rect.x, rect.y, rect.width, rect.height)
                }
                ImageFit::Circle => {
                    p.draw_image_contained_circle(image, rect.x, rect.y, rect.width)
                }
                ImageFit::Cover => p.draw_image_covered(image, rect),
            },
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CachePolicy {
    #[default]
    Never,
    Prefer,
}

#[derive(Clone)]
pub struct Paint {
    /// Content identity including resource generations; not the element's identity.
    pub key: PaintKey,
    pub cache: CachePolicy,
    pub record: Rc<dyn Fn(Size<f32>) -> Vec<Command>>,
}

impl Paint {
    pub fn new(
        key: impl Into<PaintKey>,
        record: impl Fn(Size<f32>) -> Vec<Command> + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            cache: CachePolicy::Never,
            record: Rc::new(record),
        }
    }

    /// The host reports changes with Scene::invalidate; live drawing is never cached.
    pub fn external(key: Key) -> Self {
        Self::new(PaintKey::new(key, 0), move |size| {
            vec![Command::External {
                key,
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
            }]
        })
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct Signature {
    pub content: PaintKey,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}

#[derive(Clone)]
pub(super) struct Recorded {
    pub signature: Signature,
    pub bounds: Pixels,
    pub commands: Rc<Vec<Command>>,
}

impl Recorded {
    pub fn new(paint: &Paint, size: Size<f32>, scale: f32) -> Self {
        let commands = (paint.record)(size);
        let bounds = commands
            .iter()
            .map(|command| command.bounds().pixels(scale))
            .reduce(Pixels::union)
            .unwrap_or_default();
        Self {
            signature: Signature {
                content: paint.key,
                width: size.width.to_bits(),
                height: size.height.to_bits(),
                scale: scale.to_bits(),
            },
            bounds,
            commands: Rc::new(commands),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Pixels {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Pixels {
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width as i32)
                .max(other.x + other.width as i32)
                .saturating_sub(x) as u32,
            height: (self.y + self.height as i32)
                .max(other.y + other.height as i32)
                .saturating_sub(y) as u32,
        }
    }

    pub fn visible(self, origin: (i32, i32), clip: Rect) -> Rect {
        let left = self.x + origin.0;
        let top = self.y + origin.1;
        let x = left.max(clip.x as i32);
        let y = top.max(clip.y as i32);
        let right = (left + self.width as i32).min((clip.x + clip.width) as i32);
        let bottom = (top + self.height as i32).min((clip.y + clip.height) as i32);
        Rect {
            x: x as u32,
            y: y as u32,
            width: (right - x).max(0) as u32,
            height: (bottom - y).max(0) as u32,
        }
    }
}

pub(super) struct Chunk {
    pub key: Key,
    pub recorded: Recorded,
    pub origin: (i32, i32),
    pub clip: Rect,
    pub visible: Rect,
    pub cache: CachePolicy,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RasterMode {
    #[default]
    Replay,
    Explicit,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct PaintStats {
    pub replayed_commands: usize,
    pub blits: usize,
    pub prepared_rasters: usize,
    pub repaired_pixels: usize,
    pub copied_pixels: usize,
}

struct CachedRaster {
    image: Rc<RasterImage>,
    recorded: Recorded,
}

impl crate::resources::ResourceSize for CachedRaster {
    fn heap_bytes(&self) -> usize {
        self.image.heap_bytes()
            + self
                .recorded
                .commands
                .iter()
                .map(Command::retained_bytes)
                .sum::<usize>()
    }
}

pub struct Renderer {
    cache: ResourceCache<Signature, CachedRaster>,
    prepared: Vec<Option<Resource<CachedRaster>>>,
    previous: Vec<Option<Resource<CachedRaster>>>,
    prepared_for: Option<(Rc<()>, u64)>,
    mode: RasterMode,
    pub stats: PaintStats,
}

impl Renderer {
    pub fn new(budget: usize) -> Self {
        Self {
            cache: ResourceCache::new(budget),
            prepared: Vec::new(),
            previous: Vec::new(),
            prepared_for: None,
            mode: RasterMode::Replay,
            stats: PaintStats::default(),
        }
    }

    pub fn clear_cache(&mut self) {
        self.prepared.clear();
        self.previous.clear();
        self.cache.clear();
        self.prepared_for = None;
    }

    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    pub fn prepare(&mut self, scene: &super::Scene, mode: RasterMode) {
        self.stats = PaintStats::default();
        if self
            .prepared_for
            .as_ref()
            .is_some_and(|(identity, _)| !Rc::ptr_eq(identity, &scene.identity))
        {
            self.clear_cache();
        }
        if self.mode == mode
            && self
                .prepared_for
                .as_ref()
                .is_some_and(|(identity, generation)| {
                    Rc::ptr_eq(identity, &scene.identity) && *generation == scene.generation
                })
        {
            return;
        }
        self.mode = mode;
        std::mem::swap(&mut self.prepared, &mut self.previous);
        for chunk in &scene.chunks {
            let image = if mode == RasterMode::Explicit
                && !chunk.visible.is_empty()
                && chunk.cache == CachePolicy::Prefer
                && self.cache.budget_bytes() > 0
                && chunk.recorded.commands.iter().all(Command::cacheable)
            {
                let recorded = &chunk.recorded;
                let mut key = recorded.signature;
                key.content.revision = 0;
                let previous = self.cache.get(&key);
                if let Some(previous) = previous
                    .as_ref()
                    .filter(|entry| entry.recorded.signature == recorded.signature)
                {
                    Some(previous.clone())
                } else {
                    let bounds = recorded.bounds;
                    let previous = previous.filter(|entry| entry.recorded.bounds == bounds);
                    let canvas = Rect {
                        width: bounds.width,
                        height: bounds.height,
                        ..Rect::default()
                    };
                    let regions = previous.as_ref().map_or_else(
                        || vec![canvas],
                        |entry| {
                            super::command::changed_regions(
                                &entry.recorded.commands,
                                &recorded.commands,
                                scene.scale,
                                bounds,
                            )
                        },
                    );
                    let repaired = regions
                        .iter()
                        .map(|r| r.width as usize * r.height as usize)
                        .sum::<usize>();
                    let copy = previous.is_some()
                        && repaired < bounds.width as usize * bounds.height as usize;
                    let image = if regions.is_empty() {
                        Ok(previous.as_ref().unwrap().image.clone())
                    } else {
                        // Optional caches must not change blending or fail a valid direct paint.
                        RasterImage::render(bounds.width, bounds.height, |painter| {
                            if copy {
                                painter.draw_image(
                                    &previous.as_ref().unwrap().image,
                                    0,
                                    0,
                                    bounds.width,
                                    bounds.height,
                                );
                            }
                            for region in &regions {
                                painter.viewport(*region, (0, 0), |painter| {
                                    painter.clear();
                                    for command in recorded.commands.iter() {
                                        if command
                                            .bounds()
                                            .pixels(scene.scale)
                                            .visible((-bounds.x, -bounds.y), *region)
                                            .is_empty()
                                        {
                                            continue;
                                        }
                                        command.paint(
                                            painter,
                                            scene.scale,
                                            (-bounds.x, -bounds.y),
                                            &mut |_, _| {
                                                unreachable!(
                                                    "external paint cannot be raster cached"
                                                )
                                            },
                                        );
                                    }
                                });
                            }
                        })
                        .map(Rc::new)
                    };
                    match image {
                        Ok(image) => {
                            self.stats.prepared_rasters += usize::from(!regions.is_empty());
                            self.stats.repaired_pixels += repaired;
                            if copy && !regions.is_empty() {
                                self.stats.copied_pixels +=
                                    bounds.width as usize * bounds.height as usize;
                            }
                            Some(self.cache.insert(
                                key,
                                CachedRaster {
                                    image,
                                    recorded: recorded.clone(),
                                },
                            ))
                        }
                        Err(_) => None,
                    }
                }
            } else {
                None
            };
            self.prepared.push(image);
        }
        self.previous.clear();
        self.prepared_for = Some((scene.identity.clone(), scene.generation));
    }

    pub fn paint(
        &mut self,
        scene: &super::Scene,
        painter: &mut Painter<'_>,
        repair: Rect,
        background: Color,
    ) {
        self.paint_with(scene, painter, repair, background, |_, _| {
            panic!("external paint requires a host")
        });
    }

    pub fn paint_with(
        &mut self,
        scene: &super::Scene,
        painter: &mut Painter<'_>,
        repair: Rect,
        background: Color,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) {
        painter.viewport(repair, (0, 0), |painter| {
            painter.clear();
            painter.fill_rounded_rect(
                repair.x,
                repair.y,
                repair.width,
                repair.height,
                0,
                background.bgra(),
            );
            self.draw_with(scene, painter, repair, external);
        });
    }

    pub fn draw_with(
        &mut self,
        scene: &super::Scene,
        painter: &mut Painter<'_>,
        repair: Rect,
        mut external: impl FnMut(Key, &mut Painter<'_>),
    ) {
        assert!(
            self.prepared_for
                .as_ref()
                .is_some_and(|(identity, generation)| {
                    Rc::ptr_eq(identity, &scene.identity) && *generation == scene.generation
                }),
            "prepare this scene generation before painting"
        );
        let repair = repair.intersection(painter.visible_bounds());
        painter.viewport(repair, (0, 0), |painter| {
            for (chunk, image) in scene.chunks.iter().zip(&self.prepared) {
                if chunk.visible.intersection(repair).is_empty() {
                    continue;
                }
                let bounds = chunk.recorded.bounds;
                // Ordinary primitives share the host transform. Only local callbacks need a new one.
                painter.viewport(chunk.clip, (0, 0), |painter| {
                    if let Some(image) = image {
                        painter.translated(
                            (chunk.origin.0 + bounds.x, chunk.origin.1 + bounds.y),
                            |painter| {
                                painter.draw_image(&image.image, 0, 0, bounds.width, bounds.height);
                            },
                        );
                        self.stats.blits += 1;
                    } else {
                        for command in chunk.recorded.commands.iter() {
                            command.paint(painter, scene.scale, chunk.origin, &mut external);
                        }
                        self.stats.replayed_commands += chunk.recorded.commands.len();
                    }
                });
            }
        });
    }
}
