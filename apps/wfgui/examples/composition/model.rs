use retinue_ui::{
    composition::{
        Algorithm, Bounds, CachePolicy, Color, Command, Element, LayoutContext, Paint,
        RecordingMode, Renderer, Scene, SelectionMotion,
    },
    damage::Damage,
    grid::Grid,
    painter::{CachedFont, GlyphCache, RasterImage, load_icon, load_overlay_font},
    taffy::{Dimension, LengthPercentageAuto, Point, Position, Size, Style},
};
use std::{rc::Rc, time::Instant};

pub const BACKGROUND: Color = Color::rgb(0x181c27);
const CARD: Color = Color::rgb(0x212734);
const TEXT: Color = Color::rgb(0xe8ecf2);
const MUTED: Color = Color::rgb(0x919db1);
const GREEN: Color = Color::rgb(0x3da46b);
const ACCENT: Color = Color::rgb(0x8c5edc);

pub fn fixed(width: f32, height: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(width),
            height: Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Style::default()
    }
}

pub struct Demo {
    pub scene: Scene,
    pub renderer: Renderer,
    pub size: (u32, u32),
    pub scale: f32,
    pub scroll: f32,
    pub counter: u32,
    pub replacement: bool,
    pub motion: SelectionMotion,
    font: Rc<CachedFont>,
    images: [Rc<RasterImage>; 2],
    glyph: Rc<RasterImage>,
}

impl Demo {
    pub fn new(scale: f32) -> Self {
        let glyphs = GlyphCache::new(4 * 1024 * 1024);
        let font = Rc::new(load_overlay_font(&glyphs).unwrap());
        let image = |data: &[u8], size: f32| {
            Rc::new(
                load_icon(data)
                    .unwrap()
                    .resampled((size * scale).round() as u32, (size * scale).round() as u32)
                    .unwrap(),
            )
        };
        Self {
            scene: Scene::default(),
            renderer: Renderer::new(16 * 1024 * 1024),
            size: ((960.0 * scale) as u32, (680.0 * scale) as u32),
            scale,
            scroll: 0.0,
            counter: 2,
            replacement: false,
            motion: SelectionMotion::new(10),
            font,
            images: [
                image(
                    include_bytes!(
                        "../../../wfcompanion/test/fixtures/relic-parts/sub_icons/weapon/prime_barrel_128x128.png"
                    ),
                    72.0,
                ),
                image(
                    include_bytes!(
                        "../../../wfcompanion/test/fixtures/relic-parts/sub_icons/weapon/prime_receiver_128x128.png"
                    ),
                    72.0,
                ),
            ],
            glyph: image(
                include_bytes!("../../resources/ui/mastery_relics.png"),
                16.0,
            ),
        }
    }

    fn label(
        &self,
        key: u64,
        revision: u64,
        text: String,
        width: f32,
        height: f32,
        color: Color,
    ) -> Element {
        let mut leaf = Element::new(key, fixed(width, height));
        let font = self.font.clone();
        leaf.paint = Some(Paint::new(revision, move |size| {
            vec![Command::Text {
                align: retinue_ui::composition::TextAlignment::Top,
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                size: 14.0,
                text: text.clone(),
                font: font.clone(),
                color,
            }]
        }));
        leaf
    }

