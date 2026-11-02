use super::{card_layout::Positioned, card_layout::place, *};
use crate::{
    presentation::style::CARD, presentation::style::CARD_HOVER, presentation::widgets::AssetImage,
    presentation::widgets::ComponentImage,
};
use retinue_render::{painter::CachedFont, painter::text_width};

fn child(id: Key, name: &'static str) -> Key {
    Key::new(name, id.value)
}

pub(super) fn part_key(id: Key, index: usize, count: bool) -> Key {
    key(
        if count {
            "inventory.part.count"
        } else {
            "inventory.part"
        },
        (id, index),
    )
}

#[derive(Clone)]
struct Geometry {
    image: Bounds,
    title: Bounds,
    ducats: Bounds,
    ducat_icon: Bounds,
    amount: Bounds,
    vault_icon: Bounds,
    vault_label: Bounds,
    mastered: Bounds,
    parts: Vec<(Bounds, Bounds)>,
    sell: Bounds,
    buy: Bounds,
}

impl Geometry {
    fn new(cell: &Cell, size: LayoutSize, scale: f32, font: &CachedFont) -> Self {
        let (w, h) = (size.width, size.height);
        let rect = |x: f32, y: f32, w: f32, h: f32| {
            Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
        };
        let measure = |s: &str, size| text_width(font, s, size * scale) / scale;
        let item = &cell.item;
        let set = item.group == "sets";
        let rw = w - 111.0;
        let action = if set {
            114.0_f32.min(rw / 3.0)
        } else {
            (rw - 11.0) / 2.0
        };
        let sx = if set { 103.0 + rw - action } else { 103.0 };
        let sw = if set { action } else { rw };
        let sh = h - 80.0;
        let amount = measure(&format!("x{}", item.quantity), 14.0);
        let vault = has_vault(item);
        let vw = if vault {
            24.0 + if set {
                0.0
            } else {
                measure(vault_label(item), 14.0) + 4.0
            }
        } else {
            0.0
        };
        let group =
            amount + if vault { 8.0 + vw } else { 0.0 } + if item.mastered { 28.0 } else { 0.0 };
        let left = sx + (sw - group) / 2.0;
        let ducats = measure(&item.ducats.to_string(), 16.0);
        let dl = 8.0 + (90.0 - ducats - 26.0) / 2.0;
        let count = if set { item.components.len().min(6) } else { 0 };
        let columns = count.clamp(1, 3);
        let parts = (0..count)
            .map(|i| {
                let row_count = (count - i / columns * columns).min(columns);
                let group = row_count as f32 * 35.0 + (row_count - 1) as f32 * 6.0;
                let image = rect(
                    103.0 + (rw - action - 6.0 - group) / 2.0 + (i % columns) as f32 * 41.0,
                    37.0 + (h - 45.0 - if count > 3 { 76.0 } else { 35.0 }) / 2.0
                        + (i / columns) as f32 * 41.0,
                    35.0,
                    35.0,
                );
                let value = super::cards::compact_count(item.components[i].owned as f64);
                let width = text_width(font, &value, 12.5 * scale) / scale + 8.0;
                let badge = Bounds::new(
                    (image.x + image.width + 3.0 - width).max(0.0),
                    image.y + image.height - 16.0,
                    width + 1.0,
                    18.0,
                );
                (image, badge)
            })
            .collect();
        Self {
            image: rect(10.0, 10.0, 86.0, h - 44.0),
            title: rect(103.0, 8.0, rw, 27.0),
            ducats: rect(dl, h - 32.0, ducats + 1.0, 24.0),
            ducat_icon: rect(dl + ducats + 4.0, h - 31.0, 22.0, 22.0),
            amount: rect(left, 37.0, amount + 1.0, sh),
            vault_icon: rect(left + amount + 8.0, 37.0 + (sh - 24.0) / 2.0, 24.0, 24.0),
            vault_label: rect(left + amount + 36.0, 37.0, vw - 28.0 + 1.0, sh),
            mastered: rect(left + group - 20.0, 37.0 + (sh - 20.0) / 2.0, 20.0, 20.0),
            parts,
            sell: rect(sx, h - 43.0, action, 35.0),
            buy: rect(103.0 + action + 11.0, h - 43.0, action, 35.0),
        }
    }
}

fn has_vault(item: &ListingItem) -> bool {
    item.vaulted.is_some() && (item.is_prime || matches!(item.group.as_str(), "relics" | "sets"))
}

fn vault_label(item: &ListingItem) -> &'static str {
    if item.vaulted == Some(true) {
        "Vaulted"
    } else {
        "Unvaulted"
    }
}

