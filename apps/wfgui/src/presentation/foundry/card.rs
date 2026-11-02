use super::*;
use retinue_render::{Bounds, Color, Command, TextAlignment};
use retinue_ui::{drawing::CanvasContent, drawing::DrawList, drawing::DrawingContext};

#[derive(Clone, PartialEq)]
pub(super) struct CardLayout {
    pub title: (Rect, Rect, Rect),
    pub hero: Rect,
    pub subsumed: Rect,
    pub parts: Vec<PartLayout>,
}

#[derive(Clone, PartialEq)]
pub(super) struct PartLayout {
    pub image: Rect,
    pub count: Rect,
    pub relic: Rect,
}

impl CardLayout {
    pub fn new(view: &Foundry, card: &Card) -> Self {
        let warframe = card.group == "warframe";
        let diameter = view.px(if warframe { 105.0 } else { 86.0 });
        let hero = Rect {
            x: view.px(10.0) + view.px(3.0),
            y: if warframe {
                view.grid.height - diameter
            } else {
                view.px(43.0)
            },
            width: diameter,
            height: diameter,
        };
        Self {
            title: view.title_rects(card),
            hero,
            subsumed: Rect {
                x: hero.x + hero.width - view.px(27.0),
                y: view.grid.height - view.px(29.0),
                width: view.px(25.0),
                height: view.px(25.0),
            },
            parts: view
                .parts(card.components.len())
                .into_iter()
                .enumerate()
                .map(|(i, image)| {
                    let component = &card.components[i];
                    PartLayout {
                        image,
                        count: view.count_badge(
                            image,
                            &component.owned.to_string(),
                            component.owned_relic,
                        ),
                        relic: view.relic_badge(image),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Content {
    pub frame: Bounds,
    pub scale: f32,
    pub layout: std::rc::Rc<CardLayout>,
    pub title: DrawList,
    pub artwork: DrawList,
    pub owned: bool,
    pub hovered: bool,
    pub relics: [bool; 6],
}

impl Content {
    pub fn new(
        view: &Foundry,
        card: &Card,
        index: usize,
        layout: CardLayout,
        title: DrawList,
        artwork: DrawList,
    ) -> Self {
        Self {
            frame: bounds(
                Rect {
                    width: view.grid.width,
                    height: view.grid.height,
                    ..Rect::default()
                },
                view.scale,
            ),
            scale: view.scale,
            layout: std::rc::Rc::new(layout),
            title,
            artwork,
            owned: card.owned,
            hovered: view.hovered_card() == Some(index),
            relics: std::array::from_fn(|i| card.components.get(i).is_some_and(|c| c.owned_relic)),
        }
    }
}

pub(super) fn title(view: &Foundry, card: &Card, layout: &CardLayout) -> Vec<Command> {
    let rect = layout.title.1;
    vec![Command::Text {
        bounds: Bounds::new(
            0.0,
            0.0,
            rect.width as f32 / view.scale,
            rect.height as f32 / view.scale,
        ),
        size: 20.0,
        text: title_text(view, card, rect.width),
        font: view.regular.clone(),
        color: Color::from_bgra(TEXT),
        align: TextAlignment::Center,
    }]
}

pub(super) fn bounds(rect: Rect, scale: f32) -> Bounds {
    Bounds::new(
        rect.x as f32 / scale,
        rect.y as f32 / scale,
        rect.width as f32 / scale,
        rect.height as f32 / scale,
    )
}

#[derive(PartialEq)]
pub(super) struct Background {
    pub owned: bool,
    pub hovered: bool,
}

impl CanvasContent for Background {
    fn draw(&self, context: &mut DrawingContext) {
        let bounds = context.bounds;
        context.fill(
            bounds,
            15.0,
            Color::from_bgra(if self.hovered { CARD_HOVER } else { CARD }),
        );
        if self.owned {
            context.fill(
                bounds,
                15.0,
                Color::from_bgra(if self.hovered {
                    crate::presentation::style::OWNED_CARD_HOVER
                } else {
                    crate::presentation::style::OWNED_CARD
                }),
            );
            context.emit(Command::InsetShadow {
                bounds,
                radius: 15.0,
                blur: 7.0,
                spread: 3.0,
                color: Color::from_bgra(if self.hovered {
                    crate::presentation::style::OWNED_EDGE_HOVER
                } else {
                    crate::presentation::style::OWNED_EDGE
                }),
            });
        }
    }
}

fn title_text(view: &Foundry, card: &Card, width: u32) -> String {
    let mut name = card.name.clone();
    if text_width(&view.regular, &name, 20.0 * view.scale) > width as f32 {
        while !name.is_empty()
            && text_width(&view.regular, &format!("{name}..."), 20.0 * view.scale) > width as f32
        {
            name.pop();
        }
        name.push_str("...");
    }
    name
}
