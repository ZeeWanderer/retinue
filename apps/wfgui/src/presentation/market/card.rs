use super::*;
use crate::presentation::widgets::aligned_label;
use retinue_render::{painter::CachedFont, painter::text_width};

const ACTIONS: [(&str, &str, &str); 5] = [
    ("list", "search", "View listings"),
    ("edit", "pencil", "Edit order"),
    ("add", "plus", "Add one"),
    ("close", "check", "Complete one"),
    ("delete", "trash", "Delete order"),
];

fn child(id: Key, name: &'static str) -> Key {
    Key::new(name, id.value)
}
fn action(id: Key, part: &'static str, index: usize) -> Key {
    key(part, (id, index))
}

#[derive(Clone, PartialEq)]
pub(super) struct Card {
    pub source: usize,
    pub id: String,
    name: String,
    asset: Option<Asset>,
    visible: bool,
    selected: bool,
    sell: bool,
    missing: bool,
    owned: String,
    quantity: String,
    price: String,
    extra: String,
    comparison: String,
}

impl Card {
    pub(super) fn new(row: &Order, market: &Market) -> Self {
        let order = &entries(&market.account["orders"])[row.source];
        let id = text(order, "id");
        let item = market.catalog.get(item_id(order)).unwrap_or(&Value::Null);
        let sell = order["type"] == "sell";
        let comparison = market.comparisons.get(id);
        let value =
            comparison.and_then(|q| q[if sell { "lowest_sell" } else { "highest_buy" }].as_f64());
        let mut extra = Vec::new();
        if let Some(rank) = order["rank"].as_u64() {
            extra.push(format!("Rank {rank}"));
        }
        if let Some(charges) = order["charges"].as_u64() {
            extra.push(format!("{charges} charges"));
        }
        if !text(order, "subtype").is_empty() {
            extra.push(text(order, "subtype").to_owned());
        }
        for (key, label) in [("amberStars", "amber"), ("cyanStars", "cyan")] {
            if let Some(value) = order[key].as_u64() {
                extra.push(format!("{value} {label}"));
            }
        }
        if let Some(value) = order["perTrade"].as_u64().filter(|v| *v > 1) {
            extra.push(format!("{value} per trade"));
        }
        Self {
            source: row.source,
            id: id.into(),
            name: order_name(order, market).into(),
            asset: asset(&item["asset"]),
            visible: order["visible"] == true,
            selected: market.selected.contains(id),
            sell,
            missing: row.missing,
            owned: row
                .owned
                .map(|n| format!("{n:.0} owned"))
                .unwrap_or_else(|| "-- owned".into()),
            quantity: format!("{:.0}", number(order, "quantity")),
            price: format!("{:.0}", number(order, "platinum")),
            extra: extra.join(" / "),
            comparison: format!(
                "{}: {}",
                if sell { "Lowest WTS" } else { "Highest WTB" },
                listing::price(value, comparison.is_some())
            ),
        }
    }
}

#[derive(Clone)]
struct Geometry {
    title: Bounds,
    owned: Bounds,
    select: Bounds,
    warning: Bounds,
    numbers: [(Bounds, Bounds); 2],
    extra: Bounds,
    comparison: Bounds,
    actions: [(Bounds, Bounds, Bounds); 5],
}

impl Geometry {
    fn new(card: &Card, size: LayoutSize, scale: f32, font: &CachedFont) -> Self {
        let m = Metrics {
            width: size.width,
            height: size.height,
            scale,
        };
        let w = size.width;
        let sw = text_width(font, &card.owned, 13.33 * scale) / scale;
        let trailing = if card.missing { 37.0 } else { 7.0 };
        let column = (w - 84.0) / 3.0;
        let numbers = [&card.quantity, &card.price]
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                let tw = text_width(font, value, 14.67 * scale) / scale;
                let x = 75.0 + i as f32 * column + (column - tw - 25.0) / 2.0;
                (
                    m.bounds(x, 31.0, tw + 1.0, 33.0),
                    m.bounds(x + tw + 5.0, 38.0, 20.0, 20.0),
                )
            })
            .collect::<Vec<_>>();
        Self {
            title: m.bounds(42.0, 0.0, w - 42.0 - sw - 12.0 - trailing - 25.0, 28.0),
            owned: m.bounds(w - trailing - sw, 0.0, sw + 1.0, 28.0),
            select: m.bounds(w - trailing - sw - 30.0, 1.0, 24.0, 26.0),
            warning: m.bounds(w - 37.0, 0.0, 37.0, 28.0),
            numbers: numbers.try_into().unwrap(),
            extra: m.bounds(75.0 + 2.0 * column, 31.0, column, 33.0),
            comparison: m.bounds(105.0, 70.0, (w - 240.0).max(1.0), 30.0),
            actions: std::array::from_fn(|i| {
                let x = if i == 0 {
                    76.0
                } else {
                    w - 124.0 + (i - 1) as f32 * 29.0
                };
                let r = m.bounds(x - 3.0, 71.0, 28.0, 28.0);
                let pad = 1.0;
                (
                    r,
                    Bounds::new(
                        r.x - pad,
                        r.y - pad,
                        r.width + pad * 2.0,
                        r.height + pad * 2.0,
                    ),
                    m.bounds(x, 74.0, 21.0, 21.0),
                )
            }),
        }
    }
}

