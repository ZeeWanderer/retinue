use fontdue::{Font, FontSettings};
use image::RgbaImage;
use std::num::NonZeroU32;

mod blend2d;
mod font;
mod text;

pub use font::{CachedFont, GlyphCache, GlyphMask, LabelFont};
pub use text::{text_width, wrap_text};

/// Blend2D painter borrowing a premultiplied BGRA buffer.
pub struct Painter<'a> {
    context: blend2d::Context<'a>,
    clip: crate::Rect,
    origin: (i64, i64),
}

pub use blend2d::{Error as PainterError, ImageMemory, image_memory};

/// Total concurrency, including the caller; bounded to six and available CPUs.
pub fn render_thread_budget(preferred: u32) -> NonZeroU32 {
    let available = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    bounded_threads(preferred, available)
}

fn bounded_threads(preferred: u32, available: usize) -> NonZeroU32 {
    NonZeroU32::new(preferred.clamp(1, 6).min(available.clamp(1, 6) as u32)).unwrap()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextLine {
    pub x: u32,
    pub y: u32,
    pub height: u32,
}

impl TextLine {
    pub const fn new(x: u32, y: u32, height: u32) -> Self {
        Self { x, y, height }
    }
}

impl TextBox {
    pub const fn new(x: u32, y: u32, width: u32) -> Self {
        Self { x, y, width }
    }
}

impl<'a> Painter<'a> {
    pub fn new(pixels: &'a mut [u8], width: u32, height: u32) -> Result<Self, PainterError> {
        Self::with_threads(pixels, width, height, NonZeroU32::MIN)
    }

    pub fn with_threads(
        pixels: &'a mut [u8],
        width: u32,
        height: u32,
        threads: NonZeroU32,
    ) -> Result<Self, PainterError> {
        Ok(Self {
            context: blend2d::Context::new(pixels, width, height, threads)?,
            clip: crate::Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            origin: (0, 0),
        })
    }

    pub fn thread_count(&self) -> u32 {
        self.context.thread_count()
    }

    pub(crate) fn fail(&mut self, error: PainterError) {
        self.context.fail(error);
    }

    pub fn width(&self) -> u32 {
        self.context.width()
    }

    pub fn height(&self) -> u32 {
        self.context.height()
    }

    pub fn clear(&mut self) {
        self.context.clear_all();
    }

    /// Nonnegative visible bounds in the current local coordinate system.
    pub fn visible_bounds(&self) -> crate::Rect {
        let left = (self.clip.x as i64 - self.origin.0).max(0);
        let top = (self.clip.y as i64 - self.origin.1).max(0);
        let right = (self.clip.x as i64 + self.clip.width as i64 - self.origin.0).max(0);
        let bottom = (self.clip.y as i64 + self.clip.height as i64 - self.origin.1).max(0);
        crate::Rect {
            x: left as u32,
            y: top as u32,
            width: (right - left).max(0) as u32,
            height: (bottom - top).max(0) as u32,
        }
    }

    /// Intersects the current clip in local pixels, including subsequent clears.
    pub fn clip_to_rect(&mut self, rect: crate::Rect) {
        let left = rect.x as i64 + self.origin.0;
        let top = rect.y as i64 + self.origin.1;
        let x = left.max(self.clip.x as i64);
        let y = top.max(self.clip.y as i64);
        let right = (left + rect.width as i64).min(self.clip.x as i64 + self.clip.width as i64);
        let bottom = (top + rect.height as i64).min(self.clip.y as i64 + self.clip.height as i64);
        let clip = crate::Rect {
            x: x as u32,
            y: y as u32,
            width: (right - x).max(0) as u32,
            height: (bottom - y).max(0) as u32,
        };
        if clip != self.clip {
            self.context.clip_to_rect(clip);
            self.clip = clip;
        }
    }

    /// Pixel-aligned scrolling preserves cached glyph sharpness; Blend2D masks
    /// do not support fractional transforms in the pinned renderer.
    pub fn viewport<T>(
        &mut self,
        clip: crate::Rect,
        translation: (i32, i32),
        paint: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let old = (self.origin, self.clip);
        self.clip_to_rect(clip);
        self.origin.0 += translation.0 as i64;
        self.origin.1 += translation.1 as i64;
        let result = paint(self);
        self.restore_scope(old);
        result
    }

    pub fn translated<T>(
        &mut self,
        translation: (i32, i32),
        paint: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let old = (self.origin, self.clip);
        self.origin.0 += translation.0 as i64;
        self.origin.1 += translation.1 as i64;
        let result = paint(self);
        self.restore_scope(old);
        result
    }

    fn restore_scope(&mut self, state: ((i64, i64), crate::Rect)) {
        // All other draw state is explicit in each backend operation.
        if self.clip != state.1 {
            self.context.set_clip_rect(state.1);
        }
        (self.origin, self.clip) = state;
    }

    /// Draw in a clipped local rectangle; the backend receives host-space coordinates.
    pub fn local<T>(
        &mut self,
        bounds: crate::Rect,
        paint: impl FnOnce(&mut Self, crate::Rect) -> T,
    ) -> T {
        self.viewport(bounds, (bounds.x as i32, bounds.y as i32), |p| {
            paint(
                p,
                crate::Rect {
                    x: 0,
                    y: 0,
                    ..bounds
                },
            )
        })
    }

    fn host_rect(&self, rect: crate::Rect) -> blend2d::Rect {
        blend2d::Rect {
            x: rect.x as i64 + self.origin.0,
            y: rect.y as i64 + self.origin.1,
            width: rect.width,
            height: rect.height,
        }
    }

    pub fn fill_rounded_rect(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        radius: u32,
        color: [u8; 4],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let radius = radius.min(width / 2).min(height / 2);
        self.context.fill_round_rect_rgba32(
            f64::from(x) + self.origin.0 as f64,
            f64::from(y) + self.origin.1 as f64,
            f64::from(width),
            f64::from(height),
            f64::from(radius),
            blend2d_rgba32(color),
        );
    }

    #[cfg(test)]
    pub fn fill_panel(&mut self, radius: u32, color: [u8; 4]) {
        self.fill_rounded_rect(0, 0, self.width(), self.height(), radius, color);
    }

    pub fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), width: f64, color: [u8; 4]) {
        let offset = |(x, y)| (x + self.origin.0 as f64, y + self.origin.1 as f64);
        self.context
            .stroke_line(offset(from), offset(to), width, blend2d_rgba32(color));
    }
    pub fn fill_polygon(&mut self, points: &[[f64; 2]], color: [u8; 4]) {
        if self.origin == (0, 0) {
            self.context.fill_polygon(points, blend2d_rgba32(color));
        } else {
            let points: Vec<_> = points
                .iter()
                .map(|p| [p[0] + self.origin.0 as f64, p[1] + self.origin.1 as f64])
                .collect();
            self.context.fill_polygon(&points, blend2d_rgba32(color));
        }
    }

    pub fn outline(&mut self, rect: crate::Rect, radius: u32, stroke: u32, color: [u8; 4]) {
        let rect = self.host_rect(rect);
        self.context
            .stroke_round_rect(rect, radius, stroke, blend2d_rgba32(color));
    }

    /// Zero-offset inset glow, approximated by Gaussian-weighted vector bands.
    pub fn inset_shadow(
        &mut self,
        rect: crate::Rect,
        radius: u32,
        blur: f32,
        spread: f32,
        color: [u8; 4],
    ) {
        if rect.is_empty() || color[3] == 0 || !blur.is_finite() || !spread.is_finite() {
            return;
        }
        let sigma = f64::from(blur.max(0.0)) / 2.0;
        let spread = f64::from(spread.max(0.0));
        let bands = ((spread + 3.0 * sigma).ceil() as u32).min(rect.width.min(rect.height) / 2);
        for depth in 0..bands {
            let distance = f64::from(depth) + 0.5 - spread;
            let coverage = if sigma > 0.0 {
                // Abramowitz-Stegun 7.1.26, erfc(x / sqrt(2)) / 2.
                let x = distance / (sigma * std::f64::consts::SQRT_2);
                let t = 1.0 / (1.0 + 0.3275911 * x.abs());
                let tail =
                    (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
                        + 0.254829592)
                        * t
                        * (-x * x).exp()
                        / 2.0;
                if x < 0.0 { 1.0 - tail } else { tail }
            } else {
                if distance < 0.0 { 1.0 } else { 0.0 }
            };
            let alpha = (f64::from(color[3]) * coverage).round() as u8;
            if alpha != 0 {
                self.outline(
                    rect.inset(depth),
                    radius.saturating_sub(depth),
                    1,
                    [color[0], color[1], color[2], alpha],
                );
            }
        }
    }

    pub fn draw_ring(
        &mut self,
        center_x: u32,
        center_y: u32,
        radius: u32,
        stroke: u32,
        color: [u8; 4],
    ) {
        if stroke == 0 {
            return;
        }
        self.context.stroke_circle_rgba32(
            f64::from(center_x) + self.origin.0 as f64,
            f64::from(center_y) + self.origin.1 as f64,
            f64::from(radius),
            f64::from(stroke),
            blend2d_rgba32(color),
        );
    }

    pub fn draw_image(&mut self, image: &RasterImage, x: u32, y: u32, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.context.blit_scaled_prgb32(
            &image.native,
            self.host_rect(crate::Rect {
                x,
                y,
                width,
                height,
            }),
        );
    }

    pub fn draw_image_contained(
        &mut self,
        image: &RasterImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) {
        let (draw_width, draw_height) =
            contained_size(image.native.width, image.native.height, width, height);
        self.draw_image(
            image,
            x + width.saturating_sub(draw_width) / 2,
            y + height.saturating_sub(draw_height) / 2,
            draw_width,
            draw_height,
        );
    }

    pub fn draw_image_covered(&mut self, image: &RasterImage, bounds: crate::Rect) {
        if bounds.width == 0 || bounds.height == 0 {
            return;
        }
        let (width, height) = image.dimensions();
        let scale = (bounds.width as f64 / width as f64).max(bounds.height as f64 / height as f64);
        let width = (width as f64 * scale).ceil() as u32;
        let height = (height as f64 * scale).ceil() as u32;
        self.viewport(
            bounds,
            (
                bounds.x as i32 - (width as i32 - bounds.width as i32) / 2,
                bounds.y as i32 - (height as i32 - bounds.height as i32) / 2,
            ),
            |p| p.draw_image(image, 0, 0, width, height),
        );
    }

    pub fn draw_image_contained_circle(
        &mut self,
        image: &RasterImage,
        x: u32,
        y: u32,
        diameter: u32,
    ) {
        if diameter == 0 {
            return;
        }
        let (draw_width, draw_height) =
            contained_size(image.native.width, image.native.height, diameter, diameter);
        self.context.fill_circle_prgb32(
            &image.native,
            self.host_rect(crate::Rect {
                x: x + diameter.saturating_sub(draw_width) / 2,
                y: y + diameter.saturating_sub(draw_height) / 2,
                width: draw_width,
                height: draw_height,
            }),
            blend2d::Circle {
                x: f64::from(x) + f64::from(diameter) / 2.0 + self.origin.0 as f64,
                y: f64::from(y) + f64::from(diameter) / 2.0 + self.origin.1 as f64,
                radius: f64::from(diameter) / 2.0,
            },
        );
    }

    pub fn draw_text(
        &mut self,
        font: &impl LabelFont,
        x: u32,
        y: u32,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.face().horizontal_line_metrics(size) else {
            return;
        };
        for (index, line) in text.split('\n').enumerate() {
            self.draw_glyph_line(
                font,
                (
                    x as f32,
                    y as f32 + metrics.ascent + index as f32 * metrics.new_line_size,
                ),
                size,
                line,
                color,
            );
        }
    }

    pub fn draw_text_vertically_centered(
        &mut self,
        font: &impl LabelFont,
        line: TextLine,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.face().horizontal_line_metrics(size) else {
            return;
        };
        let extra_lines = text.bytes().filter(|&c| c == b'\n').count() as f32;
        let baseline = line.y as f32
            + (line.height as f32 - metrics.ascent + metrics.descent
                - extra_lines * metrics.new_line_size)
                / 2.0
            + metrics.ascent;
        for (index, text) in text.split('\n').enumerate() {
            self.draw_glyph_line(
                font,
                (
                    line.x as f32,
                    baseline + index as f32 * metrics.new_line_size,
                ),
                size,
                text,
                color,
            );
        }
    }

    pub fn draw_centered_text(
        &mut self,
        font: &impl LabelFont,
        bounds: TextBox,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.face().horizontal_line_metrics(size) else {
            return;
        };
        for (index, line) in wrap_text(font.face(), text, size, bounds.width as f32)
            .iter()
            .enumerate()
        {
            let x =
                bounds.x as f32 + (bounds.width as f32 - text_width(font.face(), line, size)) / 2.0;
            let baseline = bounds.y as f32 + metrics.ascent + index as f32 * metrics.new_line_size;
            self.draw_glyph_line(font, (x, baseline), size, line, color);
        }
    }

    pub fn finish(self) -> Result<(), PainterError> {
        self.context.finish()
    }

    fn draw_glyph_line(
        &mut self,
        font: &impl LabelFont,
        origin: (f32, f32),
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let (mut x, baseline) = origin;
        let mut previous = None;
        for character in text.chars().filter(|c| !c.is_control()) {
            if self.context.failed() {
                return;
            }
            if let Some(previous) = previous {
                x += font
                    .face()
                    .horizontal_kern(previous, character, size)
                    .unwrap_or(0.0);
            }
            let result = font.with_glyph(character, size, |metrics, mask| {
                if let Some(mask) = mask {
                    self.context.fill_a8_mask_rgba32(
                        &mask.0,
                        (x + metrics.xmin as f32).round() as i64 + self.origin.0,
                        (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i64
                            + self.origin.1,
                        blend2d_rgba32(color),
                    );
                }
                x += metrics.advance_width;
            });
            if let Err(error) = result {
                self.fail(error);
                return;
            }
            previous = Some(character);
        }
    }

    #[cfg(test)]
    fn draw_mask(
        &mut self,
        coverage: &[u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let mask =
            blend2d::Image::new(coverage.to_vec(), width as u32, height as u32, true).unwrap();
        self.context.fill_a8_mask_rgba32(
            &mask,
            x as i64 + self.origin.0,
            y as i64 + self.origin.1,
            blend2d_rgba32(color),
        );
    }
}

#[derive(Debug)]
pub struct RasterImage {
    native: blend2d::Image,
}

impl crate::resources::ResourceSize for RasterImage {
    fn heap_bytes(&self) -> usize {
        self.native.heap_bytes()
    }
}

impl RasterImage {
    /// Paint a reusable, immutable image once; callers own its cache lifetime.
    pub fn render(
        width: u32,
        height: u32,
        paint: impl FnOnce(&mut Painter<'_>),
    ) -> Result<Self, PainterError> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(PainterError::invalid("rendered image dimensions"));
        }
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let mut painter = Painter::new(&mut pixels, width, height)?;
        paint(&mut painter);
        painter.finish()?;
        Ok(Self {
            native: blend2d::Image::new(pixels, width, height, false)?,
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.native.width, self.native.height)
    }

    pub fn resampled(&self, width: u32, height: u32) -> Result<Self, PainterError> {
        Ok(Self {
            native: self.native.resampled(width, height)?,
        })
    }

    pub fn byte_len(&self) -> usize {
        self.native.byte_len()
    }

    pub fn to_rgba(&self) -> Result<image::RgbaImage, String> {
        straight_rgba(self.native.pixels(), self.native.width, self.native.height)
    }

    pub fn from_rgba(image: image::RgbaImage) -> Result<Self, PainterError> {
        let (width, height) = image.dimensions();
        let mut pixels = image.into_raw();
        for pixel in pixels.chunks_exact_mut(4) {
            let [red, green, blue, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            pixel.copy_from_slice(&[
                premultiply(blue, alpha),
                premultiply(green, alpha),
                premultiply(red, alpha),
                alpha,
            ]);
        }
        Ok(Self {
            native: blend2d::Image::new(pixels, width, height, false)?,
        })
    }
}

pub fn fit_text_size(font: &Font, text: &str, preferred: f32, minimum: f32, max_width: u32) -> f32 {
    let mut size = preferred;
    while size > minimum && text_width(font, text, size) > max_width as f32 {
        size -= 0.5;
    }
    size
}

pub fn load_icon(bytes: &[u8]) -> Result<RasterImage, Box<dyn std::error::Error>> {
    Ok(RasterImage::from_rgba(
        image::load_from_memory(bytes)?.into_rgba8(),
    )?)
}

pub fn load_scene_icon(
    path: &str,
    budget: usize,
) -> Result<RasterImage, Box<dyn std::error::Error>> {
    use image::ImageDecoder;
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) * 4 > budget as u64 {
        return Err("image exceeds decoded asset budget".into());
    }
    Ok(RasterImage::from_rgba(
        image::DynamicImage::from_decoder(decoder)?.into_rgba8(),
    )?)
}

pub fn load_overlay_font(cache: &GlyphCache) -> Result<CachedFont, Box<dyn std::error::Error>> {
    CachedFont::from_bytes(
        include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf").as_slice(),
        FontSettings::default(),
        cache,
    )
    .map_err(Into::into)
}

/// Convert conventional CSS/RGBA channel order to little-endian ARGB8888 SHM bytes.
pub const fn css_rgba(red: u8, green: u8, blue: u8, alpha: u8) -> [u8; 4] {
    [blue, green, red, alpha]
}

pub fn straight_rgba(source: &[u8], width: u32, height: u32) -> Result<RgbaImage, String> {
    let mut destination = Vec::with_capacity(source.len());
    for pixel in source.chunks_exact(4) {
        let alpha = u32::from(pixel[3]);
        for channel in [pixel[2], pixel[1], pixel[0]] {
            let value = (u32::from(channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
            destination.push(value);
        }
        destination.push(pixel[3]);
    }
    RgbaImage::from_raw(width, height, destination)
        .ok_or_else(|| "overlay preview buffer has invalid dimensions".to_owned())
}

const fn blend2d_rgba32(color: [u8; 4]) -> u32 {
    let [blue, green, red, alpha] = color;
    u32::from_be_bytes([alpha, red, green, blue])
}

const fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}

fn contained_size(source_width: u32, source_height: u32, width: u32, height: u32) -> (u32, u32) {
    if source_width == 0 || source_height == 0 || width == 0 || height == 0 {
        return (0, 0);
    }
    if u64::from(source_width) * u64::from(height) > u64::from(source_height) * u64::from(width) {
        let height = ((u128::from(source_height) * u128::from(width)
            + u128::from(source_width) / 2)
            / u128::from(source_width))
        .max(1) as u32;
        (width, height)
    } else {
        let width = ((u128::from(source_width) * u128::from(height)
            + u128::from(source_height) / 2)
            / u128::from(source_height))
        .max(1) as u32;
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_lowering_matches_native_transforms_for_every_primitive() {
        let image = RasterImage::from_rgba(image::RgbaImage::from_fn(17, 13, |x, y| {
            image::Rgba([(x * 13) as u8, (y * 19) as u8, 120, 195])
        }))
        .unwrap();
        let font = load_overlay_font(&GlyphCache::new(8192)).unwrap();
        let draw = |p: &mut Painter<'_>, native: bool| {
            p.fill_rounded_rect(0, 0, 190, 140, 9, [25, 40, 80, 210]);
            p.outline(
                crate::Rect {
                    x: 3,
                    y: 4,
                    width: 160,
                    height: 130,
                },
                7,
                2,
                [220, 40, 50, 255],
            );
            p.draw_line((0.25, 8.5), (175.5, 11.75), 1.5, [255, 230, 100, 255]);
            p.fill_polygon(
                &[[9.5, 12.25], [30.0, 14.0], [18.25, 43.5]],
                [80, 220, 90, 160],
            );
            p.draw_ring(49, 42, 13, 3, [20, 180, 240, 255]);
            p.draw_image(&image, 2, 58, 30, 19);
            p.draw_image_contained(&image, 38, 58, 30, 30);
            p.draw_image_contained_circle(&image, 75, 58, 30);
            let cover = crate::Rect {
                x: 114,
                y: 58,
                width: 24,
                height: 30,
            };
            if native {
                p.context.save();
                p.context.clip_to_rect(cover);
                p.context.translate(106.0, 58.0);
                p.draw_image(&image, 0, 0, 40, 30);
                p.context.restore();
            } else {
                p.draw_image_covered(&image, cover);
            }
            p.inset_shadow(
                crate::Rect {
                    x: 20,
                    y: 92,
                    width: 110,
                    height: 36,
                },
                8,
                4.0,
                1.0,
                [90, 230, 40, 255],
            );
            p.draw_text(&font, 17, 93, 13.25, "Blueprint", [240, 240, 240, 255]);
            p.draw_text_vertically_centered(
                &font,
                TextLine::new(135, 92, 30),
                12.5,
                "Owned",
                [190, 255, 180, 255],
            );
        };
        for threads in [1, 4] {
            for origin in [(-9, -7), (0, 0), (17, 23)] {
                let mut actual = vec![0; 220 * 180 * 4];
                let mut expected = actual.clone();
                let clip = crate::Rect {
                    x: 5,
                    y: 6,
                    width: 190,
                    height: 160,
                };
                let mut p =
                    Painter::with_threads(&mut actual, 220, 180, NonZeroU32::new(threads).unwrap())
                        .unwrap();
                p.viewport(clip, origin, |p| draw(p, false));
                p.finish().unwrap();
                let mut p = Painter::with_threads(
                    &mut expected,
                    220,
                    180,
                    NonZeroU32::new(threads).unwrap(),
                )
                .unwrap();
                p.context.clip_to_rect(clip);
                p.context.translate(origin.0 as f64, origin.1 as f64);
                draw(&mut p, true);
                p.finish().unwrap();
                let max = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert_eq!(max, 0, "origin={origin:?}, threads={threads}");
            }
        }
    }

    #[test]
    fn local_scope_exposes_only_its_own_rectangle() {
        let mut pixels = vec![0; 100 * 100 * 4];
        let mut p = Painter::new(&mut pixels, 100, 100).unwrap();
        let rect = crate::Rect {
            x: 20,
            y: 30,
            width: 40,
            height: 50,
        };
        p.local(rect, |p, local| {
            assert_eq!(
                local,
                crate::Rect {
                    x: 0,
                    y: 0,
                    width: 40,
                    height: 50
                }
            );
            assert_eq!(p.visible_bounds(), local);
            p.fill_rounded_rect(0, 0, local.width, local.height, 0, [10, 20, 30, 255]);
        });
        assert_eq!(p.origin, (0, 0));
        p.finish().unwrap();
        assert_eq!(
            &pixels[(30 * 100 + 20) * 4..(30 * 100 + 20) * 4 + 4],
            &[10, 20, 30, 255]
        );
        assert_eq!(
            &pixels[(29 * 100 + 20) * 4..(29 * 100 + 20) * 4 + 4],
            &[0, 0, 0, 0]
        );
    }

    #[test]
    fn visible_bounds_follow_nested_transforms_and_damage_clips() {
        use crate::Rect;
        let mut pixels = vec![0; 100 * 100 * 4];
        let mut p = Painter::new(&mut pixels, 100, 100).unwrap();
        let repair = Rect {
            x: 20,
            y: 30,
            width: 40,
            height: 20,
        };
        p.clip_to_rect(repair);
        assert_eq!(p.visible_bounds(), repair);
        p.viewport(
            Rect {
                x: 10,
                y: 10,
                width: 90,
                height: 90,
            },
            (10, 10),
            |p| {
                assert_eq!(
                    p.visible_bounds(),
                    Rect {
                        x: 10,
                        y: 20,
                        width: 40,
                        height: 20
                    }
                );
                p.translated((0, -100), |p| {
                    assert_eq!(
                        p.visible_bounds(),
                        Rect {
                            x: 10,
                            y: 120,
                            width: 40,
                            height: 20
                        }
                    );
                    p.clip_to_rect(Rect {
                        x: 0,
                        y: 130,
                        width: 100,
                        height: 5,
                    });
                    assert_eq!(
                        p.visible_bounds(),
                        Rect {
                            x: 10,
                            y: 130,
                            width: 40,
                            height: 5
                        }
                    );
                });
                assert_eq!(
                    p.visible_bounds(),
                    Rect {
                        x: 10,
                        y: 20,
                        width: 40,
                        height: 20
                    }
                );
            },
        );
        assert_eq!(p.visible_bounds(), repair);
        p.finish().unwrap();
    }

    #[test]
    fn reusable_render_keeps_transparency_and_premultiplied_pixels() {
        for (width, height) in [(0, 4), (4, 0), (8193, 1), (1, u32::MAX)] {
            assert!(RasterImage::render(width, height, |_| panic!("invalid size")).is_err());
        }
        let paint = |p: &mut Painter<'_>| {
            p.fill_rounded_rect(3, 4, 17, 13, 5, [30, 90, 170, 137]);
        };
        let image = RasterImage::render(32, 24, paint).unwrap();
        let mut expected = vec![0; 32 * 24 * 4];
        let mut p = Painter::new(&mut expected, 32, 24).unwrap();
        paint(&mut p);
        p.finish().unwrap();
        let mut actual = vec![0; expected.len()];
        let mut p = Painter::new(&mut actual, 32, 24).unwrap();
        p.draw_image(&image, 0, 0, 32, 24);
        p.finish().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn clipped_clear_and_translucent_layers_match_full_paint() {
        use crate::Rect;
        let size = (160, 96);
        let clip = Rect {
            x: 17,
            y: 29,
            width: 93,
            height: 50,
        };
        for threads in [1, 4] {
            let mut full = vec![0; size.0 * size.1 * 4];
            let mut partial = vec![137; full.len()];
            for (pixels, clipped) in [(&mut full, false), (&mut partial, true)] {
                let mut p = Painter::with_threads(
                    pixels,
                    size.0 as u32,
                    size.1 as u32,
                    NonZeroU32::new(threads).unwrap(),
                )
                .unwrap();
                if clipped {
                    p.clip_to_rect(clip);
                }
                p.clear();
                p.fill_rounded_rect(2, 5, 120, 85, 18, [35, 55, 85, 190]);
                p.fill_rounded_rect(30, 40, 125, 55, 24, [90, 75, 25, 160]);
                p.finish().unwrap();
            }
            for (index, (actual, expected)) in partial
                .chunks_exact(4)
                .zip(full.chunks_exact(4))
                .enumerate()
            {
                let inside = clip.contains((index % size.0) as f64, (index / size.0) as f64);
                assert_eq!(actual, if inside { expected } else { &[137; 4] });
            }
        }
    }

    #[test]
    fn redundant_viewports_still_isolate_inner_clip_changes() {
        for threads in [1, 4] {
            let mut actual = vec![0; 40 * 30 * 4];
            let mut expected = actual.clone();
            let outer = crate::Rect {
                x: 3,
                y: 4,
                width: 30,
                height: 20,
            };
            let inner = crate::Rect {
                x: 7,
                y: 8,
                width: 10,
                height: 9,
            };
            for (pixels, nested) in [(&mut actual, true), (&mut expected, false)] {
                let mut p =
                    Painter::with_threads(pixels, 40, 30, NonZeroU32::new(threads).unwrap())
                        .unwrap();
                p.clip_to_rect(outer);
                if nested {
                    p.viewport(outer, (0, 0), |p| {
                        p.translated((0, 0), |p| {
                            p.clip_to_rect(outer);
                            p.clip_to_rect(inner);
                            p.fill_panel(0, [10, 30, 50, 255]);
                        })
                    });
                } else {
                    p.fill_rounded_rect(
                        inner.x,
                        inner.y,
                        inner.width,
                        inner.height,
                        0,
                        [10, 30, 50, 255],
                    );
                }
                assert_eq!(p.visible_bounds(), outer);
                p.fill_panel(0, [70, 60, 30, 100]);
                p.finish().unwrap();
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn scrolled_viewports_restore_transforms_and_keep_outer_damage_clip() {
        let mut pixels = vec![0_u8; 20 * 20 * 4];
        let mut p =
            Painter::with_threads(&mut pixels, 20, 20, NonZeroU32::new(2).unwrap()).unwrap();
        p.clip_to_rect(crate::Rect {
            x: 4,
            y: 4,
            width: 10,
            height: 10,
        });
        p.viewport(
            crate::Rect {
                x: 2,
                y: 2,
                width: 14,
                height: 14,
            },
            (0, -30),
            |p| {
                p.viewport(
                    crate::Rect {
                        x: 0,
                        y: 35,
                        width: 20,
                        height: 5,
                    },
                    (0, 0),
                    |p| {
                        p.fill_rounded_rect(0, 35, 20, 5, 0, [255, 0, 0, 255]);
                    },
                );
            },
        );
        p.fill_rounded_rect(4, 11, 10, 1, 0, [0, 255, 0, 255]);
        p.finish().unwrap();
        for y in 0..20 {
            for x in 0..20 {
                let expected = if (4..14).contains(&x) && (5..10).contains(&y) {
                    [255, 0, 0, 255]
                } else if (4..14).contains(&x) && y == 11 {
                    [0, 255, 0, 255]
                } else {
                    [0; 4]
                };
                assert_eq!(
                    &pixels[(y * 20 + x) * 4..(y * 20 + x + 1) * 4],
                    &expected,
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn empty_and_nested_clips_preserve_untouched_pixels() {
        use crate::Rect;
        let mut pixels = vec![137; 64];
        let mut p = Painter::new(&mut pixels, 4, 4).unwrap();
        p.clip_to_rect(Rect {
            x: u32::MAX,
            y: 1,
            width: 4,
            height: 4,
        });
        p.clear();
        p.fill_panel(0, [0, 0, 0, 255]);
        p.finish().unwrap();
        assert_eq!(pixels, vec![137; 64]);
        let mut p = Painter::new(&mut pixels, 4, 4).unwrap();
        p.clip_to_rect(Rect {
            x: 0,
            y: 0,
            width: 3,
            height: 3,
        });
        p.clip_to_rect(Rect {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        });
        p.clear();
        p.finish().unwrap();
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(pixel, if index == 10 { &[0; 4] } else { &[137; 4] });
        }
    }

    #[test]
    fn clipped_rounded_edge_has_bounded_coverage_rounding() {
        let size = (120, 80);
        let clip = crate::Rect {
            x: 0,
            y: 0,
            width: 22,
            height: 80,
        };
        let mut full = vec![0; size.0 * size.1 * 4];
        let mut partial = full.clone();
        for (pixels, clipped) in [(&mut full, false), (&mut partial, true)] {
            let mut p = Painter::new(pixels, size.0 as u32, size.1 as u32).unwrap();
            if clipped {
                p.clip_to_rect(clip);
            }
            p.clear();
            p.fill_panel(0, [22, 21, 18, 255]);
            p.fill_rounded_rect(20, 20, 70, 25, 5, [86, 83, 68, 255]);
            p.finish().unwrap();
        }
        for y in 0..size.1 {
            for x in 0..clip.width as usize {
                let offset = (y * size.0 + x) * 4;
                for c in 0..4 {
                    let difference = partial[offset + c].abs_diff(full[offset + c]);
                    assert!(
                        difference <= u8::from(x + 1 == clip.width as usize),
                        "coverage at {x},{y}: difference {difference}"
                    );
                }
            }
        }
    }

    #[test]
    fn concurrency_budget_respects_cpu_availability_and_cap() {
        for (preferred, available, expected) in [
            (4, 1, 1),
            (4, 2, 2),
            (2, 16, 2),
            (4, 16, 4),
            (6, 4, 4),
            (6, 16, 6),
            (8, 16, 6),
            (0, 16, 1),
            (4, 0, 1),
        ] {
            assert_eq!(bounded_threads(preferred, available).get(), expected);
        }
    }

    #[test]
    fn default_painter_queues_without_extra_workers() {
        let mut pixels = [0; 4];
        let painter = Painter::new(&mut pixels, 1, 1).unwrap();
        assert_eq!(painter.thread_count(), 1);
        painter.finish().unwrap();
    }

    #[test]
    fn loads_webp_assets_from_daemon_cache() {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(&[10, 20, 30, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();

        let image = load_icon(&encoded).unwrap();
        assert_eq!((image.native.width, image.native.height), (1, 1));
        let mut pixels = [0; 4];
        let mut painter = Painter::new(&mut pixels, 1, 1).unwrap();
        painter.draw_image(&image, 0, 0, 1, 1);
        painter.finish().unwrap();
        assert_eq!(pixels, [30, 20, 10, 255]);
    }

    #[test]
    fn rounded_panel_antialiases_corners() {
        let mut pixels = vec![0; 8 * 8 * 4];
        let color = [1, 2, 3, 255];
        let mut painter = Painter::new(&mut pixels, 8, 8).unwrap();
        painter.fill_panel(2, color);
        painter.finish().unwrap();

        assert!(pixel(&pixels, 8, 0, 0)[3] < 128);
        assert!(pixel(&pixels, 8, 7, 0)[3] < 128);
        assert!(pixel(&pixels, 8, 2, 0)[3] > 128);
        assert_eq!(pixel(&pixels, 8, 4, 4), color);
    }

    #[test]
    fn oversized_radius_is_clamped() {
        let mut pixels = vec![0; 4 * 2 * 4];
        let mut painter = Painter::new(&mut pixels, 4, 2).unwrap();
        painter.fill_panel(u32::MAX, [1, 2, 3, 255]);
        painter.finish().unwrap();
    }

    #[test]
    fn glyph_mask_blends_over_panel() {
        let mut pixels = vec![0; 4];
        let mut painter = Painter::new(&mut pixels, 1, 1).unwrap();
        painter.fill_panel(0, [10, 20, 30, 200]);
        painter.draw_mask(&[128], 1, 1, 0, 0, [110, 120, 130, 255]);
        painter.finish().unwrap();

        assert!((226..=229).contains(&pixels[3]));
        assert!(pixels[0] < pixels[1]);
        assert!(pixels[1] < pixels[2]);
    }

    #[test]
    fn circular_image_keeps_wide_texture_centered() {
        let image = RasterImage {
            native: blend2d::Image::new(vec![255; 8 * 4 * 4], 8, 4, false).unwrap(),
        };
        let mut pixels = vec![0; 8 * 8 * 4];
        let mut painter = Painter::new(&mut pixels, 8, 8).unwrap();
        painter.draw_image_contained_circle(&image, 0, 0, 8);
        painter.finish().unwrap();

        assert_eq!(pixel(&pixels, 8, 0, 0)[3], 0);
        assert_eq!(pixel(&pixels, 8, 4, 1)[3], 0);
        assert_eq!(pixel(&pixels, 8, 4, 4)[3], 255);
    }

    #[test]
    fn blend2d_writes_into_borrowed_buffer() {
        let mut pixels = vec![0; 4 * 4 * 4];
        let mut painter = Painter::new(&mut pixels, 4, 4).unwrap();
        painter.fill_panel(0, css_rgba(16, 22, 35, 255));
        painter.finish().unwrap();

        assert_eq!(pixel(&pixels, 4, 2, 2), [35, 22, 16, 255]);
    }

    #[test]
    fn contained_image_preserves_aspect_ratio_and_centers() {
        let image = RasterImage {
            native: blend2d::Image::new(vec![255; 4 * 2 * 4], 4, 2, false).unwrap(),
        };
        let mut pixels = vec![0; 8 * 8 * 4];
        let mut painter = Painter::new(&mut pixels, 8, 8).unwrap();
        painter.draw_image_contained(&image, 0, 0, 8, 8);
        painter.finish().unwrap();

        assert_eq!(pixel(&pixels, 8, 4, 1)[3], 0);
        assert_eq!(pixel(&pixels, 8, 4, 2)[3], 255);
        assert_eq!(pixel(&pixels, 8, 4, 5)[3], 255);
        assert_eq!(pixel(&pixels, 8, 4, 6)[3], 0);
    }

    #[test]
    fn covered_image_crops_padding_and_respects_viewport() {
        let mut rgba = image::RgbaImage::new(8, 16);
        for y in 4..12 {
            for x in 0..8 {
                rgba.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let image = RasterImage::from_rgba(rgba).unwrap();
        let mut pixels = vec![0; 12 * 12 * 4];
        let mut painter = Painter::new(&mut pixels, 12, 12).unwrap();
        painter.draw_image_covered(
            &image,
            crate::Rect {
                x: 2,
                y: 2,
                width: 8,
                height: 8,
            },
        );
        painter.finish().unwrap();
        assert_eq!(pixel(&pixels, 12, 6, 2), [0, 0, 255, 255]);
        assert_eq!(pixel(&pixels, 12, 6, 9), [0, 0, 255, 255]);
        assert_eq!(pixel(&pixels, 12, 1, 6)[3], 0);
        assert_eq!(pixel(&pixels, 12, 6, 10)[3], 0);
    }

    #[test]
    fn currency_icons_match_reference_contain_sizes() {
        assert_eq!(contained_size(333, 285, 28, 28), (28, 24));
        assert_eq!(contained_size(64, 64, 30, 30), (30, 30));
    }

    #[test]
    fn contained_image_downsampling_has_smooth_alpha_edges() {
        let image = RasterImage {
            native: blend2d::Image::new(vec![0, 0, 0, 0, 255, 255, 255, 255], 2, 1, false).unwrap(),
        };
        let mut pixels = vec![0; 4 * 2 * 4];
        let mut painter = Painter::new(&mut pixels, 4, 2).unwrap();
        painter.draw_image_contained(&image, 0, 0, 4, 2);
        painter.finish().unwrap();

        assert!(pixels.chunks_exact(4).any(|pixel| {
            let alpha = pixel[3];
            alpha > 0 && alpha < 255
        }));
    }

    #[test]
    fn bundled_font_matches_aleca_line_height() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        let metrics = font.horizontal_line_metrics(19.0).unwrap();

        assert_eq!(metrics.new_line_size.ceil(), 23.0);
    }

    #[test]
    fn measured_text_fits_one_line_at_fractional_scales() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        let text = "While this Gem is Socketed you can view Monster Categories on Enemies";
        for size in [15.0, 18.0, 18.75, 22.5] {
            assert_eq!(
                wrap_text(&font, text, size, text_width(&font, text, size)),
                [text]
            );
        }
    }

    #[test]
    fn rendered_glyph_positions_do_not_accumulate_rounding() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        for size in [14.4, 18.0, 19.0, 23.75] {
            let mut pixels = vec![0; 400 * 60 * 4];
            let mut painter = Painter::new(&mut pixels, 400, 60).unwrap();
            painter.draw_text(&font, 10, 5, size, "iiiiiiiiiiiiiiiiiiii", [255; 4]);
            painter.finish().unwrap();
            let mut columns = pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, pixel)| pixel[3] != 0)
                .map(|(index, _)| index % 400)
                .collect::<Vec<_>>();
            columns.sort_unstable();
            let span = (columns.last().unwrap() - columns.first().unwrap()) as f32;
            let metrics = font.metrics('i', size);
            let expected = 19.0 * metrics.advance_width + metrics.width as f32 - 1.0;
            assert!(
                (span - expected).abs() <= 2.0,
                "{size}: {span} vs {expected}"
            );
        }
    }

    #[test]
    fn css_colors_are_converted_to_shm_byte_order() {
        assert_eq!(css_rgba(16, 22, 35, 255), [35, 22, 16, 255]);
        assert_eq!(blend2d_rgba32([35, 22, 16, 255]), 0xFF10_1623);
    }

    #[test]
    fn premultiplied_bgra_converts_to_straight_rgba() {
        let image = straight_rgba(&[25, 50, 100, 128, 0, 0, 0, 0], 2, 1).unwrap();
        assert_eq!(image.as_raw(), &[199, 100, 50, 128, 0, 0, 0, 0]);
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        pixels[offset..offset + 4].try_into().unwrap()
    }
}
