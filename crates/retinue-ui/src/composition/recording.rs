use super::{
    Bounds, CachePolicy, Canvas, Element, Key, RasterMode, RecordingMode, Renderer, Scene,
};
use crate::{Rect, damage::Damage, painter::Painter};
use std::ops::{Deref, DerefMut};

/// Pixel-layout clients record named, locally anchored sections into a retained scene.
pub struct Recording {
    canvas: Canvas,
    size: (u32, u32),
    current: (Key, Rect),
    sections: Vec<(Key, Rect, Vec<super::Command>)>,
}

impl Recording {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            canvas: Canvas::new(1.0),
            size,
            current: (
                Key::new("frame.background", 0),
                Rect {
                    width: size.0,
                    height: size.1,
                    ..Rect::default()
                },
            ),
            sections: Vec::new(),
        }
    }

    pub fn width(&self) -> u32 {
        self.size.0
    }
    pub fn height(&self) -> u32 {
        self.size.1
    }

    pub fn section(&mut self, key: Key, bounds: Rect) {
        self.flush();
        self.current = (key, bounds);
        self.canvas = Canvas::at((bounds.x, bounds.y), 1.0);
    }

    fn flush(&mut self) {
        if !self.canvas.commands.is_empty() {
            self.sections.push((
                self.current.0,
                self.current.1,
                std::mem::take(&mut self.canvas.commands),
            ));
        }
    }

    pub fn update(mut self, scene: &mut Scene) -> Result<Damage, &'static str> {
        self.flush();
        let mut root = Element::at(
            Key::new("frame.root", 0),
            Bounds::new(0.0, 0.0, self.size.0 as f32, self.size.1 as f32),
        );
        for (key, bounds, _) in &self.sections {
            let mut node = Element::at(
                *key,
                Bounds::new(
                    bounds.x as f32,
                    bounds.y as f32,
                    bounds.width as f32,
                    bounds.height as f32,
                ),
            );
            node.paint = scene.paint(*key).cloned();
            root.children.push(node);
        }
        let damage = scene.update(root, self.size, 1.0, RecordingMode::Retained)?;
        Ok(damage.union(
            scene.record_many(
                self.sections
                    .into_iter()
                    .map(|(key, _, commands)| (key, commands, CachePolicy::Never)),
            )?,
        ))
    }
}

impl Deref for Recording {
    type Target = Canvas;
    fn deref(&self) -> &Canvas {
        &self.canvas
    }
}
impl DerefMut for Recording {
    fn deref_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
}

pub struct Surface {
    scene: Scene,
    renderer: Renderer,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            scene: Scene::default(),
            renderer: Renderer::new(0),
        }
    }
}

impl Surface {
    pub fn draw(
        &mut self,
        recording: Recording,
        painter: &mut Painter<'_>,
        repair: Rect,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) -> Result<Damage, &'static str> {
        let damage = recording.update(&mut self.scene)?;
        self.renderer.prepare(&self.scene, RasterMode::Replay);
        self.renderer
            .draw_with(&self.scene, painter, repair, external);
        Ok(damage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painter::{GlyphCache, TextBox, TextLine, load_overlay_font};
    use std::rc::Rc;

    #[test]
    fn pixel_sections_preserve_native_paint_and_report_only_changed_content() {
        let font = Rc::new(load_overlay_font(&GlyphCache::new(8192)).unwrap());
        let size = (240, 170);
        let frame = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let section = Rect {
            x: 34,
            y: 23,
            width: 160,
            height: 110,
        };
        let record = |shift: u32| {
            let mut r = Recording::new(size);
            r.section(
                Key::new("card", 0),
                Rect {
                    x: section.x + shift,
                    ..section
                },
            );
            assert_eq!((r.width(), r.height()), size);
            r.draw_ring(65 + shift, 64, 16, 3, [90, 170, 220, 180]);
            r.draw_text_vertically_centered(
                &font,
                TextLine::new(40 + shift, 80, 20),
                16.0,
                "Relic rewards",
                [255; 4],
            );
            r.draw_centered_text(
                &font,
                TextBox::new(30 + shift, 102, 150),
                14.0,
                "A wrapped reward description",
                [255; 4],
            );
            r
        };
        let mut surface = Surface::default();
        let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
        for shift in [0, 9] {
            let mut p = Painter::new(&mut pixels, size.0, size.1).unwrap();
            p.clear();
            surface
                .draw(record(shift), &mut p, frame, |_, _| unreachable!())
                .unwrap();
            p.finish().unwrap();
            let mut expected = vec![0; pixels.len()];
            let mut p = Painter::new(&mut expected, size.0, size.1).unwrap();
            p.draw_ring(65 + shift, 64, 16, 3, [90, 170, 220, 180]);
            p.draw_text_vertically_centered(
                font.as_ref(),
                TextLine::new(40 + shift, 80, 20),
                16.0,
                "Relic rewards",
                [255; 4],
            );
            p.draw_centered_text(
                font.as_ref(),
                TextBox::new(30 + shift, 102, 150),
                14.0,
                "A wrapped reward description",
                [255; 4],
            );
            p.finish().unwrap();
            assert!(
                pixels == expected,
                "recorded sections differ from direct painting"
            );
            assert!(matches!(
                record(shift).update(&mut surface.scene).unwrap(),
                Damage::None
            ));
        }
    }

    #[test]
    fn sibling_popup_does_not_change_retained_content() {
        let size = (120, 80);
        let card = Key::new("card", 0);
        let build = |popup: bool| {
            let mut r = Recording::new(size);
            r.section(
                card,
                Rect {
                    x: 10,
                    y: 10,
                    width: 90,
                    height: 60,
                },
            );
            r.fill_rounded_rect(10, 10, 90, 60, 6, [20, 40, 60, 255]);
            if popup {
                r.section(
                    Key::new("popup", 0),
                    Rect {
                        x: 30,
                        y: 20,
                        width: 50,
                        height: 40,
                    },
                );
                r.fill_rounded_rect(30, 20, 50, 40, 4, [180, 70, 20, 180]);
            }
            r
        };
        let mut scene = Scene::default();
        build(false).update(&mut scene).unwrap();
        let identity = scene.paint(card).unwrap().key;
        for popup in [true, false, true] {
            build(popup).update(&mut scene).unwrap();
            assert_eq!(scene.paint(card).unwrap().key, identity);
        }
    }
}
