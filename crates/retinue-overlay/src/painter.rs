use fontdue::{Font, FontSettings};
use image::RgbaImage;

mod blend2d;
mod text;

pub use text::{text_width, wrap_text};

/// Blend2D painter borrowing a premultiplied BGRA Wayland SHM buffer.
///
/// Blend2D runs synchronously here, so temporary icon and glyph-mask buffers only
/// need to remain alive for their individual draw calls.
pub struct Painter<'a> {
    context: blend2d::Context<'a>,
}

pub use blend2d::Error as PainterError;

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
        Ok(Self {
            context: blend2d::Context::new(pixels, width, height)?,
        })
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
            f64::from(x),
            f64::from(y),
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
        self.context
            .stroke_line(from, to, width, blend2d_rgba32(color));
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
            f64::from(center_x),
            f64::from(center_y),
            f64::from(radius),
            f64::from(stroke),
            blend2d_rgba32(color),
        );
    }

    pub fn draw_image(&mut self, image: &RasterImage, x: u32, y: u32, width: u32, height: u32) {
        if width == 0 || height == 0 || image.width == 0 || image.height == 0 {
            return;
        }
        self.context.blit_scaled_prgb32(
            blend2d::Image {
                pixels: &image.pixels,
                width: image.width,
                height: image.height,
            },
            blend2d::Rect {
                x,
                y,
                width,
                height,
            },
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
        let (draw_width, draw_height) = contained_size(image.width, image.height, width, height);
        self.draw_image(
            image,
            x + width.saturating_sub(draw_width) / 2,
            y + height.saturating_sub(draw_height) / 2,
            draw_width,
            draw_height,
        );
    }

    pub fn draw_image_contained_circle(
        &mut self,
        image: &RasterImage,
        x: u32,
        y: u32,
        diameter: u32,
    ) {
        if diameter == 0 || image.width == 0 || image.height == 0 {
            return;
        }
        let (draw_width, draw_height) =
            contained_size(image.width, image.height, diameter, diameter);
        self.context.fill_circle_prgb32(
            blend2d::Image {
                pixels: &image.pixels,
                width: image.width,
                height: image.height,
            },
            blend2d::Rect {
                x: x + diameter.saturating_sub(draw_width) / 2,
                y: y + diameter.saturating_sub(draw_height) / 2,
                width: draw_width,
                height: draw_height,
            },
            blend2d::Circle {
                x: f64::from(x) + f64::from(diameter) / 2.0,
                y: f64::from(y) + f64::from(diameter) / 2.0,
                radius: f64::from(diameter) / 2.0,
            },
        );
    }

    pub fn draw_text(
        &mut self,
        font: &Font,
        x: u32,
        y: u32,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.horizontal_line_metrics(size) else {
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
        font: &Font,
        line: TextLine,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.horizontal_line_metrics(size) else {
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
        font: &Font,
        bounds: TextBox,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let Some(metrics) = font.horizontal_line_metrics(size) else {
            return;
        };
        for (index, line) in wrap_text(font, text, size, bounds.width as f32)
            .iter()
            .enumerate()
        {
            let x = bounds.x as f32 + (bounds.width as f32 - text_width(font, line, size)) / 2.0;
            let baseline = bounds.y as f32 + metrics.ascent + index as f32 * metrics.new_line_size;
            self.draw_glyph_line(font, (x, baseline), size, line, color);
        }
    }

    pub fn finish(self) -> Result<(), PainterError> {
        self.context.finish()
    }

    fn draw_glyph_line(
        &mut self,
        font: &Font,
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
                    .horizontal_kern(previous, character, size)
                    .unwrap_or(0.0);
            }
            let (metrics, coverage) = font.rasterize(character, size);
            self.draw_mask(
                &coverage,
                metrics.width,
                metrics.height,
                (x + metrics.xmin as f32).round() as i32,
                (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32,
                color,
            );
            x += metrics.advance_width;
            previous = Some(character);
        }
    }

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
        self.context.fill_a8_mask_rgba32(
            blend2d::Mask {
                coverage,
                width,
                height,
            },
            x,
            y,
            blend2d_rgba32(color),
        );
    }
}

#[derive(Debug)]
pub struct RasterImage {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl RasterImage {
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    pub fn from_rgba(image: image::RgbaImage) -> Self {
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
        Self {
            pixels,
            width,
            height,
        }
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
    ))
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
    ))
}

pub fn load_overlay_font() -> Result<Font, Box<dyn std::error::Error>> {
    Font::from_bytes(
        include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf").as_slice(),
        FontSettings::default(),
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
            let value = (u32::from(channel) * 255)
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
    fn loads_webp_assets_from_daemon_cache() {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(&[10, 20, 30, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();

        let image = load_icon(&encoded).unwrap();
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.pixels, [30, 20, 10, 255]);
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
            pixels: vec![255; 8 * 4 * 4],
            width: 8,
            height: 4,
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
            pixels: vec![255; 4 * 2 * 4],
            width: 4,
            height: 2,
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
    fn currency_icons_match_reference_contain_sizes() {
        assert_eq!(contained_size(333, 285, 28, 28), (28, 24));
        assert_eq!(contained_size(64, 64, 30, 30), (30, 30));
    }

    #[test]
    fn contained_image_downsampling_has_smooth_alpha_edges() {
        let image = RasterImage {
            pixels: vec![0, 0, 0, 0, 255, 255, 255, 255],
            width: 2,
            height: 1,
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
        let font = load_overlay_font().unwrap();
        let metrics = font.horizontal_line_metrics(19.0).unwrap();

        assert_eq!(metrics.new_line_size.ceil(), 23.0);
    }

    #[test]
    fn measured_text_fits_one_line_at_fractional_scales() {
        let font = load_overlay_font().unwrap();
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
        let font = load_overlay_font().unwrap();
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
        assert_eq!(image.as_raw(), &[199, 99, 49, 128, 0, 0, 0, 0]);
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        pixels[offset..offset + 4].try_into().unwrap()
    }
}
