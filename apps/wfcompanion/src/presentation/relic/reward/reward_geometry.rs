use super::reward_view;
use crate::presentation::geometry::Rect;
use retinue_render::Key;
use retinue_ui::runtime::ViewTree;

const REFERENCE_HEIGHT: f64 = 1080.0;
const REFERENCE_WIDTH: f64 = 1000.0;
const REFERENCE_TOP: f64 = 630.0;
const WINDOW_HEIGHT: u32 = 355;
const WINDOW_CHROME_WIDTH: u32 = 20;
const WINDOW_OFFSET: f64 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CardSpec {
    pub(super) platinum_width: f32,
    pub(super) ducats_width: f32,
    pub(super) vaulted: bool,
    pub(super) favorite: bool,
}

impl Default for CardSpec {
    fn default() -> Self {
        Self {
            platinum_width: 42.0,
            ducats_width: 44.0,
            vaulted: false,
            favorite: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CardGeometry {
    pub(super) card: Rect,
    pub(super) name: Rect,
    pub(super) prices: Rect,
    pub(super) platinum: Rect,
    pub(super) vaulted: Option<Rect>,
    pub(super) ducats: Rect,
    pub(super) ownership: Rect,
    pub(super) components: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RewardGeometry {
    pub(super) window: Rect,
    pub(super) shell: Rect,
    pub(super) holder: Rect,
    pub(super) cards: Rect,
    pub(super) footer: Rect,
    pub(super) reward_cards: Vec<CardGeometry>,
}

impl RewardGeometry {
    pub(super) fn scaled(mut self, scale: u32) -> Self {
        self.window = self.window.scaled(scale);
        self.shell = self.shell.scaled(scale);
        self.holder = self.holder.scaled(scale);
        self.cards = self.cards.scaled(scale);
        self.footer = self.footer.scaled(scale);
        for card in &mut self.reward_cards {
            card.card = card.card.scaled(scale);
            card.name = card.name.scaled(scale);
            card.prices = card.prices.scaled(scale);
            card.platinum = card.platinum.scaled(scale);
            card.vaulted = card.vaulted.map(|bounds| bounds.scaled(scale));
            card.ducats = card.ducats.scaled(scale);
            card.ownership = card.ownership.scaled(scale);
            card.components = card.components.scaled(scale);
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
    Root,
    Shell,
    Cards,
    Holder,
    Footer,
    Card(usize),
    Name(usize),
    Prices(usize),
    Platinum(usize),
    Vaulted(usize),
    Favorite(usize),
    Ducats(usize),
    Ownership(usize),
    Components(usize),
}

impl From<Slot> for Key {
    fn from(slot: Slot) -> Self {
        let (kind, index) = match slot {
            Slot::Root => (0, 0),
            Slot::Shell => (1, 0),
            Slot::Cards => (2, 0),
            Slot::Holder => (3, 0),
            Slot::Footer => (4, 0),
            Slot::Card(i) => (5, i),
            Slot::Name(i) => (6, i),
            Slot::Prices(i) => (7, i),
            Slot::Platinum(i) => (8, i),
            Slot::Vaulted(i) => (9, i),
            Slot::Favorite(i) => (10, i),
            Slot::Ducats(i) => (11, i),
            Slot::Ownership(i) => (12, i),
            Slot::Components(i) => (13, i),
        };
        Self::new("reward.layout", kind + (index as u64) * 16)
    }
}

pub(super) fn compute(
    screen_width: u32,
    screen_height: u32,
    card_specs: &[CardSpec],
) -> Result<RewardGeometry, String> {
    let window = relic_overlay_bounds(screen_width, screen_height);
    let mut tree = ViewTree::new(
        reward_view::RewardView {
            window,
            cards: card_specs.iter().take(4).copied().collect(),
        },
        || {},
    );
    tree.update((screen_width, screen_height), 1.0)?;
    let bounds = |slot: Slot| {
        tree.geometry(tree.unique(Key::from(slot)).unwrap().unwrap())
            .unwrap()
            .hit
    };
    Ok(RewardGeometry {
        window,
        shell: bounds(Slot::Shell),
        holder: bounds(Slot::Holder),
        cards: bounds(Slot::Cards),
        footer: bounds(Slot::Footer),
        reward_cards: card_specs
            .iter()
            .take(4)
            .enumerate()
            .map(|(index, spec)| CardGeometry {
                card: bounds(Slot::Card(index)),
                name: bounds(Slot::Name(index)),
                prices: bounds(Slot::Prices(index)),
                platinum: bounds(Slot::Platinum(index)),
                vaulted: spec.vaulted.then(|| bounds(Slot::Vaulted(index))),
                ducats: bounds(Slot::Ducats(index)),
                ownership: bounds(Slot::Ownership(index)),
                components: bounds(Slot::Components(index)),
            })
            .collect(),
    })
}

fn relic_overlay_bounds(screen_width: u32, screen_height: u32) -> Rect {
    if screen_width == 0 || screen_height == 0 {
        return Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let from_1080 = f64::from(screen_height) / REFERENCE_HEIGHT;
    let content_width = (REFERENCE_WIDTH * from_1080).round().max(1.0) as u32;
    let content_width = content_width.min(screen_width);
    let width = content_width
        .saturating_add(WINDOW_CHROME_WIDTH)
        .min(screen_width);
    let offset = WINDOW_OFFSET.round() as u32;
    let x = ((screen_width - content_width) / 2)
        .saturating_sub(offset)
        .min(screen_width - width);
    let y = (REFERENCE_TOP * from_1080).round() as u32;
    let y = y.min(screen_height.saturating_sub(1));
    let height = WINDOW_HEIGHT.min(screen_height - y);
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards() -> [CardSpec; 4] {
        [CardSpec::default(); 4]
    }

    #[test]
    fn layout_matches_aleca_reference_at_1440p() {
        let layout = compute(2560, 1440, &cards()).unwrap();
        assert_eq!(
            layout.window,
            Rect {
                x: 598,
                y: 840,
                width: 1353,
                height: 355,
            }
        );
        assert_eq!(
            layout.shell,
            Rect {
                x: 613,
                y: 855,
                width: 1323,
                height: 325,
            }
        );
        assert_eq!(
            layout.holder,
            Rect {
                x: 615,
                y: 857,
                width: 1319,
                height: 270,
            }
        );
        assert_eq!(
            layout.cards,
            Rect {
                x: 622,
                y: 864,
                width: 1305,
                height: 256,
            }
        );
        assert_eq!(
            layout.footer,
            Rect {
                x: 615,
                y: 1127,
                width: 1319,
                height: 51,
            }
        );
        assert_eq!(layout.reward_cards[0].card.x, 622);
        assert_eq!(layout.reward_cards[1].card.x, 950);
        assert_eq!(layout.reward_cards[2].card.x, 1278);
        assert_eq!(layout.reward_cards[3].card.x, 1606);
        assert_eq!(
            layout.reward_cards[0].name,
            Rect {
                x: 628,
                y: 889,
                width: 309,
                height: 24,
            }
        );
        assert_eq!(
            layout.reward_cards[0].prices,
            Rect {
                x: 628,
                y: 940,
                width: 309,
                height: 30,
            }
        );
        assert_eq!(
            layout.reward_cards[0].platinum,
            Rect {
                x: 686,
                y: 941,
                width: 42,
                height: 28,
            }
        );
        assert_eq!(
            layout.reward_cards[0].ducats,
            Rect {
                x: 836,
                y: 940,
                width: 44,
                height: 30,
            }
        );
        assert_eq!(
            layout.reward_cards[0].ownership,
            Rect {
                x: 659,
                y: 988,
                width: 247,
                height: 27,
            }
        );
    }

    #[test]
    fn price_children_reflow_when_badges_exist() {
        let plain = compute(2560, 1440, &cards()).unwrap();
        let mut decorated = cards();
        decorated[0].vaulted = true;
        decorated[0].favorite = true;
        let decorated = compute(2560, 1440, &decorated).unwrap();
        assert!(decorated.reward_cards[0].platinum.x < plain.reward_cards[0].platinum.x);
        assert!(decorated.reward_cards[0].vaulted.is_some());
        assert!(decorated.reward_cards[0].ducats.x > plain.reward_cards[0].ducats.x);
    }

    #[test]
    fn platinum_box_grows_for_three_digit_price() {
        let mut specs = cards();
        specs[0].platinum_width = 68.0;
        let layout = compute(2560, 1440, &specs).unwrap();

        assert_eq!(layout.reward_cards[0].platinum.width, 68);
        assert!(layout.reward_cards[0].platinum.x < layout.reward_cards[0].ducats.x);
    }

    #[test]
    fn partial_squad_cards_are_centered_and_capped_at_quarter_width() {
        for count in 1..=3 {
            let specs = vec![CardSpec::default(); count];
            let layout = compute(2560, 1440, &specs).unwrap();
            let first = layout.reward_cards.first().unwrap().card;
            let last = layout.reward_cards.last().unwrap().card;
            let mut tree = ViewTree::new(
                reward_view::RewardView {
                    window: layout.window,
                    cards: specs,
                },
                || {},
            );
            tree.update((2560, 1440), 1.0).unwrap();
            let logical = |slot| {
                tree.geometry(tree.unique(Key::from(slot)).unwrap().unwrap())
                    .unwrap()
                    .layout
            };
            let cards = logical(Slot::Cards);
            let a = logical(Slot::Card(0));
            let b = logical(Slot::Card(count - 1));
            assert!((a.width - cards.width / 4.0).abs() < 0.001);
            assert!(((a.x - cards.x) - (cards.x + cards.width - b.x - b.width)).abs() < 0.001);
            assert!(first.width.abs_diff(layout.cards.width / 4) <= 1);
            assert!(
                (i64::from(first.x - layout.cards.x)
                    - i64::from(layout.cards.x + layout.cards.width - last.x - last.width))
                .abs()
                    <= 1
            );
        }
    }
}
