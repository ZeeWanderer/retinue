use retinue_ui::composition::{
    Bounds, CachePolicy, Canvas, Command, Element, Key, Paint, PaintKey, RasterMode, RecordingMode,
    Renderer, Scene as CompositionScene,
};
use retinue_ui::painter::CachedFont;
use retinue_ui::painter::{Painter, RasterImage, TextLine, css_rgba, load_icon, text_width};
use retinue_ui::taffy::{Dimension, LengthPercentageAuto, Size};
use std::{cell::RefCell, rc::Rc};

use crate::ducats::{Parent, Quote, Scene};
use crate::ui::Rect;

pub(in crate::overlay) struct Assets {
    platinum: Rc<RasterImage>,
    owned: Rc<RasterImage>,
    mastered: Rc<RasterImage>,
    subsumed: Rc<RasterImage>,
    composition: RefCell<Composition>,
}

impl Assets {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            platinum: Rc::new(load_icon(include_bytes!("../../../assets/platinum.png"))?),
            owned: Rc::new(load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/owned.png"
            ))?),
            mastered: Rc::new(load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/mastered.png"
            ))?),
            subsumed: Rc::new(load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/helminth.png"
            ))?),
            composition: RefCell::new(Composition::default()),
        })
    }
}

fn row(scene: &Scene, dimensions: (u32, u32)) -> Option<(Rect, f32)> {
    let hover = &scene.hover;
    if hover.viewport != [dimensions.0, dimensions.1] {
        return None;
    }
    let s = hover.scale;
    if !s.is_finite() || !(0.25..=8.0).contains(&s) {
        return None;
    }
    let left = hover.price_label[2] + 8.0 * s;
    let right = hover.panel[2] - 40.0 * s;
    let top = hover.price_label[1];
    let bottom = hover.price_label[3];
    if left < 0.0
        || top < 0.0
        || right > dimensions.0 as f32
        || bottom > dimensions.1 as f32
        || right - left < 50.0 * s
        || bottom <= top
    {
        return None;
    }
    Some((
        Rect {
            x: left.ceil() as u32,
            y: top.ceil() as u32,
            width: (right - left).floor() as u32,
            height: (bottom - top).floor() as u32,
        },
        s,
    ))
}

pub(in crate::overlay) fn draw(
    painter: &mut Painter<'_>,
    font: &Rc<CachedFont>,
    assets: &Assets,
    scene: &Scene,
) {
    let Some((row, s)) = row(scene, (painter.width(), painter.height())) else {
        return;
    };
    let state = State {
        row,
        scale: s,
        size: (painter.width(), painter.height()),
        price: price_text(scene),
        badges: scene
            .parent
            .as_ref()
            .map_or([false; 3], |p| [p.owned, p.mastered, p.subsumed]),
    };
    let mut composition = assets.composition.borrow_mut();
    composition.update(state, assets, font);
    let Composition {
        scene, renderer, ..
    } = &mut *composition;
    renderer.draw_with(scene, painter, row, |_, _| unreachable!());
}

const fn key_for(id: u64) -> Key {
    Key::new("warframe.ducat-tooltip", id)
}

#[derive(PartialEq)]
struct State {
    row: Rect,
    scale: f32,
    size: (u32, u32),
    price: Option<String>,
    badges: [bool; 3],
}

struct Composition {
    scene: CompositionScene,
    renderer: Renderer,
    state: Option<State>,
}

impl Default for Composition {
    fn default() -> Self {
        Self {
            scene: CompositionScene::default(),
            renderer: Renderer::new(0),
            state: None,
        }
    }
}

impl Composition {
    fn update(&mut self, state: State, assets: &Assets, font: &Rc<CachedFont>) {
        if self.state.as_ref() == Some(&state) {
            return;
        }
        let row = state.row;
        let s = state.scale;
        let icon = (18.0 * s).round() as u32;
        let gap = (6.0 * s).round() as u32;
        let mut children = Vec::new();
        let mut used = 0;
        let mut price_commands = None;
        if let Some(price) = &state.price {
            let width = text_width(font, price, 19.0 * s).ceil() as u32;
            if icon + gap + width <= row.width {
                children.push(image_slot(
                    1,
                    assets.platinum.clone(),
                    icon,
                    icon,
                    row.height,
                ));
                let mut text = Element::new(key_for(2), fixed(width, row.height));
                text.style.margin.right = LengthPercentageAuto::length(gap as f32);
                text.paint = self.scene.paint(key_for(2)).cloned();
                let mut canvas = Canvas::new(1.0);
                canvas.draw_text_vertically_centered(
                    font,
                    TextLine::new(0, 0, row.height),
                    19.0 * s,
                    price,
                    css_rgba(190, 169, 102, 255),
                );
                price_commands = Some(canvas.commands);
                children.push(text);
                used = icon + gap + width + 2 * gap;
            }
        }
        for (index, (image, optical)) in [
            (&assets.owned, 14.0),
            (&assets.mastered, 18.0),
            (&assets.subsumed, 18.0),
        ]
        .into_iter()
        .enumerate()
        {
            if state.badges[index] && used + icon <= row.width {
                children.push(image_slot(
                    3 + index as u64,
                    image.clone(),
                    icon,
                    (optical * s).round() as u32,
                    row.height,
                ));
                used += icon + gap;
            }
        }
        let mut root = Element::row(key_for(0), gap as f32, children);
        root.style.size = fixed(row.width, row.height).size;
        root.style.flex_shrink = 0.0;
        root.offset = (row.x as f32, row.y as f32);
        // Native Scaleform coordinates already include the game's physical scale.
        self.scene
            .update(root, state.size, 1.0, RecordingMode::Retained)
            .expect("unique tooltip keys");
        if let Some(commands) = price_commands {
            self.scene
                .record(key_for(2), commands, CachePolicy::Never)
                .expect("price element");
        }
        self.renderer.prepare(&self.scene, RasterMode::Replay);
        self.state = Some(state);
    }
}

