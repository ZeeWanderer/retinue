use super::{card_layout::Positioned, card_layout::place, *};
use crate::{presentation::widgets::AssetImage, presentation::widgets::ComponentImage};
use retinue_ui::{drawing::CanvasContent, drawing::DrawingContext};

#[derive(PartialEq)]
struct SearchRing(f32);

impl CanvasContent for SearchRing {
    fn draw(&self, context: &mut DrawingContext) {
        context.outline(
            context.bounds,
            (18.5 * self.0).round() / self.0,
            self.0.round() / self.0,
            Color::from_bgra(crate::presentation::style::SEARCH_HIGHLIGHT),
        );
    }
}

fn child(id: Key, name: &'static str) -> Key {
    Key::new(name, id.value)
}
fn reward(id: Key, index: usize) -> Key {
    key("relic.reward", (id, index))
}
fn ring(id: Key, index: usize) -> Key {
    key("relic.reward.ring", (id, index))
}
fn refinement(id: Key, index: usize, column: u8) -> Key {
    key("relic.refinement", (id, index, column))
}

#[derive(Clone)]
struct Geometry {
    image: Bounds,
    amount: Bounds,
    vault: Bounds,
    title: Bounds,
    platinum: Bounds,
    ducats: Bounds,
    rows: Vec<(usize, [Bounds; 3])>,
    rewards: Vec<Bounds>,
    font: f32,
}
impl Geometry {
    fn new(cell: &Cell, size: LayoutSize, context: &CardContext) -> Self {
        let (w, h) = (size.width, size.height);
        let rect = |x: f32, y: f32, w: f32, h: f32| {
            Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
        };
        let middle = (w - 239.0).clamp(0.0, 220.0);
        let left = (w - 80.0 - middle - 127.0 - 20.0) / 2.0;
        let refinements = cell
            .item
            .refinements
            .iter()
            .enumerate()
            .filter(|(_, r)| !context.owned_only || r.amount_owned > 0)
            .take(4)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let rh = if refinements.len() >= 4 { 16.0 } else { 18.0 };
        let top = (h - 23.0 - refinements.len() as f32 * rh) / 2.0;
        let mid_x = left + 90.0;
        let values = mid_x + middle - 68.0;
        Self {
            image: rect(left + 4.0, 8.0, 72.0, h - 16.0),
            amount: rect(left + 18.0, h - 25.0, 44.0, 20.0),
            vault: rect(left + 2.0, 8.0, 23.0, 23.0),
            title: rect(mid_x, top, (middle - 74.0).max(1.0), 23.0),
            platinum: rect(values + 8.0, top + 2.0, 19.0, 19.0),
            ducats: rect(values + 42.0, top + 2.0, 19.0, 19.0),
            font: if refinements.len() >= 4 { 12.0 } else { 13.33 },
            rows: refinements
                .into_iter()
                .enumerate()
                .map(|(i, index)| {
                    let y = top + 23.0 + i as f32 * rh;
                    (
                        index,
                        [
                            rect(mid_x, y, (middle - 74.0).max(1.0), rh),
                            rect(values, y, 34.0, rh),
                            rect(values + 34.0, y, 34.0, rh),
                        ],
                    )
                })
                .collect(),
            rewards: (0..cell.item.rewards.len().min(6))
                .map(|i| {
                    rect(
                        mid_x + middle + 14.0 + (i % 3) as f32 * 41.0,
                        (h - 78.0) / 2.0 + (i / 3) as f32 * 41.0,
                        37.0,
                        37.0,
                    )
                })
                .collect(),
        }
    }
}

#[derive(PartialEq)]
pub(super) struct RelicCardView {
    pub id: Key,
    pub cell: State<Cell>,
    pub metrics: State<Metrics>,
    pub context: State<CardContext>,
    pub fonts: Fonts,
}

#[view]
impl View for RelicCardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let id = self.id;
        let cell = self.cell.read();
        let m = self.metrics.read();
        let context = self.context.read();
        let size = m.uniform_grid().cell();
        let g = Geometry::new(&cell, size, &context);
        ui! {
            LayoutView(Positioned::new(size)) {
                fill(if cell.hovered { crate::presentation::style::CARD_HOVER } else { controls::CARD }, 12.0)
                place(compose(AssetImage::new(cell.item.asset.as_ref(), m.px(g.image.width.max(g.image.height)))), g.image).named(child(id, "relic.image"))
                place(label(format!("x{}", cell.item.amount_owned), self.fonts.medium.clone(), 15.0, TEXT, true), g.amount)
                if cell.item.vaulted == Some(true) {
                    place(compose(AssetImage::icon("vaulted", m.px(g.vault.width.max(g.vault.height)))), g.vault).named(child(id, "relic.vault"))
                }
                place(label(cell.item.name.clone(), self.fonts.medium.clone(), 19.0, TEXT, false), g.title)
                    .named(child(id, "relic.title"))
                place(compose(AssetImage::icon("platinum", m.px(g.platinum.width.max(g.platinum.height)))), g.platinum).named(child(id, "relic.platinum"))
                place(compose(AssetImage::icon("ducats", m.px(g.ducats.width.max(g.ducats.height)))), g.ducats).named(child(id, "relic.ducats"))
                ForEach(g.rows.iter().enumerate(), |(_, (index, _))| *index as u64) { |(i, (index, bounds))|
                    let r = &cell.item.refinements[*index];
                    place(label(format!("{}x {}", r.amount_owned, r.refinement), self.fonts.regular.clone(),
                        g.font, TEXT, false), bounds[0]).named(refinement(id, i, 0))
                    place(label(price(r.expected_platinum, context.prices_ready), self.fonts.regular.clone(),
                        g.font, TEXT, true), bounds[1]).named(refinement(id, i, 1))
                    place(label(format!("{:.0}", r.expected_ducats), self.fonts.regular.clone(),
                        g.font, TEXT, true), bounds[2]).named(refinement(id, i, 2))
                }
                ForEach(g.rewards.iter().take(6).enumerate(), |(i, _)| *i as u64) { |(i, bounds)|
                    let item = &cell.item.rewards[i];
                    place(compose(ComponentImage { asset: item.asset.clone(), owned: item.owned > 0,
                        edge: m.px(bounds.width), scale: m.scale }), *bounds).named(reward(id, i)).interactive()
                    if !context.query.is_empty() && item.name.to_lowercase().contains(&context.query) {
                        place(Canvas::new(SearchRing(m.scale)), *bounds).named(ring(id, i))
                    }
                }
            }.interactive()
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, id: Key, cell: &Cell) {
    let item = &cell.item;
    let index = cell.index;
    ui.view_hit(id, format!("list:item:{index}:menu"), "", true);
    for (id, name) in [
        (child(id, "relic.image"), "relic.card.image"),
        (child(id, "relic.title"), "relic.card.title"),
        (refinement(id, 0, 0), "relic.card.refinement"),
    ] {
        if let Some(r) = ui.view_bounds(id) {
            ui.first_region(name, r);
        }
    }
    for (i, item) in item.rewards.iter().take(6).enumerate() {
        let reward = reward(id, i);
        if let Some(r) = ui.view_bounds(reward) {
            ui.first_region("relic.card.reward", r);
        }
        ui.view_hit(
            reward,
            format!("list:item:{index}:reward:{i}"),
            format!(
                "{} - {:.2}% - {} ducats",
                item.name, item.chance, item.ducats
            ),
            !item.slug.as_deref().unwrap_or("").is_empty(),
        );
    }
}
