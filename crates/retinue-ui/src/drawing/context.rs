use super::TextStyle;
use crate::Bounds;
use retinue_render::{Color, Command, ImageFit, ImageSource};
use std::rc::Rc;

pub struct DrawingContext {
    pub bounds: Bounds,
    pub text_style: TextStyle,
    commands: Vec<Command>,
}

impl DrawingContext {
    pub fn new(bounds: Bounds, text_style: TextStyle) -> Self {
        Self {
            bounds,
            text_style,
            commands: Vec::new(),
        }
    }

    pub fn finish(self) -> Vec<Command> {
        self.commands
    }
    pub fn emit(&mut self, command: Command) {
        self.commands.push(command);
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
        let mut child = Self::new(self.bounds, self.text_style.clone());
        draw(&mut child);
        self.emit(Command::Clip {
            bounds,
            commands: Rc::new(child.commands),
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
        );
        draw(&mut child);
        self.emit(Command::Translate {
            offset,
            commands: Rc::new(child.commands),
        });
    }
}
