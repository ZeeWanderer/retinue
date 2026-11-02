use super::{DrawList, PixelCanvas};
use crate::{Rect, layout::anchors::AnchorPoint, prelude::*};
use retinue_render::{CacheHint, Key};
use std::ops::{Deref, DerefMut};

/// Named local drawings for custom pixel-layout content. Layout and painter order
/// remain visible to the SDK; unchanged layers retain their recordings.
pub struct PixelLayers {
    canvas: PixelCanvas,
    size: (u32, u32),
    current: (Key, Rect, CacheHint),
    layers: Vec<Layer>,
}

#[derive(Clone, PartialEq)]
struct Layer {
    key: Key,
    bounds: Rect,
    hint: CacheHint,
    drawing: DrawList,
}

#[derive(PartialEq)]
pub struct LayeredDrawing {
    size: (u32, u32),
    layers: Vec<Layer>,
}

impl View for LayeredDrawing {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            ZStack() {
                for layer in &self.layers {
                    Canvas(layer.drawing.clone())
                        .frame(layer.bounds.width as f32, layer.bounds.height as f32)
                        .offset(layer.bounds.x as f32, layer.bounds.y as f32)
                        .reuse(layer.hint).id(layer.key)
                }
            }.alignment(Alignment::Start)
                .frame_aligned(self.size.0 as f32, self.size.1 as f32, AnchorPoint::TOP_LEADING)
        }
    }
}

impl PixelLayers {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            canvas: PixelCanvas::new(1.0),
            size,
            current: (
                Key::new("canvas.background", 0),
                Rect {
                    width: size.0,
                    height: size.1,
                    ..Rect::default()
                },
                CacheHint::Dynamic,
            ),
            layers: Vec::new(),
        }
    }
    pub fn width(&self) -> u32 {
        self.size.0
    }
    pub fn height(&self) -> u32 {
        self.size.1
    }

    pub fn section(&mut self, key: Key, bounds: Rect) {
        self.cached_section(key, bounds, CacheHint::Dynamic);
    }

    pub fn cached_section(&mut self, key: Key, bounds: Rect, hint: CacheHint) {
        self.flush();
        self.current = (key, bounds, hint);
        self.canvas = PixelCanvas::at((bounds.x, bounds.y), 1.0);
    }

    fn flush(&mut self) {
        if !self.canvas.commands.is_empty() {
            self.layers.push(Layer {
                key: self.current.0,
                bounds: self.current.1,
                hint: self.current.2,
                drawing: std::mem::take(&mut self.canvas.commands).into(),
            });
        }
    }

    pub fn finish(mut self) -> LayeredDrawing {
        self.flush();
        LayeredDrawing {
            size: self.size,
            layers: self.layers,
        }
    }
}

impl Deref for PixelLayers {
    type Target = PixelCanvas;
    fn deref(&self) -> &Self::Target {
        &self.canvas
    }
}
impl DerefMut for PixelLayers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.canvas
    }
}