#[derive(PartialEq)]
pub(super) struct InventoryCardView {
    pub id: Key,
    pub cell: State<Cell>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

impl View for InventoryCardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let id = self.id;
        let cell = self.cell.read();
        let m = self.metrics.read();
        let size = m.uniform_grid().cell();
        let g = Geometry::new(&cell, size, m.scale, &self.fonts.regular);
        let label = |text: String, size, font, center| {
            fitted_label(text, font, size, TEXT, center, m.scale)
        };
        ui! {
            LayoutView(Positioned::new(size)) {
                fill(if cell.hovered { CARD_HOVER } else { CARD }, 20.0)
                place(AssetImage::new(cell.item.asset.as_ref(), m.px(g.image.width.max(g.image.height))), g.image).id(child(id, "inventory.image"))
                place(label(cell.item.name.clone(), 17.0, self.fonts.medium.clone(), true), g.title)
                    .id(child(id, "inventory.title")).interactive()
                if cell.item.ducats > 0 {
                    place(label(cell.item.ducats.to_string(), 16.0, self.fonts.medium.clone(), false), g.ducats)
                    place(AssetImage::icon("ducats", m.px(g.ducat_icon.width)), g.ducat_icon).id(child(id, "inventory.ducats"))
                }
                place(label(format!("x{}", cell.item.quantity), 14.0, self.fonts.regular.clone(), false), g.amount)
                if has_vault(&cell.item) {
                    place(AssetImage::icon(if cell.item.vaulted == Some(true) { "vaulted" } else { "unvaulted" }, m.px(g.vault_icon.width)), g.vault_icon).id(child(id, "inventory.vault"))
                    if cell.item.group != "sets" {
                        place(label(vault_label(&cell.item).into(), 14.0, self.fonts.regular.clone(), false), g.vault_label)
                    }
                }
                if cell.item.mastered {
                    place(AssetImage::icon("owned", m.px(g.mastered.width)), g.mastered).id(child(id, "inventory.mastered"))
                }
                for (i, (image, count)) in g.parts.iter().enumerate() {
                    let part = &cell.item.components[i];
                    place(ComponentImage { asset: part.asset.clone(), owned: part.blueprint_owned > 0 || (part.required > 0 && part.owned >= part.required),
                        edge: m.px(image.width), scale: m.scale }, *image).id(part_key(id, i, false)).interactive()
                    place(slot(part_key(id, i, true)), *count)
                }
                if cell.item.market_name.as_ref().is_some_and(|s| !s.is_empty()) {
                    place(slot(child(id, "inventory.sell")), g.sell).interactive()
                    if cell.item.group != "sets" {
                        place(slot(child(id, "inventory.buy")), g.buy).interactive()
                    }
                }
            }.interactive()
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, id: Key, cell: &Cell) {
    let item = &cell.item;
    let index = cell.index;
    ui.view_hit(id, format!("list:item:{index}:menu"), &item.name, true);
    for (id, name) in [
        (child(id, "inventory.image"), "inventory.card.image"),
        (child(id, "inventory.title"), "inventory.card.title"),
    ] {
        if let Some(r) = ui.view_bounds(id) {
            ui.first_region(name, r);
        }
    }
    ui.view_hit(
        child(id, "inventory.title"),
        format!("list:item:{index}:menu"),
        &item.name,
        true,
    );
    if item.group == "sets" {
        for (i, part) in item.components.iter().take(6).enumerate() {
            let image = part_key(id, i, false);
            if let Some(r) = ui.view_bounds(image) {
                ui.first_region("inventory.card.component", r);
            }
            ui.view_hit(
                image,
                format!("list:item:{index}:component:{i}"),
                &part.name,
                true,
            );
            ui.view_control(part_key(id, i, true), |ui, r| {
                ui.fill(r, 9.0, crate::presentation::settings::rgb(0x141b2a));
                let pad = ui.px(4.0);
                ui.text(
                    Rect {
                        x: r.x + pad,
                        width: r.width.saturating_sub(2 * pad),
                        ..r
                    },
                    super::cards::compact_count(part.owned as f64),
                    12.5,
                    TEXT,
                );
            });
        }
    }
    ui.view_control(child(id, "inventory.sell"), |ui, r| {
        super::cards::quote_pill(
            ui,
            format!("list:item:{index}:buy"),
            r,
            if item.group == "sets" { "SELL" } else { "WTS" },
            &price(cell.quote.sell, cell.quote.resolved),
            if item.quantity > 0 {
                crate::presentation::style::SELL_QUOTE
            } else {
                crate::presentation::style::UNAVAILABLE_QUOTE
            },
        )
    });
    ui.view_control(child(id, "inventory.buy"), |ui, r| {
        super::cards::quote_pill(
            ui,
            format!("list:item:{index}:sell"),
            r,
            "WTB",
            &price(cell.quote.buy, cell.quote.resolved),
            crate::presentation::style::BUY_QUOTE,
        )
    });
}
