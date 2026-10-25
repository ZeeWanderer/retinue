use super::*;
use widgets::{BAD, GOOD, MUTED, TEXT};
mod editor;

const VARIANTS: [(&str, &str, &str); 3] = [
    ("charges", "max_charges", "Charges"),
    ("amberStars", "max_amber_stars", "Amber stars"),
    ("cyanStars", "max_cyan_stars", "Cyan stars"),
];
fn bounded_value(value: &str, min: u32, max: u32) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|v| (*v >= min) && (*v <= max))
        .ok_or_else(|| format!("Enter a whole number from {min} to {max}"))
}

pub struct Market {
    pub account: Value,
    pub matches: Value,
    pub quote: Value,
    pub description: Value,
    pub catalog: HashMap<String, Value>,
    pub dialog: Option<Value>,
    pub scroll: Scroll,
    pub offset: u32,
    search: String,
    query: String,
    item: String,
    side: String,
    email: String,
    password: String,
    price: String,
    price_edited: bool,
    quantity: String,
    rank: String,
    variants: HashMap<String, String>,
    per_trade: String,
    subtype: String,
    category: String,
    order_side: String,
    visibility: String,
    sort: String,
    descending: bool,
    stock: String,
    comparisons: HashMap<String, Value>,
    pub error: Option<String>,
    selected: HashSet<String>,
    filtered: Vec<String>,
    suggestions: bool,
    suggestion: Option<usize>,
    search_at: Option<Instant>,
    pub last_quote: Instant,
}
impl Default for Market {
    fn default() -> Self {
        Self {
            account: Value::Null,
            matches: Value::Null,
            quote: Value::Null,
            description: Value::Null,
            catalog: HashMap::new(),
            dialog: None,
            scroll: Default::default(),
            offset: 0,
            search: String::new(),
            query: String::new(),
            item: String::new(),
            side: "sell".into(),
            email: String::new(),
            password: String::new(),
            price: "1".into(),
            price_edited: false,
            quantity: "1".into(),
            rank: "0".into(),
            variants: HashMap::new(),
            per_trade: "1".into(),
            subtype: String::new(),
            category: "all".into(),
            order_side: "all".into(),
            visibility: "all".into(),
            sort: "name".into(),
            descending: false,
            stock: "all".into(),
            comparisons: HashMap::new(),
            error: None,
            selected: HashSet::new(),
            filtered: Vec::new(),
            suggestions: false,
            suggestion: None,
            search_at: None,
            last_quote: Instant::now(),
        }
    }
}
impl Market {
    pub fn tick(&mut self, now: Instant) -> bool {
        let next = self.scroll.offset(now).round() as u32;
        let changed = next != self.offset;
        self.offset = next;
        changed
    }
    fn item(&self) -> &Value {
        self.catalog.get(&self.item).unwrap_or(&self.description)
    }
    fn suggestions(&self) -> &[Value] {
        entries(&self.matches["resolutions"])
            .first()
            .map(|v| entries(&v["matches"]))
            .unwrap_or(&[])
    }
}
fn item_id(row: &Value) -> &str {
    text(row, "itemId")
}
fn order_name<'a>(order: &'a Value, market: &'a Market) -> &'a str {
    market
        .catalog
        .get(item_id(order))
        .map(|v| text(v, "name"))
        .filter(|s| !s.is_empty())
        .unwrap_or("Loading item...")
}

