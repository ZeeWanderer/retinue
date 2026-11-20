use super::TextStyle;
use crate::{
    Bounds,
    layout::{Layout, LayoutResult},
};
use retinue_render::{Color, Command, ImageFit, ImageSource};
use std::sync::Arc;

pub struct DrawingContext<'a> {
    pub bounds: Bounds,
    pub text_style: TextStyle,
    commands: Vec<Command>,
    layout: LayoutResult<'a>,
}

impl<'a> DrawingContext<'a> {
    pub fn new(bounds: Bounds, text_style: TextStyle) -> Self {
        Self {
            bounds,
            text_style,
            commands: Vec::new(),
            layout: LayoutResult::default(),
        }
    }

    pub fn with_layout(mut self, layout: LayoutResult<'a>) -> Self {
        self.layout = layout;
        self
    }

    pub fn layout<L: Layout>(&self) -> Option<&'a L::Cache> {
        self.layout.get::<L>()
    }

    pub fn finish(self) -> Vec<Command> {
        self.commands
    }
    pub fn emit(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn shaped_text(
        &mut self,
        text: &super::text::shaping::ShapedText,
        origin: (f32, f32),
        color: Color,
    ) {
        self.commands.extend(text.commands(origin, color));
    }

    pub fn fill(&mut self, bounds: Bounds, radius: f32, color: Color) {
        self.emit(Command::Fill {
            bounds,
            radius,
            color,
        });
    }

    pub fn outline(&mut self, bounds: Bounds, radius: f32, thickness: f32, color: Color) {
        self.emit(Command::Outline {
            bounds,
            radius,
            thickness,
            color,
        });
    }

    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), thickness: f32, color: Color) {
        self.emit(Command::Line {
            from,
            to,
            thickness,
            color,
        });
    }

    pub fn ring(&mut self, center: (f32, f32), radius: f32, thickness: f32, color: Color) {
        self.emit(Command::Ring {
            center,
            radius,
            thickness,
            color,
        });
    }

    pub fn image(&mut self, image: impl Into<ImageSource>, bounds: Bounds, fit: ImageFit) {
        self.emit(Command::Image {
            bounds,
            image: image.into(),
            fit,
        });
    }

    pub fn clipped(&mut self, bounds: Bounds, draw: impl FnOnce(&mut Self)) {
        let mut child = Self::new(self.bounds, self.text_style.clone()).with_layout(self.layout);
        draw(&mut child);
        self.emit(Command::Clip {
            bounds,
            commands: Arc::new(child.commands),
        });
    }

    pub fn translated(&mut self, offset: (f32, f32), draw: impl FnOnce(&mut Self)) {
        let mut child = Self::new(
            Bounds::new(
                self.bounds.x - offset.0,
                self.bounds.y - offset.1,
                self.bounds.width,
                self.bounds.height,
            ),
            self.text_style.clone(),
        )
        .with_layout(self.layout);
        draw(&mut child);
        self.emit(Command::Translate {
            offset,
            commands: Arc::new(child.commands),
        });
    }
}