#[derive(PartialEq)]
pub(super) struct OrderCardView {
    pub id: Key,
    pub card: State<Card>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

impl View for OrderCardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let id = self.id;
        let card = self.card.read();
        let metrics = self.metrics.read();
        let size = metrics.uniform_grid().cell();
        let m = Metrics {
            width: size.width,
            height: size.height,
            scale: metrics.scale,
        };
        let g = Geometry::new(&card, size, m.scale, &self.fonts.regular);
        let label = |text: &str, size, center| {
            fitted_label(
                text,
                self.fonts.regular.clone(),
                size,
                TEXT,
                center,
                m.scale,
            )
        };
        ui! {
            ZStack() {
                fill(crate::presentation::style::CARD, 18.0)
                place(fill(crate::presentation::style::SUMMARY_HEADER, 18.0), Bounds::new(0.0, 0.0, m.width, 28.0))
                place(fill(crate::presentation::style::SUMMARY_HEADER, 0.0), m.bounds(0.0, 18.0, m.width, 10.0))
                place(slot(child(id, "market.visibility.icon")), m.bounds(9.0, 4.0, 20.0, 20.0))
                place(slot(child(id, "market.visibility.hit")), m.bounds(0.0, 0.0, 37.0, 28.0)).interactive()
                place(label(&card.name, 14.67, false), g.title)
                place(aligned_label(&card.owned, self.fonts.light.clone(), 13.33, MUTED, 0.0, m.scale), g.owned)
                if card.missing {
                    place(fill(crate::presentation::settings::rgb(0x9a3737), 18.0), g.warning)
                    place(fill(crate::presentation::settings::rgb(0x9a3737), 0.0), m.bounds(m.width - 37.0, 0.0, 18.0, 28.0))
                    place(fill(crate::presentation::settings::rgb(0x9a3737), 0.0), m.bounds(m.width - 37.0, 18.0, 37.0, 10.0))
                    place(label("!", 16.0, true), g.warning).id(child(id, "market.warning")).interactive()
                }
                place(slot(child(id, "market.select")), g.select).interactive()
                place(fill(crate::presentation::style::INSET_PANEL, 18.0), m.bounds(0.0, 28.0, 70.0, 79.0))
                place(fill(crate::presentation::style::INSET_PANEL, 0.0), m.bounds(0.0, 28.0, 70.0, 18.0))
                place(fill(crate::presentation::style::INSET_PANEL, 0.0), m.bounds(52.0, 28.0, 18.0, 79.0))
                place(slot(child(id, "market.image")), m.bounds(13.0, 32.0, 44.0, 44.0))
                place(fill([0, 0, 0, 190], 8.0), m.bounds(12.0, 83.0, 46.0, 20.0))
                place(label(if card.sell { "WTS" } else { "WTB" }, 13.33, true), m.bounds(12.0, 83.0, 46.0, 20.0))
                place(label(&card.quantity, 14.67, false), g.numbers[0].0)
                place(slot(child(id, "market.quantity.icon")), g.numbers[0].1)
                place(label(&card.price, 14.67, false), g.numbers[1].0)
                place(slot(child(id, "market.price.icon")), g.numbers[1].1)
                place(label(&card.extra, 14.0, true), g.extra).id(child(id, "market.details")).interactive()
                place(label(&card.comparison, 13.33, false), g.comparison)
                for (i, (hit, outline, icon)) in g.actions.iter().enumerate() {
                    place(slot(action(id, "market.action.outline", i)), *outline)
                    place(slot(action(id, "market.action.icon", i)), *icon)
                    place(slot(action(id, "market.action.hit", i)), *hit).interactive()
                }
            }.alignment(Alignment::Start).frame(size.width, size.height)
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, id: Key, card: &Card, busy: bool) {
    let command = |action| format!("market:order:{}:{action}", card.id);
    ui.view_control(child(id, "market.visibility.icon"), |ui, r| {
        ui.bitmap(r, market_icon(if card.visible { "eye" } else { "eye-off" }))
    });
    ui.view_hit(
        child(id, "market.visibility.hit"),
        command("visibility"),
        if card.visible {
            "Hide order"
        } else {
            "Make order public"
        },
        !busy,
    );
    ui.view_hit(
        child(id, "market.warning"),
        command("warning"),
        "Order quantity exceeds owned quantity",
        false,
    );
    ui.view_control(child(id, "market.select"), |ui, r| {
        ui.toggle(command("select"), r, "", card.selected)
    });
    ui.view_control(child(id, "market.image"), |ui, r| {
        ui.image(r, card.asset.as_ref())
    });
    ui.view_control(child(id, "market.quantity.icon"), |ui, r| {
        ui.bitmap(r, market_icon("boxes"))
    });
    ui.view_icon(child(id, "market.price.icon"), "platinum", None);
    ui.view_hit(
        child(id, "market.details"),
        command("details"),
        &card.extra,
        false,
    );
    for (i, (name, icon, label)) in ACTIONS.iter().enumerate() {
        ui.view_control(action(id, "market.action.outline", i), |ui, r| {
            let pad = ui.px(1.0);
            ui.outline(
                Rect {
                    x: pad,
                    y: pad,
                    width: r.width.saturating_sub(2 * pad),
                    height: r.height.saturating_sub(2 * pad),
                },
                6.0,
                [255, 255, 255, 184],
            );
        });
        ui.view_control(action(id, "market.action.icon", i), |ui, r| {
            ui.bitmap(r, market_icon(icon))
        });
        ui.view_hit(
            action(id, "market.action.hit", i),
            command(name),
            *label,
            !busy,
        );
    }
}