fn fixed(width: u32, height: u32) -> retinue_ui::taffy::Style {
    retinue_ui::taffy::Style {
        size: Size {
            width: Dimension::length(width as f32),
            height: Dimension::length(height as f32),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn image_slot(id: u64, image: Rc<RasterImage>, slot: u32, drawn: u32, height: u32) -> Element {
    let key = key_for(id);
    let mut element = Element::new(key, fixed(slot, height));
    element.paint = Some(Paint::new(PaintKey::new(key, drawn as u64), move |_| {
        vec![Command::Image {
            bounds: Bounds::new(
                ((slot - drawn) / 2) as f32,
                (height.saturating_sub(drawn) / 2) as f32,
                drawn as f32,
                drawn as f32,
            ),
            image: image.clone().into(),
            fit: retinue_ui::composition::ImageFit::Contain,
        }]
    }));
    element
}

fn price_text(scene: &Scene) -> Option<String> {
    if !scene.listed {
        None
    } else if let Some(quote) = &scene.quote {
        quote.lowest_sell.map(|p| p.to_string())
    } else if scene.failed {
        Some("--".into())
    } else {
        Some("...".into())
    }
}

pub(in crate::overlay) fn fixture(dimensions: (u32, u32)) -> Scene {
    let s = dimensions.0 as f32 / 2560.0;
    Scene {
        hover: retinue_warframe::game_observer::ui::DucatHover {
            title: "AKJAGARA PRIME BARREL".into(),
            ducats: 100,
            viewport: [dimensions.0, dimensions.1],
            panel: [934.4, 264.0, 1494.4, 516.8].map(|v| v * s),
            price_label: [958.4, 344.0, 1078.4, 386.88].map(|v| v * s),
            scale: 1.6 * s,
        },
        parent: Some(Parent {
            name: "Akjagara Prime".into(),
            owned: true,
            mastered: true,
            subsumed: false,
        }),
        listed: true,
        quote: Some(Quote {
            lowest_sell: Some(8),
            quoted_at: 0,
            stale: false,
        }),
        failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_row_keeps_native_anchors_and_removes_unlisted_price() {
        use retinue_ui::painter::{GlyphCache, load_overlay_font};
        let font = Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap());
        let assets = Assets::load().unwrap();
        for size in [(1920, 1080), (2560, 1440), (2560, 1600), (3840, 2160)] {
            let sample = fixture(size);
            let (row, s) = row(&sample, size).unwrap();
            let mut composition = Composition::default();
            composition.update(
                State {
                    row,
                    scale: s,
                    size,
                    price: Some("8".into()),
                    badges: [true; 3],
                },
                &assets,
                &font,
            );
            let icon = (18.0 * s).round() as u32;
            let gap = (6.0 * s).round() as u32;
            let text_width = text_width(&font, "8", 19.0 * s).ceil() as u32;
            assert_eq!(
                composition.scene.bounds(key_for(1)).unwrap().layout.x,
                row.x as f32
            );
            assert_eq!(
                composition.scene.bounds(key_for(2)).unwrap().layout.x,
                (row.x + icon + gap) as f32
            );
            assert_eq!(
                composition.scene.bounds(key_for(3)).unwrap().layout.x,
                (row.x + icon + 3 * gap + text_width) as f32
            );
            assert_eq!(
                composition.scene.hit_test((row.x as f64, row.y as f64)),
                None
            );
            composition.update(
                State {
                    row,
                    scale: s,
                    size,
                    price: None,
                    badges: [true; 3],
                },
                &assets,
                &font,
            );
            assert!(composition.scene.bounds(key_for(2)).is_none());
            assert_eq!(
                composition.scene.bounds(key_for(3)).unwrap().layout.x,
                row.x as f32
            );
        }
    }

    #[test]
    fn row_fits_beside_ducats_without_covering_native_control() {
        for size in [(1920, 1080), (2560, 1440), (2560, 1600), (3840, 2160)] {
            let scene = fixture(size);
            let (rect, scale) = row(&scene, size).unwrap();
            assert!(rect.x as f32 >= scene.hover.price_label[2]);
            assert!((rect.x + rect.width) as f32 <= scene.hover.panel[2] - 39.0 * scale);
        }
    }

    #[test]
    fn mismatched_window_size_is_not_guessed() {
        assert!(row(&fixture((2560, 1440)), (1920, 1080)).is_none());
    }

    #[test]
    fn absent_listing_hides_price_but_not_parent_status() {
        let mut scene = fixture((2560, 1440));
        scene.failed = true;
        assert_eq!(price_text(&scene).as_deref(), Some("8"));
        scene.failed = false;
        scene.listed = false;
        assert_eq!(price_text(&scene), None);
        assert!(scene.parent.as_ref().unwrap().owned);
        scene.listed = true;
        scene.quote.as_mut().unwrap().lowest_sell = None;
        assert_eq!(price_text(&scene), None);
        scene.quote = None;
        assert_eq!(price_text(&scene).as_deref(), Some("..."));
        scene.failed = true;
        assert_eq!(price_text(&scene).as_deref(), Some("--"));
        assert!(scene.parent.as_ref().unwrap().owned);
    }
}