    fn tab(&self, key: u64, text: &str, now: Instant) -> Element {
        let weight = self.motion.weight(key, now);
        let width = 36.0 + 91.0 * weight;
        let mut leaf = Element::new(key, fixed(width, 36.0));
        leaf.clip = true;
        leaf.interactive = true;
        let font = self.font.clone();
        let image = self.glyph.clone();
        let text = text.to_owned();
        let selected = key == self.motion.selected().value;
        leaf.paint = Some(Paint::new(
            50 + key + u64::from(selected) * 10,
            move |size| {
                vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                        radius: 18.0,
                        color: if selected { ACCENT } else { CARD },
                    },
                    Command::Image {
                        bounds: Bounds::new(10.0, 10.0, 16.0, 16.0),
                        image: image.clone().into(),
                        fit: retinue_ui::composition::ImageFit::Stretch,
                    },
                    Command::Text {
                        align: retinue_ui::composition::TextAlignment::Top,
                        bounds: Bounds::new(36.0, 8.0, 86.0, 20.0),
                        size: 14.0,
                        text: text.clone(),
                        font: font.clone(),
                        color: TEXT,
                    },
                ]
            },
        ));
        leaf
    }

    fn component(&self, index: usize, width: f32) -> Element {
        let id = 1000 + index as u64 * 10;
        let image = self.images[usize::from(self.replacement && index == 0)].clone();
        let mut artwork = Element::new(id + 1, fixed(78.0, 78.0));
        artwork.paint = Some(Paint::new(
            100 + u64::from(self.replacement && index == 0),
            move |_| {
                vec![
                    Command::Fill {
                        bounds: Bounds::new(0.0, 0.0, 78.0, 78.0),
                        radius: 39.0,
                        color: GREEN,
                    },
                    Command::Fill {
                        bounds: Bounds::new(2.0, 2.0, 74.0, 74.0),
                        radius: 37.0,
                        color: CARD,
                    },
                    Command::Image {
                        bounds: Bounds::new(3.0, 3.0, 72.0, 72.0),
                        image: image.clone().into(),
                        fit: retinue_ui::composition::ImageFit::Stretch,
                    },
                ]
            },
        ));
        artwork.paint.as_mut().unwrap().cache = CachePolicy::Prefer;
        let amount = if index == 0 {
            self.counter
        } else {
            2 + index as u32 % 5
        };
        let mut count = self.label(
            id + 2,
            200 + amount as u64,
            amount.to_string(),
            24.0,
            20.0,
            TEXT,
        );
        let font = self.font.clone();
        count.paint = Some(Paint::new(200 + amount as u64, move |_| {
            vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 24.0, 20.0),
                    radius: 8.0,
                    color: BACKGROUND,
                },
                Command::Text {
                    align: retinue_ui::composition::TextAlignment::Top,
                    bounds: Bounds::new(7.0, 1.0, 16.0, 18.0),
                    size: 14.0,
                    text: amount.to_string(),
                    font: font.clone(),
                    color: TEXT,
                },
            ]
        }));
        let mut badge = Element::new(id + 3, fixed(24.0, 24.0));
        let glyph = self.glyph.clone();
        badge.paint = Some(Paint::new(300, move |_| {
            vec![
                Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, 24.0, 24.0),
                    radius: 12.0,
                    color: Color::rgb(0xc5a453),
                },
                Command::Image {
                    bounds: Bounds::new(4.0, 4.0, 16.0, 16.0),
                    image: glyph.clone().into(),
                    fit: retinue_ui::composition::ImageFit::Stretch,
                },
            ]
        }));
        let mut overlay = Element::new(id + 4, fixed(82.0, 84.0));
        overlay.algorithm = Algorithm::Custom(component_overlay);
        overlay.children = vec![artwork, count, badge];
        let labels = Element::column(
            id + 5,
            6.0,
            vec![
                self.label(
                    id + 6,
                    400,
                    "Prime component".into(),
                    width - 110.0,
                    22.0,
                    TEXT,
                ),
                self.label(
                    id + 7,
                    401,
                    "Blueprint owned".into(),
                    width - 110.0,
                    20.0,
                    MUTED,
                ),
            ],
        );
        let mut card = Element::row(id, 12.0, vec![overlay, labels]);
        card.style.size = fixed(width, 108.0).size;
        card.style.padding = retinue_ui::taffy::Rect::length(10.0_f32);
        card.style.align_items = Some(retinue_ui::taffy::AlignItems::CENTER);
        card.paint = Some(Paint::new(500, |size| {
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                radius: 8.0,
                color: CARD,
            }]
        }));
        card
    }

    pub fn description(&self, now: Instant) -> Element {
        let width = self.size.0 as f32 / self.scale;
        let height = self.size.1 as f32 / self.scale;
        let tabs = Element::row(
            2,
            8.0,
            ["Warframes", "Primary", "Secondary", "Melee"]
                .iter()
                .enumerate()
                .map(|(i, text)| self.tab(10 + i as u64, text, now))
                .collect(),
        );
        let grid = Grid::new((width - 40.0) as u32, 260, 108, 10);
        let mut content = Element::new(4, fixed(width - 40.0, grid.content_height(5000) as f32));
        content.offset.1 = -self.scroll;
        for index in grid.visible(
            5000,
            self.scroll as u32,
            (height - 140.0).max(0.0) as u32,
            1,
        ) {
            let mut card = self.component(index, grid.width as f32);
            let (x, y) = grid.origin(index);
            card.style.position = Position::Absolute;
            card.style.inset.left = LengthPercentageAuto::length(x as f32);
            card.style.inset.top = LengthPercentageAuto::length(y as f32);
            content.children.push(card);
        }
        let mut scroll = Element::column(3, 0.0, vec![content]);
        scroll.style = fixed(width - 40.0, (height - 140.0).max(1.0));
        scroll.clip = true;
        let mut root = Element::column(
            1,
            14.0,
            vec![
                self.label(5, 402, "Composition prototype".into(), 350.0, 26.0, TEXT),
                tabs,
                scroll,
            ],
        );
        root.style.size = fixed(width, height).size;
        root.style.padding = retinue_ui::taffy::Rect::length(20.0_f32);
        root
    }

    pub fn update(&mut self, now: Instant, mode: RecordingMode) -> Damage {
        self.scene
            .update(self.description(now), self.size, self.scale, mode)
            .unwrap()
    }

    pub fn scroll_by(&mut self, distance: f32) {
        let grid = Grid::new(
            (self.size.0 as f32 / self.scale - 40.0) as u32,
            260,
            108,
            10,
        );
        let max =
            (grid.content_height(5000) as f32 - (self.size.1 as f32 / self.scale - 140.0)).max(0.0);
        self.scroll = (self.scroll + distance).clamp(0.0, max);
    }
}

fn component_overlay(context: &mut LayoutContext<'_>) -> Size<f32> {
    let size = context.container_size(Size {
        width: 82.0,
        height: 84.0,
    });
    context.anchor(
        0,
        Bounds::new(2.0, 2.0, 78.0, 78.0),
        Point::ZERO,
        Point::ZERO,
    );
    let frame = Bounds::new(0.0, 0.0, size.width, size.height);
    context.anchor(1, frame, Point { x: 1.0, y: 1.0 }, Point::ZERO);
    context.anchor(2, frame, Point { x: 0.0, y: 1.0 }, Point::ZERO);
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_anchors_inside_absolute_flex_cards_keep_leaf_sizes() {
        for scale in [1.0, 1.25, 2.0] {
            let mut demo = Demo::new(scale);
            demo.update(Instant::now(), RecordingMode::Retained);
            let geometry = |key| {
                demo.scene
                    .geometry()
                    .iter()
                    .find(|node| node.key.value == key)
                    .unwrap()
            };
            assert_eq!(geometry(1001).layout.width, 78.0);
            assert_eq!(geometry(1002).layout.width, 24.0);
            assert_eq!(geometry(1003).layout.height, 24.0);
            for key in [1001, 1002, 1003] {
                let child = geometry(key);
                let card = geometry(1000);
                assert_eq!(child.hit, child.paint);
                assert!(child.layout.y + child.layout.height <= card.layout.y + card.layout.height);
            }
        }
    }
}
