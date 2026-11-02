mod orders_view;
mod rail_view;
use super::*;
use crate::{
    presentation::widgets::fill, presentation::widgets::key, presentation::widgets::label,
    presentation::widgets::place, presentation::widgets::slot,
};
use retinue_render::{Bounds, CacheHint, Color, Key, painter::CachedFont};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};
use std::rc::Rc;

#[cfg(test)]
mod tests;

const SEARCH: Key = Key::new("market.rail.search", 0);
const IMAGE: Key = Key::new("market.rail.image", 0);
const REFRESH: Key = Key::new("market.rail.refresh", 0);
const REFRESH_ICON: Key = Key::new("market.rail.refresh.icon", 0);
const VIEWPORT: Key = Key::new("market.rail.viewport", 0);
const SCROLL: Key = Key::new("market.rail.scroll", 0);
const PRICE: Key = Key::new("market.rail.price", 0);
const QUANTITY: Key = Key::new("market.rail.quantity", 0);
const POST: Key = Key::new("market.rail.post", 0);
const WEBSITE: Key = Key::new("market.rail.website", 0);

#[derive(Clone, Default, PartialEq)]
struct Detail {
    present: bool,
    name: String,
    authenticated: bool,
    error: Option<String>,
    loading: bool,
}

#[derive(Clone, PartialEq)]
enum Editor {
    Choice(Vec<(String, String)>),
    Number(u32, u32),
}

#[derive(Clone, PartialEq)]
struct Filter {
    action: String,
    label: String,
    value: String,
    editor: Editor,
    bounds: Bounds,
}

#[derive(Clone, PartialEq)]
struct Order {
    index: usize,
    name: String,
    status: String,
    quantity: String,
    price: String,
}

#[derive(Clone, PartialEq)]
struct Suggestion {
    index: usize,
    name: String,
    selected: bool,
}

pub(in crate::presentation) struct MarketRail {
    area: State<(f32, f32)>,
    detail: State<Detail>,
    filters: Items<Filter>,
    orders: Items<Order>,
    suggestions: Items<Suggestion>,
    offset: State<f32>,
}

fn viewport((w, h): (f32, f32), filters: usize, detail: &Detail) -> Bounds {
    let top =
        168.0 + filters as f32 * 37.0 + if detail.error.is_some() { 46.0 } else { 0.0 } + 29.0;
    let bottom = if detail.authenticated { 112.0 } else { 44.0 };
    Bounds::new(7.0, top, (w - 14.0).max(0.0), (h - top - bottom).max(0.0))
}

impl MarketRail {
    pub fn new() -> Self {
        Self {
            area: State::new((0.0, 0.0)),
            detail: State::default(),
            filters: Items::default(),
            orders: Items::default(),
            suggestions: Items::default(),
            offset: State::new(0.0),
        }
    }

    pub fn view(&self, regular: Rc<CachedFont>, bold: Rc<CachedFont>) -> Element {
        rail_view::MarketRailView {
            area: self.area.clone(),
            detail: self.detail.clone(),
            filters: self.filters.clone(),
            orders: self.orders.clone(),
            suggestions: self.suggestions.clone(),
            offset: self.offset.clone(),
            regular: TextStyle::new(regular, 14.0, Color::from_bgra(TEXT)),
            bold: TextStyle::new(bold, 14.0, Color::from_bgra(TEXT)),
        }
        .into_element()
    }
}

impl Pages {
    pub(in crate::presentation) fn sync_market_rail(&mut self, now: Instant) {
        let m = &self.market;
        let item = m.item();
        let view = &self.market_rail_view;
        let area = (
            self.rail.size.0 as f32 / self.rail.scale,
            self.rail.size.1 as f32 / self.rail.scale,
        );
        view.area.set(area);
        let detail = Detail {
            present: !m.item.is_empty(),
            name: if text(item, "name").is_empty() {
                m.query.clone()
            } else {
                text(item, "name").to_owned()
            },
            authenticated: m.account["authenticated"] == true,
            error: m.error.clone(),
            loading: self.pending.contains_key("market:quote"),
        };
        view.detail.set(detail.clone());
        let mut filters = Vec::new();
        let max_rank = number(item, "max_rank") as u32;
        if max_rank > 0 {
            filters.push(Filter {
                action: "market:rank".into(),
                label: "Rank".into(),
                value: m.rank.clone(),
                editor: Editor::Choice(
                    (0..=max_rank)
                        .map(|n| (n.to_string(), n.to_string()))
                        .collect(),
                ),
                bounds: Bounds::new(61.0, 0.0, 66.0, 30.0),
            });
        }
        let subtypes = entries(&item["subtypes"])
            .iter()
            .filter_map(Value::as_str)
            .map(|s| (s.to_owned(), s.to_owned()))
            .collect::<Vec<_>>();
        if !subtypes.is_empty() {
            filters.push(Filter {
                action: "market:subtype".into(),
                label: String::new(),
                value: m.subtype.clone(),
                editor: Editor::Choice(subtypes),
                bounds: Bounds::new(0.0, 0.0, 150.0, 30.0),
            });
        }
        for (key, maximum, label) in VARIANTS {
            let max = number(item, maximum) as u32;
            if max > 0 {
                filters.push(Filter {
                    action: format!("market:variant:{key}"),
                    label: label.into(),
                    value: m.variants.get(key).cloned().unwrap_or_else(|| "0".into()),
                    editor: Editor::Number(0, max),
                    bounds: Bounds::new(110.0, 0.0, 96.0, 30.0),
                });
            }
        }
        if item["bulk_tradable"] == true {
            filters.push(Filter {
                action: "market:per-trade".into(),
                label: "Per trade".into(),
                value: m.per_trade.clone(),
                editor: Editor::Number(1, 6),
                bounds: Bounds::new(110.0, 0.0, 96.0, 30.0),
            });
        }
        let frame = viewport(area, filters.len(), &detail);
        view.filters
            .replace(
                filters
                    .into_iter()
                    .map(|f| (key("market.filter", &f.action), f)),
            )
            .expect("unique market filters");
        let mut occurrences = HashMap::new();
        view.orders
            .replace(
                entries(
                    &m.quote[if m.side == "sell" {
                        "sell_orders"
                    } else {
                        "buy_orders"
                    }],
                )
                .iter()
                .enumerate()
                .map(|(index, o)| {
                    let user = &o["user"];
                    let name = if text(user, "ingame_name").is_empty() {
                        text(user, "ingameName")
                    } else {
                        text(user, "ingame_name")
                    };
                    let identity = (text(o, "id"), name);
                    let occurrence = occurrences.entry(identity).or_insert(0_u32);
                    let id = key("market.order", (identity, *occurrence));
                    *occurrence += 1;
                    (
                        id,
                        Order {
                            index,
                            name: name.into(),
                            status: text(user, "status").into(),
                            quantity: format!("{:.0}", number(o, "quantity")),
                            price: format!("{:.0}", number(o, "platinum")),
                        },
                    )
                }),
            )
            .expect("unique market listings");
        let suggestions = if m.suggestions { m.suggestions() } else { &[] };
        view.suggestions
            .replace(suggestions.iter().enumerate().map(|(index, item)| {
                (
                    key("market.suggestion", (item_id(item), index)),
                    Suggestion {
                        index,
                        name: text(item, "name").into(),
                        selected: m.suggestion == Some(index),
                    },
                )
            }))
            .expect("unique market suggestions");
        self.rail_scroll.resize(
            self.rail.px(view.orders.order().len() as f32 * 44.0) as f32,
            self.rail.px(frame.height) as f32,
            now,
        );
        self.rail_offset = self.rail_scroll.offset(now).round() as u32;
        view.offset.set(self.rail_offset as f32 / self.rail.scale);
    }

