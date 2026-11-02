use retinue_render::painter::CachedFont;
use retinue_render::painter::{Painter, RasterImage, load_icon, text_width};
use retinue_render::{Key, RasterMode, Renderer};
use retinue_ui::{prelude::*, runtime::ViewTree};
use std::{cell::RefCell, rc::Rc};

mod price_view;
mod tooltip_view;

use crate::ducats::{Parent, Quote, Scene};
use crate::presentation::geometry::Rect;

pub(in crate::presentation) struct Assets {
    platinum: Rc<RasterImage>,
    owned: Rc<RasterImage>,
    mastered: Rc<RasterImage>,
    subsumed: Rc<RasterImage>,
    composition: RefCell<Composition>,
}

impl Assets {
    pub(in crate::presentation) fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.composition.get_mut().renderer.set_host(host);
    }

    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            platinum: Rc::new(load_icon(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/platinum.png"
            )))?),
            owned: Rc::new(load_icon(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/warframe/ui/owned.png"
            )))?),
            mastered: Rc::new(load_icon(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/warframe/ui/mastered.png"
            )))?),
            subsumed: Rc::new(load_icon(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/warframe/ui/helminth.png"
            )))?),
            composition: RefCell::new(Composition::default()),
        })
    }
}

fn tooltip_bounds(scene: &Scene, dimensions: (u32, u32)) -> Option<(Rect, f32)> {
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

pub(in crate::presentation) fn draw(
    painter: &mut Painter<'_>,
    font: &Rc<CachedFont>,
    assets: &Assets,
    scene: &Scene,
) {
    let Some((row, s)) = tooltip_bounds(scene, (painter.width(), painter.height())) else {
        return;
    };
    let state = TooltipState {
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
    let Composition { tree, renderer, .. } = &mut *composition;
    let _pass = renderer.host().begin_pass(painter.thread_count());
    renderer.prepare(tree.frame(), RasterMode::Replay);
    renderer.draw_with(tree.frame(), painter, row, |_, _| unreachable!());
}

const fn key_for(id: u64) -> Key {
    Key::new("warframe.ducat-tooltip", id)
}

#[derive(Clone, PartialEq)]
struct TooltipState {
    row: Rect,
    scale: f32,
    size: (u32, u32),
    price: Option<String>,
    badges: [bool; 3],
}

struct Composition {
    tree: ViewTree,
    renderer: Renderer,
    state: Option<TooltipState>,
    view: Option<Tooltip>,
}

impl Default for Composition {
    fn default() -> Self {
        Self {
            tree: ViewTree::new(empty(), || {}),
            renderer: Renderer::new(retinue_render::ResidencyPolicy::overlay(0)),
            state: None,
            view: None,
        }
    }
}

impl Composition {
    fn update(&mut self, state: TooltipState, assets: &Assets, font: &Rc<CachedFont>) {
        if self.state.as_ref() == Some(&state) {
            return;
        }
        let metrics = Metrics::new(&state, font);
        let view = self.view.get_or_insert_with(|| {
            let view = Tooltip {
                metrics: State::new(metrics),
                price: State::new(None),
            };
            self.tree.set_root(tooltip_view::TooltipView {
                metrics: view.metrics.clone(),
                price: view.price.clone(),
                style: TextStyle::new(font.clone(), 19.0, Color::rgb(0xbea966)),
                images: [
                    assets.platinum.clone().into(),
                    assets.owned.clone().into(),
                    assets.mastered.clone().into(),
                    assets.subsumed.clone().into(),
                ],
            });
            view
        });
        view.metrics.set(metrics);
        view.price.set(state.price.clone());
        // Native Scaleform coordinates already include the game's physical scale.
        self.tree
            .update(state.size, 1.0)
            .expect("valid tooltip view");
        self.state = Some(state);
    }

    #[cfg(test)]
    fn bounds(&self, key: Key) -> Option<retinue_ui::scene::Geometry> {
        self.tree.geometry(self.tree.unique(key).unwrap()?)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Metrics {
    row: Rect,
    scale: f32,
    icon: u32,
    gap: u32,
    price_width: u32,
    visible: [bool; 4],
}

impl Metrics {
    fn new(state: &TooltipState, font: &CachedFont) -> Self {
        let icon = (18.0 * state.scale).round() as u32;
        let gap = (6.0 * state.scale).round() as u32;
        let price_width = state.price.as_ref().map_or(0, |text| {
            text_width(font, text, 19.0 * state.scale).ceil() as u32
        });
        let mut visible = [false; 4];
        visible[0] = state.price.is_some() && icon + gap + price_width <= state.row.width;
        let mut used = if visible[0] {
            icon + price_width + 3 * gap
        } else {
            0
        };
        for (index, badge) in state.badges.into_iter().enumerate() {
            visible[index + 1] = badge && used + icon <= state.row.width;
            if visible[index + 1] {
                used += icon + gap;
            }
        }
        Self {
            row: state.row,
            scale: state.scale,
            icon,
            gap,
            price_width,
            visible,
        }
    }
}

struct Tooltip {
    metrics: State<Metrics>,
    price: State<Option<String>>,
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

pub(in crate::presentation) fn fixture(dimensions: (u32, u32)) -> Scene {
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
        use retinue_render::painter::{GlyphCache, load_overlay_font};
        let font = Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap());
        let assets = Assets::load().unwrap();
        for size in [(1920, 1080), (2560, 1440), (2560, 1600), (3840, 2160)] {
            let sample = fixture(size);
            let (row, s) = tooltip_bounds(&sample, size).unwrap();
            let mut composition = Composition::default();
            composition.update(
                TooltipState {
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
                composition.bounds(key_for(1)).unwrap().layout.x,
                row.x as f32
            );
            assert_eq!(
                composition.bounds(key_for(2)).unwrap().layout.x,
                (row.x + icon + gap) as f32
            );
            assert_eq!(
                composition.bounds(key_for(3)).unwrap().layout.x,
                (row.x + icon + 3 * gap + text_width) as f32
            );
            assert_eq!(
                composition
                    .tree
                    .scene()
                    .hit_test((row.x as f64, row.y as f64)),
                None
            );
            composition.update(
                TooltipState {
                    row,
                    scale: s,
                    size,
                    price: None,
                    badges: [true; 3],
                },
                &assets,
                &font,
            );
            assert!(composition.bounds(key_for(2)).is_none());
            assert_eq!(
                composition.bounds(key_for(3)).unwrap().layout.x,
                row.x as f32
            );
        }
    }

    #[test]
    fn tooltip_motion_reuses_paint_and_price_updates_only_rerecord_text() {
        use retinue_render::painter::{GlyphCache, load_overlay_font};
        let font = Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap());
        let assets = Assets::load().unwrap();
        for size in [(1920, 1080), (2560, 1440), (3840, 2160)] {
            let (row, scale) = tooltip_bounds(&fixture(size), size).unwrap();
            let mut state = TooltipState {
                row,
                scale,
                size,
                price: Some("8".into()),
                badges: [true; 3],
            };
            let mut composition = Composition::default();
            composition.update(state.clone(), &assets, &font);
            state.row.y += 8;
            composition.update(state.clone(), &assets, &font);
            assert_eq!(composition.tree.scene().stats.recordings, 0);
            assert_eq!(composition.tree.scene().stats.layout_computations, 0);
            state.price = Some("9".into());
            composition.update(state, &assets, &font);
            assert_eq!(composition.tree.scene().stats.recordings, 1);
            assert_eq!(composition.tree.scene().stats.layout_computations, 0);
            assert_eq!(composition.tree.stats.bodies, 0);
            assert_eq!(composition.tree.stats.projected, 1);
        }
    }

    #[test]
    fn row_fits_beside_ducats_without_covering_native_control() {
        for size in [(1920, 1080), (2560, 1440), (2560, 1600), (3840, 2160)] {
            let scene = fixture(size);
            let (rect, scale) = tooltip_bounds(&scene, size).unwrap();
            assert!(rect.x as f32 >= scene.hover.price_label[2]);
            assert!((rect.x + rect.width) as f32 <= scene.hover.panel[2] - 39.0 * scale);
        }
    }

    #[test]
    fn mismatched_window_size_is_not_guessed() {
        assert!(tooltip_bounds(&fixture((2560, 1440)), (1920, 1080)).is_none());
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