fn category(item: &Value) -> &str {
    for key in ["group", "category"] {
        if let Some(value) = item[key].as_str() {
            return value;
        }
    }
    let tags = entries(&item["tags"]);
    for (tag, category) in [
        ("relic", "relics"),
        ("arcane", "arcanes"),
        ("mod", "mods"),
        ("set", "sets"),
    ] {
        if tags.iter().any(|t| t == tag) {
            return category;
        }
    }
    let name = text(item, "name").to_lowercase();
    if [
        " blueprint",
        " chassis",
        " neuroptics",
        " systems",
        " barrel",
        " receiver",
        " blade",
        " handle",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
    {
        "parts"
    } else {
        "misc"
    }
}

impl Pages {
    pub(super) fn capture_market(&mut self, view: &str, select: &str) -> Result<(), String> {
        if view == "login" {
            self.market.account = json!({"authenticated":false});
            return Ok(());
        }
        if !matches!(view, "edit-order" | "delete-order") {
            return Err(format!("Unsupported Market capture view: {view}"));
        }
        let order = entries(&self.market.account["orders"])
            .iter()
            .find(|order| {
                select.is_empty()
                    || text(order, "id") == select
                    || order_name(order, &self.market).eq_ignore_ascii_case(select)
            })
            .ok_or_else(|| format!("Market order not found: {select}"))?;
        let action = view.trim_end_matches("-order");
        self.market_signal(Signal {
            key: format!("market:order:{}:{action}", text(order, "id")),
            value: None,
        });
        Ok(())
    }

    fn market_owned(&self, order: &Value) -> Option<f64> {
        if !self.inventory.ready || !self.market.catalog.contains_key(item_id(order)) {
            return None;
        }
        let name = order_name(order, &self.market);
        Some(
            self.inventory
                .items
                .iter()
                .filter(|row| {
                    text(row, "market_name") == name
                        && order["rank"]
                            .as_u64()
                            .is_none_or(|rank| row["rank"].as_u64() == Some(rank))
                })
                .map(|row| number(row, "quantity"))
                .sum(),
        )
    }
    fn market_missing(&self, order: &Value) -> bool {
        order["type"] == "sell"
            && self
                .market_owned(order)
                .is_some_and(|owned| owned < number(order, "quantity"))
    }
    pub(super) fn market_error(&mut self, key: &str, error: String) {
        if let Some(id) = key.strip_prefix("market:comparison:") {
            self.market.comparisons.insert(id.into(), Value::Null);
        }
        self.market.error = Some(error);
    }
    pub(crate) fn open_market(&mut self, item: String, side: &str) {
        if item.is_empty() {
            return;
        }
        self.market.item = item.clone();
        self.market.side = side.into();
        self.market.query = item.replace('_', " ");
        self.market.suggestions = false;
        self.market.quote = Value::Null;
        self.market.description = Value::Null;
        self.market.rank = "0".into();
        self.market.variants.clear();
        self.market.per_trade = "1".into();
        self.market.subtype.clear();
        self.market.error = None;
        self.market.price_edited = false;
        self.rail_scroll.to(0.0, false, Instant::now());
        self.rail_tab = true;
        self.market_reveal = true;
        self.queue(
            "market:describe",
            json!({"op":"market_describe","items":[item]}),
        );
        self.market_quote(false);
        self.queue("market", json!({"op":"market_account"}));
        self.dirty = true;
    }
    pub(super) fn market_reply(&mut self, key: &str, data: Value) {
        if !key.starts_with("market:comparison:") && !key.starts_with("market:catalog:") {
            self.market.error = None;
        }
        match key {
            "market" => {
                self.market.account = data;
                self.market.selected.retain(|id| {
                    entries(&self.market.account["orders"])
                        .iter()
                        .any(|o| text(o, "id") == id)
                });
                let ids = entries(&self.market.account["orders"])
                    .iter()
                    .map(|o| item_id(o).to_owned())
                    .filter(|id| !id.is_empty() && !self.market.catalog.contains_key(id))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                for chunk in ids.chunks(100) {
                    self.queue(
                        &format!("market:catalog:{}", chunk[0]),
                        json!({"op":"market_describe","items":chunk}),
                    );
                }
                if !self.inventory.ready {
                    self.queue("inventory", json!({"op":"inventory_view"}));
                }
            }
            "market:describe" => {
                self.market.description = entries(&data["items"])
                    .first()
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(item) = entries(&data["items"]).first() {
                    self.market
                        .catalog
                        .insert(self.market.item.clone(), item.clone());
                    self.market.query = text(item, "name").into();
                    let subtype = entries(&item["subtypes"])
                        .first()
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if self.market.subtype.is_empty() {
                        self.market.subtype = subtype.into();
                    }
                    self.market_quote(false);
                }
            }
            "market:resolve" => {
                self.market.matches = data;
                self.market.suggestion = None;
            }
            "market:quote" => {
                let quote = entries(&data["quotes"])
                    .first()
                    .map(|q| q["quote"].clone())
                    .unwrap_or_else(|| data["quote"].clone());
                if !self.market.price_edited
                    && let Some(price) = quote[if self.market.side == "sell" {
                        "lowest_sell"
                    } else {
                        "highest_buy"
                    }]
                    .as_i64()
                {
                    self.market.price = price.max(1).to_string();
                }
                self.market.quote = quote;
                self.market.last_quote = Instant::now();
            }
            _ if key.starts_with("market:comparison:") => {
                let quote = entries(&data["quotes"])
                    .first()
                    .map(|q| q["quote"].clone())
                    .unwrap_or_else(|| data["quote"].clone());
                self.market
                    .comparisons
                    .insert(key.trim_start_matches("market:comparison:").into(), quote);
            }
            _ if key.starts_with("market:catalog:") => {
                for item in entries(&data["items"]) {
                    self.market
                        .catalog
                        .insert(text(item, "id").into(), item.clone());
                }
            }
            _ if key.starts_with("market:write") => {
                self.market.dialog = None;
                self.market.password.clear();
                self.ui.forget("market:password");
                self.queue("market", json!({"op":"market_account"}));
            }
            _ => (),
        }
    }
    fn market_quote(&mut self, refresh: bool) {
        if self.market.item.is_empty() {
            return;
        }
        let Ok(filters) = self.market_filters() else {
            return;
        };
        let request = if filters.as_object().is_some_and(|f| !f.is_empty()) {
            json!({"op":"market_quote_variant","item":self.market.item,"filters":filters,"refresh":refresh})
        } else {
            json!({"op":"market_quote","items":[self.market.item],"ttl":60,"refresh":refresh})
        };
        self.queue("market:quote", request);
    }
    fn market_filters(&self) -> Result<Value, String> {
        let mut result = json!({});
        let item = self.market.item();
        if number(item, "max_rank") > 0.0 {
            result["rank"] = json!(bounded_value(
                &self.market.rank,
                0,
                number(item, "max_rank") as u32
            )?);
        }
        for (key, maximum, _) in VARIANTS {
            let max = number(item, maximum) as u32;
            if max > 0 {
                result[key] = json!(bounded_value(
                    self.market
                        .variants
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or("0"),
                    0,
                    max
                )?);
            }
        }
        if !self.market.subtype.is_empty() {
            result["subtype"] = json!(self.market.subtype);
        }
        Ok(result)
    }
    pub(super) fn market_tick(&mut self, now: Instant) {
        if self.market.search_at.is_some_and(|t| now >= t) {
            self.market.search_at = None;
            if self.market.query.trim().len() >= 2 {
                self.queue(
                    "market:resolve",
                    json!({"op":"market_resolve","labels":[self.market.query],"limit":8}),
                );
            }
        }
        if self.rail_tab
            && !self.market.item.is_empty()
            && self.market.last_quote.elapsed() > Duration::from_secs(60)
        {
            self.market.last_quote = now;
            self.market_quote(false);
        }
    }
    pub(super) fn layout_market(&mut self) {
        let w = self.ui.size.0 as f32 / self.ui.scale;
        let h = self.ui.size.1 as f32 / self.ui.scale;
        let authenticated = self.market.account["authenticated"] == true;
        let busy = self.pending.keys().any(|k| k.starts_with("market:write"));
        if !authenticated {
            let x = ((w - 360.0) / 2.0).max(18.0);
            let width = (w - 36.0).min(360.0);
            self.ui.bold(
                self.ui.rect(x, 42.0, width, 35.0),
                "Warframe.market",
                24.0,
                TEXT,
            );
            self.ui.text(
                self.ui.rect(x, 82.0, width, 46.0),
                "Sign in to manage your orders",
                15.0,
                MUTED,
            );
            self.ui.field(
                "market:email",
                self.ui.rect(x, 143.0, width, 36.0),
                &self.market.email,
                "Email",
            );
            self.ui.password(
                "market:password",
                self.ui.rect(x, 190.0, width, 36.0),
                &self.market.password,
            );
            self.ui.button(
                "market:login",
                self.ui.rect(x, 239.0, width, 36.0),
                if busy { "Signing in..." } else { "Sign in" },
                true,
                !busy && !self.market.email.is_empty() && !self.market.password.is_empty(),
            );
            self.ui.button(
                "market:browse",
                self.ui.rect(x, 292.0, width, 34.0),
                "Browse public listings",
                false,
                true,
            );
            if let Some(error) = self.market.error.as_ref().or(self.error.as_ref()) {
                self.ui
                    .text(self.ui.rect(x, 340.0, width, 70.0), error, 14.0, BAD);
            }
            return;
        }
        let profile = &self.market.account["profile"];
        self.ui.text(
            self.ui.rect(14.0, 8.0, w - 375.0, 35.0),
            text(profile, "ingameName"),
            17.0,
            TEXT,
        );
        self.ui.choice(
            "market:presence",
            self.ui.rect(w - 350.0, 10.0, 130.0, 31.0),
            text(&self.market.account["presence"], "mode"),
            &[
                ("auto", "Auto"),
                ("ingame", "In game"),
                ("online", "Online"),
                ("invisible", "Offline"),
            ],
        );
        self.ui.button(
            "market:refresh",
            self.ui.rect(w - 210.0, 10.0, 90.0, 31.0),
            "Refresh",
            false,
            !busy,
        );
        self.ui.button(
            "market:logout",
            self.ui.rect(w - 110.0, 10.0, 95.0, 31.0),
            "Log out",
            false,
            !busy,
        );
        self.ui.bold(
            self.ui.rect(14.0, 52.0, 130.0, 28.0),
            "My orders",
            17.0,
            TEXT,
        );
        self.ui.search_field(
            "market:search",
            self.ui.rect(14.0, 89.0, w - 335.0, 32.0),
            &self.market.search,
            "Filter orders",
            crate::search::Anchor::Leading,
        );
        self.ui.choice(
            "market:side",
            self.ui.rect(w - 311.0, 89.0, 110.0, 32.0),
            &self.market.order_side,
            &[
                ("all", "All orders"),
                ("sell", "Sell orders"),
                ("buy", "Buy orders"),
            ],
        );
        self.ui.choice(
            "market:visibility",
            self.ui.rect(w - 191.0, 89.0, 177.0, 32.0),
            &self.market.visibility,
            &[
                ("all", "Public and hidden"),
                ("public", "Public"),
                ("hidden", "Hidden"),
            ],
        );
        self.ui.choice(
            "market:category",
            self.ui.rect(14.0, 130.0, 120.0, 30.0),
            &self.market.category,
            &[
                ("all", "All items"),
                ("parts", "Parts"),
                ("sets", "Sets"),
                ("mods", "Mods"),
                ("arcanes", "Arcanes"),
                ("relics", "Relics"),
                ("misc", "Misc"),
            ],
        );
        self.ui.choice(
            "market:sort",
            self.ui.rect(144.0, 130.0, 115.0, 30.0),
            &self.market.sort,
            &[
                ("name", "Name"),
                ("price", "Price"),
                ("quantity", "Quantity"),
            ],
        );
        self.ui.arrow(
            self.ui.rect(273.0, 138.0, 14.0, 14.0),
            self.market.descending,
            TEXT,
        );
        self.ui.hit(
            "market:direction",
            self.ui.rect(265.0, 130.0, 30.0, 30.0),
            "Reverse order",
            true,
        );
        self.ui.choice(
            "market:stock",
            self.ui.rect(304.0, 130.0, 135.0, 30.0),
            &self.market.stock,
            &[
                ("all", "Any inventory"),
                ("available", "Available"),
                ("missing", "Missing"),
            ],
        );
        let actions_y = if w < 650.0 { 170.0 } else { 130.0 };
        self.ui.choice(
            "market:bulk",
            self.ui.rect(w - 190.0, actions_y, 176.0, 30.0),
            "actions",
            &[
                ("actions", "Actions"),
                ("public", "Set all public"),
                ("hidden", "Set all hidden"),
                ("delete", "Delete selected..."),
                ("filtered", "Delete filtered..."),
                ("reconcile", "Match inventory..."),
            ],
        );
        let query = self.market.search.to_lowercase();
        let mut orders = entries(&self.market.account["orders"])
            .iter()
            .filter(|o| {
                let description = self.market.catalog.get(item_id(o));
                let name = order_name(o, &self.market);
                (query.is_empty() || name.to_lowercase().contains(&query))
                    && (self.market.stock == "all"
                        || self.market_missing(o) == (self.market.stock == "missing"))
                    && (self.market.order_side == "all"
                        || text(o, "type") == self.market.order_side)
                    && match self.market.visibility.as_str() {
                        "public" => o["visible"] == true,
                        "hidden" => o["visible"] != true,
                        _ => true,
                    }
                    && (self.market.category == "all"
                        || description.is_some_and(|d| category(d) == self.market.category))
            })
            .cloned()
            .collect::<Vec<_>>();
        orders.sort_by(|a, b| {
            let order = match self.market.sort.as_str() {
                "price" => number(a, "platinum").total_cmp(&number(b, "platinum")),
                "quantity" => number(a, "quantity").total_cmp(&number(b, "quantity")),
                _ => order_name(a, &self.market).cmp(order_name(b, &self.market)),
            };
            if self.market.descending {
                order.reverse()
            } else {
                order
            }
        });
        self.market.filtered = orders.iter().map(|o| text(o, "id").into()).collect();
        let start = actions_y + 44.0;
        let viewport = self.ui.rect(10.0, start, w - 20.0, (h - start).max(0.0));
        self.ui.region("page.grid", viewport);
        let grid = retinue_ui::grid::Grid::new(
            viewport.width,
            self.ui.px(370.0),
            self.ui.px(107.0),
            self.ui.px(8.0),
        );
        self.market.scroll.resize(
            grid.content_height(orders.len()) as f32,
            viewport.height as f32,
            Instant::now(),
        );
        self.ui.begin_scroll(viewport, self.market.offset);
        for (visible, index) in grid
            .visible(orders.len(), self.market.offset, viewport.height, 0)
            .enumerate()
        {
            let order = &orders[index];
            let (x, y) = grid.origin(index);
            let s = self.ui.scale;
            let (x, y, cw) = (x as f32 / s, y as f32 / s, grid.width as f32 / s);
            let card = self.ui.rect(x, y, cw, 107.0);
            if visible == 0 {
                self.ui.region("page.card", card);
            }
            let id = text(order, "id");
            self.ui.begin_scope(("order", id));
            let name = order_name(order, &self.market).to_owned();
            let item = self
                .market
                .catalog
                .get(item_id(order))
                .cloned()
                .unwrap_or(Value::Null);
            self.ui.fill(card, 18.0, widgets::CARD);
            self.ui.fill(
                self.ui.rect(x, y, cw, 28.0),
                18.0,
                crate::style::SUMMARY_HEADER,
            );
            self.ui.fill(
                self.ui.rect(x, y + 18.0, cw, 10.0),
                0.0,
                crate::style::SUMMARY_HEADER,
            );
            let icon = if order["visible"] == true {
                "eye"
            } else {
                "eye-off"
            };
            self.ui.bitmap(
                self.ui.rect(x + 9.0, y + 4.0, 20.0, 20.0),
                market_icon(icon),
            );
            self.ui.hit(
                format!("market:order:{id}:visibility"),
                self.ui.rect(x, y, 37.0, 28.0),
                if order["visible"] == true {
                    "Hide order"
                } else {
                    "Make order public"
                },
                !busy,
            );
            let owned = self.market_owned(order);
            let missing = self.market_missing(order);
            let status = owned
                .map(|n| format!("{n:.0} owned"))
                .unwrap_or_else(|| "-- owned".into());
            let sw = self.ui.measure(&status, 13.33, false);
            let trailing = if missing { 37.0 } else { 7.0 };
            self.ui.text(
                self.ui
                    .rect(x + 42.0, y, cw - 42.0 - sw - 12.0 - trailing - 25.0, 28.0),
                &name,
                14.67,
                TEXT,
            );
            self.ui.aligned(
                self.ui.rect(x + cw - trailing - sw, y, sw + 1.0, 28.0),
                status,
                13.33,
                MUTED,
                widgets::Weight::Light,
                0.0,
            );
            if missing {
                let warning = self.ui.rect(x + cw - 37.0, y, 37.0, 28.0);
                self.ui.fill(warning, 18.0, crate::settings::rgb(0x9a3737));
                self.ui.fill(
                    self.ui.rect(x + cw - 37.0, y, 18.0, 28.0),
                    0.0,
                    crate::settings::rgb(0x9a3737),
                );
                self.ui.fill(
                    self.ui.rect(x + cw - 37.0, y + 18.0, 37.0, 10.0),
                    0.0,
                    crate::settings::rgb(0x9a3737),
                );
                self.ui
                    .aligned(warning, "!", 16.0, TEXT, widgets::Weight::Regular, 0.5);
                self.ui.hit(
                    format!("market:order:{id}:warning"),
                    warning,
                    "Order quantity exceeds owned quantity",
                    false,
                );
            }
            self.ui.toggle(
                format!("market:order:{id}:select"),
                self.ui
                    .rect(x + cw - trailing - sw - 30.0, y + 1.0, 24.0, 26.0),
                "",
                self.market.selected.contains(id),
            );
            self.ui.fill(
                self.ui.rect(x, y + 28.0, 70.0, 79.0),
                18.0,
                crate::style::INSET_PANEL,
            );
            self.ui.fill(
                self.ui.rect(x, y + 28.0, 70.0, 18.0),
                0.0,
                crate::style::INSET_PANEL,
            );
            self.ui.fill(
                self.ui.rect(x + 52.0, y + 28.0, 18.0, 79.0),
                0.0,
                crate::style::INSET_PANEL,
            );
            self.ui.image(
                self.ui.rect(x + 13.0, y + 32.0, 44.0, 44.0),
                asset(&item["asset"]).as_ref(),
            );
            self.ui.fill(
                self.ui.rect(x + 12.0, y + 83.0, 46.0, 20.0),
                8.0,
                [0, 0, 0, 190],
            );
            self.ui.aligned(
                self.ui.rect(x + 12.0, y + 83.0, 46.0, 20.0),
                if text(order, "type") == "sell" {
                    "WTS"
                } else {
                    "WTB"
                },
                13.33,
                TEXT,
                widgets::Weight::Regular,
                0.5,
            );
            let column = (cw - 84.0) / 3.0;
            for (i, value, icon) in [
                (0, format!("{:.0}", number(order, "quantity")), "boxes"),
                (1, format!("{:.0}", number(order, "platinum")), "platinum"),
            ] {
                let tw = self.ui.measure(&value, 14.67, false);
                let left = x + 75.0 + i as f32 * column + (column - tw - 25.0) / 2.0;
                self.ui.text(
                    self.ui.rect(left, y + 31.0, tw + 1.0, 33.0),
                    value,
                    14.67,
                    TEXT,
                );
                let r = self.ui.rect(left + tw + 5.0, y + 38.0, 20.0, 20.0);
                if icon == "platinum" {
                    self.ui.icon(r, icon);
                } else {
                    self.ui.bitmap(r, market_icon(icon));
                }
            }
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
            let extra_bounds = self
                .ui
                .rect(x + 75.0 + 2.0 * column, y + 31.0, column, 33.0);
            let extra = extra.join(" / ");
            self.ui.hit(
                format!("market:order:{id}:details"),
                extra_bounds,
                &extra,
                false,
            );
            self.ui.aligned(
                extra_bounds,
                extra,
                14.0,
                TEXT,
                widgets::Weight::Regular,
                0.5,
            );
            let comparison = self.market.comparisons.get(id);
            let value = comparison.and_then(|q| {
                q[if order["type"] == "sell" {
                    "lowest_sell"
                } else {
                    "highest_buy"
                }]
                .as_f64()
            });
            self.ui.text(
                self.ui
                    .rect(x + 105.0, y + 70.0, (cw - 240.0).max(1.0), 30.0),
                format!(
                    "{}: {}",
                    if order["type"] == "sell" {
                        "Lowest WTS"
                    } else {
                        "Highest WTB"
                    },
                    listing::price(value, comparison.is_some())
                ),
                13.33,
                TEXT,
            );
            if comparison.is_none() && !item.is_null() {
                let filters = order
                    .as_object()
                    .map(|o| {
                        o.iter()
                            .filter(|(key, _)| {
                                matches!(
                                    key.as_str(),
                                    "rank" | "charges" | "amberStars" | "cyanStars" | "subtype"
                                )
                            })
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect::<serde_json::Map<_, _>>()
                    })
                    .unwrap_or_default();
                let op = if filters.is_empty() {
                    json!({"op":"market_quote","items":[item_id(order)],"ttl":60})
                } else {
                    json!({"op":"market_quote_variant","item":item_id(order),"filters":filters})
                };
                self.queue(&format!("market:comparison:{id}"), op);
            }
            for (i, (action, icon, label)) in [
                ("list", "search", "View listings"),
                ("edit", "pencil", "Edit order"),
                ("add", "plus", "Add one"),
                ("close", "check", "Complete one"),
                ("delete", "trash", "Delete order"),
            ]
            .into_iter()
            .enumerate()
            {
                let cx = if i == 0 {
                    x + 76.0
                } else {
                    x + cw - 124.0 + (i - 1) as f32 * 29.0
                };
                self.ui.outline(
                    self.ui.rect(cx - 3.0, y + 71.0, 28.0, 28.0),
                    6.0,
                    [255, 255, 255, 184],
                );
                self.ui
                    .bitmap(self.ui.rect(cx, y + 74.0, 21.0, 21.0), market_icon(icon));
                self.ui.hit(
                    format!("market:order:{id}:{action}"),
                    self.ui.rect(cx - 3.0, y + 71.0, 28.0, 28.0),
                    label,
                    !busy,
                );
            }
            self.ui.end_scope();
        }
        self.ui.end_scroll();
        self.ui.scrollbar(
            "market:scroll",
            viewport,
            self.market.scroll.maximum(),
            self.market.offset as f32,
        );
        if orders.is_empty() {
            self.ui.text(
                self.ui.rect(24.0, 210.0, w - 48.0, 35.0),
                "No matching orders",
                17.0,
                MUTED,
            );
        }
        if let Some(error) = self.market.error.as_ref().or(self.error.as_ref()) {
            self.ui.text(
                self.ui.rect(14.0, h - 30.0, w - 28.0, 26.0),
                error,
                13.0,
                BAD,
            );
        }
        self.market_dialog();
    }
    pub(super) fn layout_market_rail(&mut self) {
        let w = self.rail.size.0 as f32 / self.rail.scale;
        let h = self.rail.size.1 as f32 / self.rail.scale;
        self.rail.search_field(
            "market:query",
            self.rail.rect(10.0, 45.0, w - 55.0, 34.0),
            &self.market.query,
            "Search Warframe.market",
            crate::search::Anchor::Leading,
        );
        self.rail
            .icon(self.rail.rect(w - 36.0, 52.0, 20.0, 20.0), "search");
        self.rail.hit(
            "market:query.submit",
            self.rail.rect(w - 43.0, 45.0, 34.0, 34.0),
            "Search",
            true,
        );
        if self.market.item.is_empty() {
            self.rail.text(
                self.rail.rect(18.0, 102.0, w - 36.0, 50.0),
                "Search an item to view current listings",
                14.0,
                MUTED,
            );
        } else {
            let item = self.market.item().clone();
            self.rail.image(
                self.rail.rect(13.0, 91.0, 64.0, 64.0),
                asset(&item["asset"]).as_ref(),
            );
            self.rail.bold(
                self.rail.rect(88.0, 90.0, w - 100.0, 29.0),
                if text(&item, "name").is_empty() {
                    &self.market.query
                } else {
                    text(&item, "name")
                },
                16.0,
                TEXT,
            );
            for (i, (id, label)) in [("sell", "WTS"), ("buy", "WTB")].into_iter().enumerate() {
                self.rail.button(
                    format!("market:mode:{id}"),
                    self.rail.rect(88.0 + i as f32 * 76.0, 126.0, 68.0, 29.0),
                    label,
                    self.market.side == id,
                    true,
                );
            }
            self.rail
                .icon(self.rail.rect(w - 36.0, 130.0, 20.0, 20.0), "refresh");
            self.rail.hit(
                "market:quote-refresh",
                self.rail.rect(w - 42.0, 124.0, 32.0, 32.0),
                "Refresh listings",
                true,
            );
            let mut top = 168.0;
            if number(&item, "max_rank") > 0.0 {
                self.rail
                    .text(self.rail.rect(12.0, top, 60.0, 30.0), "Rank", 13.0, MUTED);
                let options = (0..=number(&item, "max_rank") as u32)
                    .map(|i| (i.to_string(), i.to_string()))
                    .collect::<Vec<_>>();
                let opts = options
                    .iter()
                    .map(|(a, b)| (a.as_str(), b.as_str()))
                    .collect::<Vec<_>>();
                self.rail.choice(
                    "market:rank",
                    self.rail.rect(73.0, top, 66.0, 30.0),
                    &self.market.rank,
                    &opts,
                );
                top += 37.0;
            }
            if !entries(&item["subtypes"]).is_empty() {
                let options = entries(&item["subtypes"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| (s, s))
                    .collect::<Vec<_>>();
                self.rail.choice(
                    "market:subtype",
                    self.rail.rect(12.0, top, 150.0, 30.0),
                    &self.market.subtype,
                    &options,
                );
                top += 37.0;
            }
            for (key, maximum, label) in VARIANTS {
                let max = number(&item, maximum) as u32;
                if max == 0 {
                    continue;
                }
                self.rail
                    .text(self.rail.rect(12.0, top, 100.0, 30.0), label, 13.0, MUTED);
                self.rail.number_field(
                    &format!("market:variant:{key}"),
                    self.rail.rect(122.0, top, 96.0, 30.0),
                    self.market
                        .variants
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or("0"),
                    label,
                    0,
                    max,
                );
                top += 37.0;
            }
            if item["bulk_tradable"] == true {
                self.rail.text(
                    self.rail.rect(12.0, top, 100.0, 30.0),
                    "Per trade",
                    13.0,
                    MUTED,
                );
                self.rail.number_field(
                    "market:per-trade",
                    self.rail.rect(122.0, top, 96.0, 30.0),
                    &self.market.per_trade,
                    "Per trade",
                    1,
                    6,
                );
                top += 37.0;
            }
            if let Some(error) = &self.market.error {
                self.rail
                    .text(self.rail.rect(12.0, top, w - 24.0, 42.0), error, 12.0, BAD);
                top += 46.0;
            }
            let authenticated = self.market.account["authenticated"] == true;
            let bottom = if authenticated { 112.0 } else { 44.0 };
            self.rail.text(
                self.rail.rect(12.0, top, w - 24.0, 25.0),
                "Player / Status",
                12.0,
                MUTED,
            );
            self.rail.text(
                self.rail.rect(w - 135.0, top, 46.0, 25.0),
                "Qty",
                12.0,
                MUTED,
            );
            self.rail.text(
                self.rail.rect(w - 79.0, top, 65.0, 25.0),
                "Price",
                12.0,
                MUTED,
            );
            top += 29.0;
            let viewport = self
                .rail
                .rect(7.0, top, w - 14.0, (h - top - bottom).max(0.0));
            let orders = entries(
                &self.market.quote[if self.market.side == "sell" {
                    "sell_orders"
                } else {
                    "buy_orders"
                }],
            )
            .to_vec();
            self.rail_scroll.resize(
                self.rail.px(orders.len() as f32 * 44.0) as f32,
                viewport.height as f32,
                Instant::now(),
            );
            self.rail.begin_scroll(viewport, self.rail_offset);
            for (i, order) in orders.iter().enumerate() {
                let y = i as f32 * 44.0;
                if self.rail.px(y + 44.0) < self.rail_offset
                    || self.rail.px(y) > self.rail_offset + viewport.height
                {
                    continue;
                }
                let user = &order["user"];
                self.rail.begin_scope((
                    "market-listing",
                    text(order, "id"),
                    text(user, "ingame_name"),
                ));
                let name = text(user, "ingame_name");
                let name = if name.is_empty() {
                    text(user, "ingameName")
                } else {
                    name
                };
                self.rail
                    .fill(self.rail.rect(0.0, y, w - 14.0, 41.0), 5.0, widgets::CARD);
                self.rail.text(
                    self.rail.rect(8.0, y + 1.0, w - 150.0, 22.0),
                    name,
                    13.0,
                    TEXT,
                );
                self.rail.text(
                    self.rail.rect(8.0, y + 21.0, w - 150.0, 18.0),
                    text(user, "status"),
                    11.0,
                    GOOD,
                );
                self.rail.text(
                    self.rail.rect(w - 142.0, y, 44.0, 41.0),
                    format!("{:.0}", number(order, "quantity")),
                    13.0,
                    TEXT,
                );
                self.rail.bold(
                    self.rail.rect(w - 86.0, y, 48.0, 41.0),
                    format!("{:.0}", number(order, "platinum")),
                    14.0,
                    TEXT,
                );
                self.rail
                    .icon(self.rail.rect(w - 42.0, y + 13.0, 16.0, 16.0), "platinum");
                self.rail.hit(
                    format!("market:whisper:{i}"),
                    self.rail.rect(0.0, y, w - 14.0, 41.0),
                    "Copy trade whisper",
                    true,
                );
                self.rail.end_scope();
            }
            self.rail.end_scroll();
            self.rail.scrollbar(
                "rail:scroll",
                viewport,
                self.rail_scroll.maximum(),
                self.rail_offset as f32,
            );
            if orders.is_empty() {
                self.rail.text(
                    self.rail.rect(16.0, top + 12.0, w - 32.0, 35.0),
                    if self.pending.contains_key("market:quote") {
                        "Loading listings..."
                    } else {
                        "No online listings"
                    },
                    14.0,
                    MUTED,
                );
            }
            if authenticated {
                self.rail.text(
                    self.rail.rect(12.0, h - 110.0, 80.0, 24.0),
                    "Platinum",
                    12.0,
                    MUTED,
                );
                self.rail.number_field(
                    "market:price",
                    self.rail.rect(12.0, h - 83.0, 90.0, 30.0),
                    &self.market.price,
                    "Platinum",
                    1,
                    1_000_000,
                );
                self.rail.text(
                    self.rail.rect(116.0, h - 110.0, 80.0, 24.0),
                    "Quantity",
                    12.0,
                    MUTED,
                );
                self.rail.number_field(
                    "market:quantity",
                    self.rail.rect(116.0, h - 83.0, 90.0, 30.0),
                    &self.market.quantity,
                    "Quantity",
                    1,
                    1_000_000,
                );
                self.rail.button(
                    "market:post",
                    self.rail.rect(220.0, h - 83.0, w - 232.0, 30.0),
                    if self.market.side == "sell" {
                        "Post sell"
                    } else {
                        "Post buy"
                    },
                    true,
                    !self.pending.keys().any(|k| k.starts_with("market:write")) && !item.is_null(),
                );
            }
            self.rail.button(
                "market:website",
                self.rail.rect(12.0, h - 40.0, w - 24.0, 30.0),
                "Open on Warframe.market",
                false,
                true,
            );
        }
        if self.market.suggestions {
            let matches = self.market.suggestions().to_vec();
            for (i, item) in matches.iter().enumerate() {
                self.rail.button(
                    format!("market:match:{i}"),
                    self.rail.rect(10.0, 81.0 + i as f32 * 30.0, w - 20.0, 30.0),
                    text(item, "name"),
                    self.market.suggestion == Some(i),
                    true,
                );
            }
        }
    }
    fn market_dialog(&mut self) {
        let Some(dialog) = self.market.dialog.clone() else {
            return;
        };
        self.ui.begin_modal();
        let w = self.ui.size.0 as f32 / self.ui.scale;
        let h = self.ui.size.1 as f32 / self.ui.scale;
        let item = self
            .market
            .catalog
            .get(item_id(&dialog["order"]))
            .cloned()
            .unwrap_or(Value::Null);
        let height = if dialog["edit"] == true {
            editor::height(&dialog, &item) + 28.0
        } else {
            210.0
        };
        let x = (w - 330.0) / 2.0;
        let y = ((h - height) / 2.0).max(0.0);
        self.ui
            .fill(self.ui.rect(0.0, 0.0, w, h), 0.0, [0, 0, 0, 150]);
        self.ui
            .fill(self.ui.rect(x, y, 330.0, height), 8.0, widgets::POPOVER);
        self.ui
            .region("market.dialog", self.ui.rect(x, y, 330.0, height));
        self.ui.bold(
            self.ui.rect(x + 16.0, y + 12.0, 298.0, 30.0),
            text(&dialog, "title"),
            18.0,
            TEXT,
        );
        if dialog["edit"] == true {
            self.ui.text(
                self.ui.rect(x + 16.0, y + 44.0, 298.0, 26.0),
                order_name(&dialog["order"], &self.market),
                14.0,
                TEXT,
            );
            editor::render(&mut self.ui, &dialog, &item, x, y + 28.0);
        } else {
            self.ui.text(
                self.ui.rect(x + 16.0, y + 56.0, 298.0, 60.0),
                text(&dialog, "message"),
                14.0,
                MUTED,
            );
        }
        self.ui.button(
            "market:cancel",
            self.ui.rect(x + 16.0, y + height - 54.0, 138.0, 35.0),
            "Cancel",
            false,
            true,
        );
        let enabled = !self.pending.keys().any(|k| k.starts_with("market:write"));
        let rect = self.ui.rect(x + 174.0, y + height - 54.0, 138.0, 35.0);
        if dialog["edit"] == true {
            self.ui
                .button("market:confirm", rect, "Save", true, enabled);
        } else {
            self.ui
                .destructive("market:confirm", rect, "Confirm", enabled);
        }
        if let Some(error) = &self.market.error {
            self.ui.text(
                self.ui.rect(x + 16.0, y + height - 85.0, 298.0, 28.0),
                error,
                12.0,
                BAD,
            );
        }
    }
    pub(super) fn market_search_key(
        &mut self,
        key: retinue_ui::desktop::winit::keyboard::Key<&str>,
    ) -> bool {
        use retinue_ui::desktop::winit::keyboard::{Key, NamedKey};
        if !self.market.suggestions || self.rail.focus.as_deref() != Some("market:query") {
            return false;
        }
        let count = self.market.suggestions().len();
        match key {
            Key::Named(NamedKey::ArrowDown) => {
                self.market.suggestion = (count > 0)
                    .then(|| self.market.suggestion.map_or(0, |i| (i + 1).min(count - 1)))
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.market.suggestion = (count > 0).then(|| {
                    self.market
                        .suggestion
                        .map_or(count - 1, |i| i.saturating_sub(1))
                })
            }
            Key::Named(NamedKey::Enter) => self.market_signal(Signal {
                key: format!("market:match:{}", self.market.suggestion.unwrap_or(0)),
                value: None,
            }),
            Key::Named(NamedKey::Escape) => self.market.suggestions = false,
            _ => return false,
        }
        self.dirty = true;
        self.layout();
        true
    }
    pub(super) fn market_signal(&mut self, signal: Signal) {
        let value = signal.value.unwrap_or_default();
        let key = signal.key.as_str();
        let writing = matches!(
            key,
            "market:login"
                | "market:password.submit"
                | "market:logout"
                | "market:presence"
                | "market:post"
                | "market:confirm"
                | "market:bulk"
        ) || key.starts_with("market:order:")
            && !key.ends_with(":select")
            && !key.ends_with(":list");
        if writing
            && self
                .pending
                .keys()
                .any(|k| k.starts_with("market:write") || k == "market")
        {
            return;
        }
        match key{
            "market:scroll"=>{if let Ok(y)=value.parse(){self.market.scroll.to(y,false,Instant::now());self.market.offset=y.max(0.0) as u32;}},
            "market:email"=>self.market.email=value,
            "market:password"=>self.market.password=value,
            "market:login"|"market:password.submit"=>self.queue("market:write:login",json!({"op":"market_login","email":self.market.email,"password":self.market.password})),
            "market:logout"=>self.queue("market:write:logout",json!({"op":"market_logout"})),
            "market:presence"=>self.queue("market:write:presence",json!({"op":"market_presence_set","mode":value})),
            "market:refresh"=>{self.market.comparisons.clear();self.queue("market",json!({"op":"market_account"}));},
            "market:browse"=>self.rail_tab=true,
            "market:search"=>self.market.search=value,
            "market:side"=>self.market.order_side=value,
            "market:visibility"=>self.market.visibility=value,
            "market:category"=>self.market.category=value,
            "market:sort"=>self.market.sort=value,
            "market:stock"=>self.market.stock=value,
            "market:direction"=>self.market.descending=!self.market.descending,
            "market:query"=>{self.market.query=value;self.market.suggestions=true;self.market.search_at=Some(Instant::now()+Duration::from_millis(180));},
            "market:query.submit"=>{if let Some(item)=self.market.suggestions().get(self.market.suggestion.unwrap_or(0)){self.open_market(text(item,"slug").into(),"sell");}},
            "market:rank"=>{self.market.rank=value;self.market_quote(false);},
            "market:subtype"=>{self.market.subtype=value;self.market_quote(false);},
            "market:price"=>{self.market.price=value;self.market.price_edited=true;},
            "market:quantity"=>self.market.quantity=value,
            "market:per-trade"=>self.market.per_trade=value,
            "market:quote-refresh"=>self.market_quote(true),
            "market:website"=>{let slug=text(self.market.item(),"slug");if !slug.is_empty(){self.external=Some(format!("https://warframe.market/items/{slug}"));}},
            "market:post"=>{
                let Some((price,quantity))=positive_values(&self.market.price,&self.market.quantity)else{self.error=Some("Enter a positive whole price and quantity".into());return;};
                if self.market.account["authenticated"]!=true||text(self.market.item(),"id").is_empty(){return;}
                let mut order=match self.market_filters(){Ok(value)=>value,Err(error)=>{self.error=Some(error);return;}};
                order["itemId"]=json!(text(self.market.item(),"id"));order["type"]=json!(self.market.side);order["platinum"]=json!(price);order["quantity"]=json!(quantity);order["visible"]=json!(true);
                if self.market.item()["bulk_tradable"]==true{
                    let per=match bounded_value(&self.market.per_trade,1,6){Ok(v)=>v,Err(e)=>{self.error=Some(e);return;}};
                    order["perTrade"]=json!(per);order["quantity"]=json!((quantity/per*per).max(per));
                }
                self.queue("market:write:create",json!({"op":"market_order_create","order":order}));
            }
            "market:bulk"=>match value.as_str(){
                "public"|"hidden"=>self.queue("market:write:visibility",json!({"op":"market_orders_visibility","visible":value=="public"})),
                "delete"|"filtered"=>{
                    let ids=if value=="delete"{self.market.selected.iter().cloned().collect::<Vec<_>>()}else{self.market.filtered.clone()};
                    if !ids.is_empty(){self.market.dialog=Some(json!({"title":"Delete orders?","message":format!("{} orders will be removed.",ids.len()),"operations":ids.into_iter().map(|id|json!({"op":"market_order_delete","order_id":id})).collect::<Vec<_>>()}));}
                },
                "reconcile"=>self.reconcile_market(),
                _=>()
            },
            "market:cancel"=>self.market.dialog=None,
            "market:confirm"=>{
                if let Some(dialog)=self.market.dialog.clone(){
                    if let Some(operations)=dialog["operations"].as_array(){for (i,op) in operations.iter().enumerate(){self.queue(&format!("market:write:batch:{i}"),op.clone());}}
                    else if dialog["edit"]==true{
                        let item=self.market.catalog.get(item_id(&dialog["order"])).unwrap_or(&Value::Null);
                        match editor::patch(&dialog,item){
                            Ok(patch)=>self.queue("market:write:edit",json!({"op":"market_order_update","order_id":dialog["id"],"patch":patch})),
                            Err(error)=>self.market.error=Some(error),
                        }
                    }else{self.queue("market:write:delete",json!({"op":"market_order_delete","order_id":dialog["id"]}));}
                }
            }
            _ if key.starts_with("market:edit:")=>{
                if let Some(dialog)=&mut self.market.dialog {
                    let field=key.trim_start_matches("market:edit:");
                    dialog["draft"][field]=if field=="visible"{json!(dialog["draft"][field]!=true)}else{json!(value)};
                }
            }
            _ if key.starts_with("market:variant:")=>{self.market.variants.insert(key.trim_start_matches("market:variant:").into(),value);self.market_quote(false);},
            _ if key.starts_with("market:match:")=>{
                if let Some(i)=key.rsplit(':').next().and_then(|v|v.parse::<usize>().ok())&&let Some(item)=self.market.suggestions().get(i){self.open_market(text(item,"slug").into(),"sell");}
            }
            _ if key.starts_with("market:mode:")=>{self.market.side=key.rsplit(':').next().unwrap_or("sell").into();self.rail_scroll.to(0.0,false,Instant::now());}
            _ if key.starts_with("market:whisper:")=>{
                if let Some(i)=key.rsplit(':').next().and_then(|v|v.parse::<usize>().ok())&&let Some(order)=entries(&self.market.quote[if self.market.side=="sell"{"sell_orders"}else{"buy_orders"}]).get(i){
                    if let Some(whisper)=whisper(order,text(self.market.item(),"name"),&self.market.side){
                        self.rail.clipboard=Some(("market:whisper".into(),Instant::now(),retinue_ui::desktop::clipboard::Operation::Write(whisper)));
                    }else{self.market.error=Some("Listing has no player name".into());}
                }
            }
            _ if key.starts_with("market:order:")=>{
                let Some((id,action))=key.trim_start_matches("market:order:").rsplit_once(':')else{return};
                let Some(order)=entries(&self.market.account["orders"]).iter().find(|o|text(o,"id")==id).cloned()else{return};
                match action{
                    "select"=>{if !self.market.selected.remove(id){self.market.selected.insert(id.into());}},
                    "visibility"=>self.queue("market:write:visibility",json!({"op":"market_order_update","order_id":id,"patch":{"visible":order["visible"]!=true}})),
                    "add"=>self.queue("market:write:quantity",json!({"op":"market_order_update","order_id":id,"patch":{"quantity":number(&order,"quantity") as u64+number(&order,"perTrade").max(1.0) as u64}})),
                    "close"=>self.queue("market:write:close",json!({"op":"market_order_close","order_id":id,"quantity":number(&order,"perTrade").max(1.0) as u64})),
                    "list"=>{
                        self.open_market(item_id(&order).into(),"sell");
                        self.market.rank=number(&order,"rank").to_string();
                        self.market.subtype=text(&order,"subtype").into();
                        self.market.per_trade=number(&order,"perTrade").max(1.0).to_string();
                        for (key,_,_) in VARIANTS {if let Some(value)=order.get(key){self.market.variants.insert(key.into(),value.to_string());}}
                        self.market_quote(false);
                    },
                    "edit"=>{self.market.error=None;self.market.dialog=Some(editor::draft(&order));},
                    "delete"=>self.market.dialog=Some(json!({"id":id,"title":"Delete order?","message":order_name(&order,&self.market)})),_=>()
                }
            }
            _=>(),
        }
    }
    fn reconcile_market(&mut self) {
        let mut operations = vec![];
        for order in entries(&self.market.account["orders"])
            .iter()
            .filter(|o| o["type"] == "sell")
        {
            let Some(owned) = self.market_owned(order) else {
                self.market.error =
                    Some("Wait for inventory and item details before reconciling orders".into());
                return;
            };
            // Non-rank variants are not distinguished in the inventory projection.
            if ["charges", "amberStars", "cyanStars", "subtype"]
                .iter()
                .any(|key| order.get(key).is_some_and(|v| !v.is_null()))
            {
                self.market.error =
                    Some("Variant quantities cannot be reconciled automatically".into());
                return;
            }
            let per = number(order, "perTrade").max(1.0) as u64;
            let target = owned as u64 / per * per;
            if target < number(order, "quantity") as u64 {
                operations.push(if target<per {json!({"op":"market_order_delete","order_id":order["id"]})}
                    else{json!({"op":"market_order_update","order_id":order["id"],"patch":{"quantity":target}})});
            }
        }
        if operations.is_empty() {
            self.market.error = Some("Sell quantities already match inventory".into());
            return;
        }
        self.market.dialog = Some(
            json!({"title":"Reconcile sell orders?","message":format!("Adjust or remove {} orders to match inventory.",operations.len()),"operations":operations}),
        );
    }
}
fn positive_values(price: &str, quantity: &str) -> Option<(u32, u32)> {
    let p = price.parse().ok()?;
    let q = quantity.parse().ok()?;
    (p > 0 && q > 0).then_some((p, q))
}
fn whisper(order: &Value, item: &str, side: &str) -> Option<String> {
    let user = &order["user"];
    let name = if text(user, "ingameName").is_empty() {
        text(user, "ingame_name")
    } else {
        text(user, "ingameName")
    };
    if name.is_empty() {
        return None;
    }
    let mut details = vec![];
    if !text(order, "subtype").is_empty() {
        details.push(text(order, "subtype").to_owned());
    }
    for (key, label) in [
        ("rank", "Rank"),
        ("charges", "Charges"),
        ("amberStars", "Amber stars"),
        ("cyanStars", "Cyan stars"),
    ] {
        if let Some(value) = order[key].as_u64() {
            details.push(format!("{label} {value}"));
        }
    }
    let details = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    let per = number(order, "perTrade").max(1.0) as u64;
    let pack = if per > 1 {
        format!(" x{per}")
    } else {
        String::new()
    };
    let action = if side == "sell" { "buy" } else { "sell" };
    Some(format!(
        "/w {name} Hi! I want to {action}:{pack} \"{item}{details}\" for {:.0} platinum. (warframe.market)",
        number(order, "platinum")
    ))
}
fn market_icon(name: &str) -> &'static [u8] {
    macro_rules! icon {
        ($n:literal) => {
            include_bytes!(concat!("../../resources/market/wfgui-", $n, ".png"))
        };
    }
    match name {
        "eye" => icon!("eye"),
        "boxes" => icon!("boxes"),
        "eye-off" => icon!("eye-off"),
        "search" => icon!("search"),
        "pencil" => icon!("pencil"),
        "plus" => icon!("plus"),
        "check" => icon!("check"),
        _ => icon!("trash"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_inputs_never_silently_become_zero() {
        assert_eq!(positive_values("0", "2"), None);
        assert_eq!(positive_values("4.5", "2"), None);
        assert_eq!(positive_values("5", "2"), Some((5, 2)));
    }
    #[test]
    fn whispers_identify_variant_and_pack() {
        let order =
            json!({"user":{"ingameName":"Trader"},"rank":5,"charges":2,"perTrade":3,"platinum":24});
        assert_eq!(
            whisper(&order, "Mod", "sell").unwrap(),
            "/w Trader Hi! I want to buy: x3 \"Mod (Rank 5, Charges 2)\" for 24 platinum. (warframe.market)"
        );
        assert!(whisper(&Value::Null, "Mod", "buy").is_none());
    }
    #[test]
    fn listing_navigation_preserves_rank_and_quotes_do_not_overwrite_edited_price() {
        let mut pages = Pages::new(Default::default()).unwrap();
        pages.fixture();
        pages.fixture = false;
        pages.market.account["orders"][0]["rank"] = json!(5);
        pages.market_signal(Signal {
            key: "market:order:example:list".into(),
            value: None,
        });
        let description = json!({"items":[{"id":"example-item","name":"Mod","max_rank":10}]});
        pages.market_reply("market:describe", description);
        assert_eq!(pages.market_filters().unwrap()["rank"], 5);
        pages.market_signal(Signal {
            key: "market:price".into(),
            value: Some("22".into()),
        });
        pages.market_reply("market:quote", json!({"quote":{"lowest_sell":5}}));
        assert_eq!(pages.market.price, "22");
    }
}
