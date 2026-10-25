use crate::ui::Rect;
use retinue_ui::{
    composition::{Element, Key, RecordingMode, Scene},
    taffy::{geometry::Rect as TaffyRect, prelude::*},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Root,
    Shell,
    Header,
    Close,
    Grid,
    Card(usize),
    Footer,
}

impl From<Slot> for Key {
    fn from(slot: Slot) -> Self {
        let value = match slot {
            Slot::Root => 0,
            Slot::Shell => 1,
            Slot::Header => 2,
            Slot::Close => 3,
            Slot::Grid => 4,
            Slot::Footer => 5,
            Slot::Card(i) => 6 + i as u64,
        };
        Self::new("suggestion.layout", value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) shell: Rect,
    pub(super) header: Rect,
    pub(super) close: Rect,
    pub(super) grid: Rect,
    pub(super) cards: Vec<Rect>,
    pub(super) footer: Rect,
}

impl Layout {
    pub(super) fn scaled(mut self, scale: u32) -> Self {
        self.shell = self.shell.scaled(scale);
        self.header = self.header.scaled(scale);
        self.close = self.close.scaled(scale);
        self.grid = self.grid.scaled(scale);
        self.footer = self.footer.scaled(scale);
        self.cards = self
            .cards
            .into_iter()
            .map(|card| card.scaled(scale))
            .collect();
        self
    }
}

pub(super) fn compute(screen_width: u32, screen_height: u32, item_count: usize) -> Layout {
    build(screen_width, screen_height, item_count)
        .expect("relic suggestion layout should always be valid")
}

fn build(screen_width: u32, screen_height: u32, item_count: usize) -> Result<Layout, String> {
    let margin = 20.min(screen_width / 2).min(screen_height / 2);
    let width = 480.min(screen_width.saturating_sub(margin));
    let height = 220.min(screen_height.saturating_sub(margin));
    let inner_width = width.saturating_sub(4);
    let inner_height = height.saturating_sub(4);
    let header_height = 44.min(inner_height);
    let footer_height = 31.min(inner_height.saturating_sub(header_height));
    let grid_height = inner_height.saturating_sub(header_height + footer_height);
    let gap = 5;
    let padding = 5.min(inner_width / 2).min(grid_height / 2);
    let card_width = inner_width
        .saturating_sub(padding * 2 + gap)
        .checked_div(2)
        .unwrap_or(0);
    let card_height = grid_height
        .saturating_sub(padding * 2 + gap)
        .checked_div(2)
        .unwrap_or(0);

    let close = Element::new(
        Slot::Close,
        Style {
            size: Size {
                width: length(28.0_f32),
                height: length(28.0_f32),
            },
            margin: TaffyRect {
                left: zero(),
                right: length(7.0_f32),
                top: length(8.0_f32),
                bottom: zero(),
            },
            align_self: Some(AlignSelf::START),
            justify_self: Some(JustifySelf::END),
            ..Default::default()
        },
    );
    let header = Element::stack(
        Slot::Header,
        Style {
            size: Size {
                width: percent(1.0_f32),
                height: length(header_height as f32),
            },
            flex_shrink: 0.0,
            ..Default::default()
        },
        vec![close],
    );

    let card_count = item_count.min(4);
    let mut cards = Vec::with_capacity(card_count);
    for index in 0..card_count {
        cards.push(Element::new(
            Slot::Card(index),
            Style {
                size: Size {
                    width: length(card_width as f32),
                    height: length(card_height as f32),
                },
                ..Default::default()
            },
        ));
    }
    let mut grid = Element::grid(
        Slot::Grid,
        Style {
            size: Size {
                width: percent(1.0_f32),
                height: auto(),
            },
            min_size: Size {
                width: length(0.0_f32),
                height: length(0.0_f32),
            },
            flex_grow: 1.0,
            padding: TaffyRect {
                left: length(padding as f32),
                right: length(padding as f32),
                top: length(padding as f32),
                bottom: length(padding as f32),
            },
            gap: Size {
                width: length(gap as f32),
                height: length(gap as f32),
            },
            grid_template_columns: vec![length(card_width as f32), length(card_width as f32)],
            grid_template_rows: vec![length(card_height as f32), length(card_height as f32)],
            ..Default::default()
        },
        cards,
    );
    grid.clip = true;
    let footer = Element::new(
        Slot::Footer,
        Style {
            size: Size {
                width: percent(1.0_f32),
                height: length(footer_height as f32),
            },
            flex_shrink: 0.0,
            ..Default::default()
        },
    );
    let shell = Element::new(
        Slot::Shell,
        Style {
            flex_direction: FlexDirection::Column,

            size: Size {
                width: length(width as f32),
                height: length(height as f32),
            },
            border: TaffyRect {
                left: length(2.0_f32),
                right: length(2.0_f32),
                top: length(2.0_f32),
                bottom: length(2.0_f32),
            },
            ..Default::default()
        },
    )
    .with_children(vec![header, grid, footer]);
    let root = Element::new(
        Slot::Root,
        Style {
            size: Size {
                width: length(screen_width as f32),
                height: length(screen_height as f32),
            },
            padding: TaffyRect {
                left: zero(),
                right: length(margin as f32),
                top: length(margin as f32),
                bottom: zero(),
            },
            align_items: Some(AlignItems::START),
            justify_content: Some(JustifyContent::END),
            ..Default::default()
        },
    )
    .with_children(vec![shell]);
    let mut scene = Scene::default();
    scene.update(
        root,
        (screen_width, screen_height),
        1.0,
        RecordingMode::Retained,
    )?;
    let bounds = |slot: Slot| {
        scene
            .layout_rect(slot.into())
            .expect("suggestion layout node")
    };

    Ok(Layout {
        shell: bounds(Slot::Shell),
        header: bounds(Slot::Header),
        close: bounds(Slot::Close),
        grid: bounds(Slot::Grid),
        cards: (0..card_count)
            .map(|index| bounds(Slot::Card(index)))
            .collect(),
        footer: bounds(Slot::Footer),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_aleca_reference() {
        let layout = compute(2560, 1440, 4);
        assert_eq!(
            layout.shell,
            Rect {
                x: 2060,
                y: 20,
                width: 480,
                height: 220,
            }
        );
        assert_eq!(layout.header.height, 44);
        assert_eq!(
            layout.close,
            Rect {
                x: 2503,
                y: 30,
                width: 28,
                height: 28,
            }
        );
        assert_eq!(layout.grid.height, 141);
        assert_eq!(layout.cards.len(), 4);
        assert_eq!(layout.cards[0].width, 230);
        assert_eq!(layout.cards[0].height, 63);
        assert_eq!(layout.footer.height, 31);
    }
}
