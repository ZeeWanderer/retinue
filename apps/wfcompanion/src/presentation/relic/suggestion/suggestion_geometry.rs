use super::suggestion_view;
use crate::presentation::geometry::Rect;
use retinue_render::Key;
use retinue_ui::runtime::ViewTree;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
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
pub(super) struct SuggestionGeometry {
    pub(super) shell: Rect,
    pub(super) header: Rect,
    pub(super) close: Rect,
    pub(super) grid: Rect,
    pub(super) cards: Vec<Rect>,
    pub(super) footer: Rect,
}

impl SuggestionGeometry {
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

pub(super) fn compute(
    screen_width: u32,
    screen_height: u32,
    item_count: usize,
) -> SuggestionGeometry {
    build(screen_width, screen_height, item_count)
        .expect("relic suggestion layout should always be valid")
}

fn build(
    screen_width: u32,
    screen_height: u32,
    item_count: usize,
) -> Result<SuggestionGeometry, String> {
    let card_count = item_count.min(4);
    let mut tree = ViewTree::new(
        suggestion_view::SuggestionView {
            width: screen_width,
            height: screen_height,
            count: card_count,
        },
        || {},
    );
    tree.update((screen_width, screen_height), 1.0)?;
    let bounds = |slot: Slot| {
        tree.geometry(tree.unique(Key::from(slot)).unwrap().unwrap())
            .unwrap()
            .hit
    };
    Ok(SuggestionGeometry {
        shell: bounds(Slot::Shell),
        header: bounds(Slot::Header),
        close: bounds(Slot::Close),
        grid: bounds(Slot::Grid),
        cards: (0..card_count).map(|i| bounds(Slot::Card(i))).collect(),
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
