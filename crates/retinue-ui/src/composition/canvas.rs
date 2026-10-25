use super::{Bounds, Color, Command, ImageFit, ImageSource, TextAlignment};
use crate::{
    Rect,
    painter::{CachedFont, TextBox, TextLine, text_width, wrap_text},
};
use std::rc::Rc;

/// Records custom pixel-layout painting into the same retained command stream.
pub struct Canvas {
    pub commands: Vec<Command>,
    scale: f32,
    origin: (i32, i32),
}

impl Canvas {
    pub fn new(scale: f32) -> Self {
        assert!(scale.is_finite() && scale > 0.0);
        Self {
            commands: Vec::new(),
            scale,
            origin: (0, 0),
        }
    }

    pub fn at(origin: (u32, u32), scale: f32) -> Self {
        Self {
            origin: (origin.0 as i32, origin.1 as i32),
            ..Self::new(scale)
        }
    }

    pub fn draw_ring(&mut self, x: u32, y: u32, radius: u32, stroke: u32, color: [u8; 4]) {
        self.commands.push(Command::Ring {
            center: (
                (x as f32 - self.origin.0 as f32) / self.scale,
                (y as f32 - self.origin.1 as f32) / self.scale,
            ),
            radius: radius as f32 / self.scale,
            thickness: stroke as f32 / self.scale,
            color: Color::from_bgra(color),
        });
    }

    pub fn external(&mut self, key: super::Key, bounds: Rect) {
        self.commands.push(Command::External {
            key,
            bounds: self.bounds(bounds),
        });
    }

    pub fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), thickness: f64, color: [u8; 4]) {
        let local = |point: (f64, f64)| {
            (
                (point.0 as f32 - self.origin.0 as f32) / self.scale,
                (point.1 as f32 - self.origin.1 as f32) / self.scale,
            )
        };
        self.commands.push(Command::Line {
            from: local(from),
            to: local(to),
            thickness: thickness as f32 / self.scale,
            color: Color::from_bgra(color),
        });
    }

    pub fn draw_centered_text(
        &mut self,
        font: &Rc<CachedFont>,
        bounds: TextBox,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let lines = wrap_text(font, text, size, bounds.width as f32).len();
        let height = font
            .horizontal_line_metrics(size)
            .map_or(size, |m| m.new_line_size)
            * lines.max(1) as f32;
        self.commands.push(Command::Text {
            bounds: self.bounds(Rect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: height.ceil() as u32 + 2,
            }),
            font: font.clone(),
            text: text.into(),
            size: size / self.scale,
            color: Color::from_bgra(color),
            align: TextAlignment::WrappedCenter,
        });
    }

    pub fn bounds(&self, r: Rect) -> Bounds {
        Bounds::new(
            (r.x as f32 - self.origin.0 as f32) / self.scale,
            (r.y as f32 - self.origin.1 as f32) / self.scale,
            r.width as f32 / self.scale,
            r.height as f32 / self.scale,
        )
    }

    pub fn fill(&mut self, r: Rect, radius: u32, color: [u8; 4]) {
        self.commands.push(Command::Fill {
            bounds: self.bounds(r),
            radius: radius as f32 / self.scale,
            color: Color::from_bgra(color),
        });
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
        self.fill(
            Rect {
                x,
                y,
                width,
                height,
            },
            radius,
            color,
        );
    }

    pub fn outline(&mut self, r: Rect, radius: u32, thickness: u32, color: [u8; 4]) {
        self.commands.push(Command::Outline {
            bounds: self.bounds(r),
            radius: radius as f32 / self.scale,
            thickness: thickness as f32 / self.scale,
            color: Color::from_bgra(color),
        });
    }

    pub fn text(&mut self, r: Rect, font: Rc<CachedFont>, text: String, size: f32, color: [u8; 4]) {
        self.commands.push(Command::Text {
            bounds: self.bounds(r),
            font,
            text,
            size,
            color: Color::from_bgra(color),
            align: TextAlignment::Center,
        });
    }

    pub fn image(&mut self, r: Rect, image: impl Into<ImageSource>, fit: ImageFit) {
        self.commands.push(Command::Image {
            bounds: self.bounds(r),
            image: image.into(),
            fit,
        });
    }

    pub fn clip(&mut self, bounds: Rect, draw: impl FnOnce(&mut Self)) {
        self.viewport(bounds, (0, 0), draw);
    }

    pub fn local(&mut self, bounds: Rect, draw: impl FnOnce(&mut Self, Rect)) {
        self.viewport(bounds, (bounds.x as i32, bounds.y as i32), |canvas| {
            draw(
                canvas,
                Rect {
                    x: 0,
                    y: 0,
                    ..bounds
                },
            );
        });
    }

    pub fn viewport(&mut self, bounds: Rect, offset: (i32, i32), draw: impl FnOnce(&mut Self)) {
        let mut nested = Self {
            origin: (self.origin.0 - offset.0, self.origin.1 - offset.1),
            ..Self::new(self.scale)
        };
        draw(&mut nested);
        self.commands.push(Command::Clip {
            bounds: self.bounds(bounds),
            commands: Rc::new(nested.commands),
        });
    }

    pub fn draw_image(
        &mut self,
        image: impl Into<ImageSource>,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) {
        self.image(
            Rect {
                x,
                y,
                width,
                height,
            },
            image,
            ImageFit::Stretch,
        );
    }

    pub fn draw_image_contained(
        &mut self,
        image: impl Into<ImageSource>,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) {
        self.image(
            Rect {
                x,
                y,
                width,
                height,
            },
            image,
            ImageFit::Contain,
        );
    }

    pub fn draw_image_contained_circle(
        &mut self,
        image: impl Into<ImageSource>,
        x: u32,
        y: u32,
        diameter: u32,
    ) {
        self.image(
            Rect {
                x,
                y,
                width: diameter,
                height: diameter,
            },
            image,
            ImageFit::Circle,
        );
    }

    pub fn draw_text(
        &mut self,
        font: &Rc<CachedFont>,
        x: u32,
        y: u32,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let width = text
            .lines()
            .map(|line| text_width(font, line, size))
            .fold(0.0, f32::max)
            .ceil() as u32
            + 4;
        let height = font
            .horizontal_line_metrics(size)
            .map_or(size, |m| m.new_line_size)
            * text.lines().count().max(1) as f32;
        self.commands.push(Command::Text {
            bounds: self.bounds(Rect {
                x,
                y,
                width,
                height: height.ceil() as u32 + 2,
            }),
            font: font.clone(),
            text: text.into(),
            size: size / self.scale,
            color: Color::from_bgra(color),
            align: TextAlignment::Top,
        });
    }

    pub fn draw_text_vertically_centered(
        &mut self,
        font: &Rc<CachedFont>,
        line: TextLine,
        size: f32,
        text: &str,
        color: [u8; 4],
    ) {
        let width = text_width(font, text, size).ceil() as u32 + 4;
        self.text(
            Rect {
                x: line.x,
                y: line.y,
                width,
                height: line.height,
            },
            font.clone(),
            text.into(),
            size / self.scale,
            color,
        );
    }
}