    pub(in crate::presentation) fn bind_market_rail(&mut self) {
        let m = &self.market;
        let ui = &mut self.rail;
        ui.view_control(SEARCH, |ui, rect| {
            ui.styled_search_field(
                "market:query",
                rect,
                &m.query,
                "Search Warframe.market",
                crate::presentation::widgets::search::Style::Fill(
                    crate::presentation::widgets::search::Anchor::Trailing,
                ),
            );
        });
        ui.view_control(IMAGE, |ui, rect| {
            ui.image(rect, asset(&m.item()["asset"]).as_ref())
        });
        for (i, (side, label)) in [("sell", "WTS"), ("buy", "WTB")].into_iter().enumerate() {
            ui.view_control(Key::new("market.rail.side", i as u64), |ui, rect| {
                ui.button(
                    format!("market:mode:{side}"),
                    rect,
                    label,
                    m.side == side,
                    true,
                )
            });
        }
        ui.view_icon(REFRESH_ICON, "refresh", None);
        ui.view_hit(REFRESH, "market:quote-refresh", "Refresh listings", true);
        for &id in self.market_rail_view.filters.order().keys() {
            let filter = self.market_rail_view.filters.get(id).unwrap();
            ui.view_control(
                Key::new("market.filter.control", id.value),
                |ui, rect| match &filter.editor {
                    Editor::Choice(options) => ui.choice(
                        &filter.action,
                        rect,
                        &filter.value,
                        &options
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect::<Vec<_>>(),
                    ),
                    Editor::Number(min, max) => ui.number_field(
                        &filter.action,
                        rect,
                        &filter.value,
                        &filter.label,
                        *min,
                        *max,
                    ),
                },
            );
        }
        for &id in self.market_rail_view.orders.order().keys() {
            let order = self.market_rail_view.orders.get(id).unwrap();
            ui.view_icon(
                Key::new("market.order.currency", id.value),
                "platinum",
                None,
            );
            ui.view_hit(
                Key::new("market.order.hit", id.value),
                format!("market:whisper:{}", order.index),
                "Copy trade whisper",
                true,
            );
        }
        ui.view_control(SCROLL, |ui, rect| {
            ui.scrollbar(
                "rail:scroll",
                rect,
                self.rail_scroll.maximum(),
                self.rail_offset as f32,
            )
        });
        ui.view_control(PRICE, |ui, rect| {
            ui.number_field("market:price", rect, &m.price, "Platinum", 1, 1_000_000)
        });
        ui.view_control(QUANTITY, |ui, rect| {
            ui.number_field(
                "market:quantity",
                rect,
                &m.quantity,
                "Quantity",
                1,
                1_000_000,
            )
        });
        ui.view_control(POST, |ui, rect| {
            ui.button(
                "market:post",
                rect,
                if m.side == "sell" {
                    "Post sell"
                } else {
                    "Post buy"
                },
                true,
                !self.pending.keys().any(|k| k.starts_with("market:write")) && !m.item().is_null(),
            )
        });
        ui.view_control(WEBSITE, |ui, rect| {
            ui.button(
                "market:website",
                rect,
                "Open on Warframe.market",
                false,
                true,
            )
        });
        for &id in self.market_rail_view.suggestions.order().keys() {
            let item = self.market_rail_view.suggestions.get(id).unwrap();
            ui.view_control(id, |ui, rect| {
                ui.button(
                    format!("market:match:{}", item.index),
                    rect,
                    &item.name,
                    item.selected,
                    true,
                )
            });
        }
    }
}
