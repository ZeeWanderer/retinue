use super::{Bounds, Key, OptimizationContext, PaintProgram};
use crate::{
    Rect,
    painter::{CachedFont, Painter, RasterImage},
    resources::Resource,
};
use std::rc::Rc;

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
        self.id() == other.id()
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
    Fragment {
        program: PaintProgram,
        context: OptimizationContext,
    },
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
    pub(super) fn cull_bounds(&self, scale: f32) -> Option<Pixels> {
        if let Self::Fragment { program, .. } = self {
            return (!program.analysis().effects.barrier).then(|| program.bounds());
        }
        matches!(
            self,
            Self::Fill { .. }
                | Self::Text { .. }
                | Self::Image { .. }
                | Self::InsetShadow { .. }
                | Self::Clip { .. }
        )
        .then(|| self.bounds().pixels(scale))
    }

    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Fragment { program, .. } => program.logical_bounds(),
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
                Bounds::new(
                    min[0] - 1.0,
                    min[1] - 1.0,
                    max[0] - min[0] + 2.0,
                    max[1] - min[1] + 2.0,
                )
            }
        }
    }

    pub(super) fn paint(
        &self,
        painter: &mut Painter<'_>,
        scale: f32,
        origin: (i32, i32),
        external: &mut impl FnMut(Key, &mut Painter<'_>),
        compiled: bool,
        cull: bool,
        stats: &mut PaintStats,
        fragments: &mut impl FnMut(&PaintProgram, &OptimizationContext) -> Option<Rc<RasterImage>>,
    ) {
        if cull {
            let invisible = match self {
                Self::Fill { color, .. }
                | Self::Text { color, .. }
                | Self::InsetShadow { color, .. } => color.0[3] == 0,
                _ => false,
            };
            if invisible
                || self.cull_bounds(scale).is_some_and(|bounds| {
                    !painter.intersects(bounds.visible(origin, painter.visible_bounds()))
                })
            {
                stats.culled_commands += 1;
                return;
            }
        }
        stats.replayed_commands += 1;
        match self {
            Self::Fragment { program, context } => {
                if let Some(image) = fragments(program, context) {
                    let bounds = program.bounds();
                    painter.translated((origin.0 + bounds.x, origin.1 + bounds.y), |p| {
                        p.draw_image(&image, 0, 0, bounds.width, bounds.height);
                    });
                    stats.blits += 1;
                    stats.fragment_blits += 1;
                } else {
                    program.replay(painter, origin, external, compiled, cull, stats, fragments);
                }
                return;
            }
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
                    command.paint(
                        painter, scale, origin, external, compiled, cull, stats, fragments,
                    );
                }
                return;
            }
            Self::Clip { bounds, commands } => {
                let pixels = bounds.pixels(scale);
                let clip = pixels.visible(origin, painter.visible_bounds());
                painter.viewport(clip, (0, 0), |painter| {
                    for command in commands.iter() {
                        command.paint(
                            painter, scale, origin, external, compiled, cull, stats, fragments,
                        );
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pixels {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Pixels {
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn translated(self, offset: (i32, i32)) -> Self {
        Self {
            x: self.x.saturating_add(offset.0),
            y: self.y.saturating_add(offset.1),
            ..self
        }
    }

    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: ((i64::from(self.x) + i64::from(self.width))
                .min(i64::from(other.x) + i64::from(other.width))
                - i64::from(x))
            .max(0) as u32,
            height: ((i64::from(self.y) + i64::from(self.height))
                .min(i64::from(other.y) + i64::from(other.height))
                - i64::from(y))
            .max(0) as u32,
        }
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
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
        let left = i64::from(self.x) + i64::from(origin.0);
        let top = i64::from(self.y) + i64::from(origin.1);
        let x = left.max(i64::from(clip.x));
        let y = top.max(i64::from(clip.y));
        let right =
            (left + i64::from(self.width)).min(i64::from(clip.x.saturating_add(clip.width)));
        let bottom =
            (top + i64::from(self.height)).min(i64::from(clip.y.saturating_add(clip.height)));
        Rect {
            x: x as u32,
            y: y as u32,
            width: (right - x).max(0) as u32,
            height: (bottom - y).max(0) as u32,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RasterMode {
    #[default]
    Replay,
    Explicit,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct PaintStats {
    pub indexed_nodes: usize,
    pub indexed_commands_skipped: usize,
    pub fragment_candidates: usize,
    pub prepared_fragments: usize,
    pub fragment_blits: usize,
    pub preparation_blits: usize,
    pub planned_instances: usize,
    pub inspected_groups: usize,
    pub inspected_regions: usize,
    pub rejected_rasters: usize,
    pub preparation_tasks: usize,
    #[cfg(any(test, feature = "benchmark"))]
    pub materialization_ns: u64,
    pub preparation_edges: usize,
    pub failed_preparations: usize,
    pub deferred_preparations: usize,
    pub inspected_instances: usize,
    pub changed_programs: usize,
    pub replayed_commands: usize,
    pub culled_commands: usize,
    pub blits: usize,
    pub prepared_rasters: usize,
    pub repaired_pixels: usize,
    pub copied_pixels: usize,
    pub simplified_commands: usize,
    pub promoted_layers: usize,
    pub demoted_layers: usize,
    pub layer_bytes: usize,
    pub layer_prepare_ns: u64,
}
