mod cards;

use super::*;
use retinue_ui::composition::{Key as ElementKey, SelectionMotion};
use retinue_ui::grid::Grid;
use std::cmp::Ordering;
use widgets::{ACCENT, BAD, MUTED, TEXT};

fn categories(page: Page) -> &'static [(&'static str, &'static str, &'static str)] {
    match page {
        Page::Inventory => &[
            ("parts", "Parts", "parts"),
            ("relics", "Relics", "relics"),
            ("mods", "Mods", "mods"),
            ("arcanes", "Arcanes", "arcanes"),
            ("misc", "Misc", "misc"),
            ("sets", "Sets", "sets"),
        ],
        Page::Mastery => &[
            ("all", "All", "parts"),
            ("warframes", "Warframes", "warframe"),
            ("weapons", "Weapons", "primary"),
            ("companions", "Companions", "companion"),
        ],
        _ => &[
            ("all", "All", "parts"),
            ("lith", "Lith", "lith"),
            ("meso", "Meso", "meso"),
            ("neo", "Neo", "neo"),
            ("axi", "Axi", "axi"),
            ("requiem", "Requiem", "requiem"),
            ("vanguard", "Vanguard", "relics"),
        ],
    }
}

fn category_key(index: usize) -> ElementKey {
    ElementKey::new("listing.category", index as u64)
}

fn summary_positions(available: f32, widths: &[f32]) -> Vec<(f32, f32)> {
    let mut result = Vec::with_capacity(widths.len());
    let mut first = 0;
    let mut y = 0.0;
    while first < widths.len() {
        let mut end = first + 1;
        let mut total = widths[first];
        while end < widths.len() && total + widths[end] + (end - first) as f32 * 10.0 <= available {
            total += widths[end];
            end += 1;
        }
        let gap = ((available - total) / (end - first) as f32).max(0.0);
        let mut x = gap / 2.0;
        for width in &widths[first..end] {
            result.push((x, y));
            x += width + gap;
        }
        first = end;
        y += 163.0;
    }
    result
}

#[derive(Clone, Default)]
pub struct Quote {
    pub sell: Option<f64>,
    pub buy: Option<f64>,
    pub resolved: bool,
}

pub struct Listing {
    pub page: Page,
    pub data: Value,
    pub items: Vec<Value>,
    pub visible: Vec<usize>,
    pub ready: bool,
    pub error: Option<String>,
    pub category: String,
    pub search: String,
    pub mode: String,
    pub sort: String,
    pub descending: bool,
    pub owned: bool,
    pub filters: bool,
    pub prime: String,
    pub mastered: String,
    pub vaulted: String,
    pub duplicates: String,
    pub complete: String,
    pub scroll: Scroll,
    pub offset: u32,
    pub viewport: Rect,
    pub grid: Grid,
    pub menu: Option<(usize, Option<usize>)>,
    pub(super) menu_anchor: (f64, f64),
    category_motion: SelectionMotion,
    category_sampled_at: Instant,
}

impl Listing {
    pub fn new(page: Page) -> Self {
        Self {
            page,
            data: Value::Null,
            items: vec![],
            visible: vec![],
            ready: false,
            error: None,
            category: if page == Page::Inventory {
                "parts"
            } else {
                "all"
            }
            .into(),
            search: String::new(),
            mode: "easy".into(),
            sort: "name".into(),
            descending: false,
            owned: true,
            filters: false,
            prime: "all".into(),
            mastered: "all".into(),
            vaulted: "all".into(),
            duplicates: "all".into(),
            complete: "all".into(),
            scroll: Default::default(),
            offset: 0,
            viewport: Rect::default(),
            grid: Grid::new(600, 300, 130, 8),
            menu: None,
            menu_anchor: (0.0, 0.0),
            category_motion: SelectionMotion::new(category_key(0)),
            category_sampled_at: Instant::now(),
        }
    }
    pub fn replace(&mut self, data: Value, quotes: &HashMap<String, Quote>) {
        let anchor = self.anchor();
        self.menu = None;
        self.items = entries(&data["items"]).to_vec();
        self.data = data;
        self.ready = true;
        self.error = None;
        self.reorder_anchored(quotes, false, anchor);
    }
    pub fn wheel(&mut self, delta: f32, smooth: bool) {
        self.scroll.by(delta, smooth, Instant::now());
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let offset = self.scroll.offset(now).round() as u32;
        let changed = offset != self.offset
            || self
                .category_motion
                .deadline(self.category_sampled_at)
                .is_some();
        self.offset = offset;
        self.category_sampled_at = now;
        changed
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        [
            self.scroll.deadline(now),
            self.category_motion.deadline(now),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    fn sync_category(&mut self, animate: bool, now: Instant) {
        let index = categories(self.page)
            .iter()
            .position(|(id, _, _)| *id == self.category)
            .unwrap_or(0);
        if self.category_motion.selected() != category_key(index) {
            self.category_motion
                .select(category_key(index), now, !animate);
            self.category_sampled_at = now;
        }
    }
    pub fn report(&self) -> Value {
        json!({"ready":self.ready,"error":self.error,"total":self.items.len(),"visible":self.visible.len(),"search":self.search,"category":self.category,"mode":self.mode,"offset":self.offset})
    }
    fn accepts(&self, row: &Value) -> bool {
        if self.category != "all"
            && text(
                row,
                if self.page == Page::Relics {
                    "era"
                } else {
                    "group"
                },
            )
            .to_lowercase()
                != self.category
        {
            return false;
        }
        if !self.search.is_empty() {
            let search = self.search.to_lowercase();
            let matches = text(row, "name").to_lowercase().contains(&search)
                || entries(
                    &row[if self.page == Page::Relics {
                        "rewards"
                    } else {
                        "components"
                    }],
                )
                .iter()
                .any(|c| {
                    text(c, "name").to_lowercase().contains(&search)
                        || text(c, "full_name").to_lowercase().contains(&search)
                });
            if !matches {
                return false;
            }
        }
        for (setting, key) in [
            (&self.prime, "is_prime"),
            (&self.mastered, "mastered"),
            (&self.vaulted, "vaulted"),
        ] {
            if setting != "all" && row[key].as_bool() != Some(setting == "yes") {
                return false;
            }
        }
        match self.page {
            Page::Relics => !self.owned || number(row, "amount_owned") > 0.0,
            Page::Mastery => {
                if row["mastered"] == true {
                    return false;
                }
                match self.mode.as_str() {
                    "relics" => row["from_relics"] == true,
                    "platinum" => row["buyable"] == true,
                    _ => {
                        number(row, "rank") < number(row, "max_rank")
                            && (row["owned"] == true
                                || (row["has_recipe"] == true
                                    && number(row, "missing_parts") == 0.0))
                    }
                }
            }
            _ => {
                (self.duplicates == "all"
                    || (number(row, "quantity") > 1.0) == (self.duplicates == "yes"))
                    && (self.complete == "all"
                        || text(row, "group") != "sets"
                        || (number(row, "quantity") > 0.0) == (self.complete == "yes"))
            }
        }
    }
    fn price_names(&self) -> Vec<String> {
        let mut result = HashSet::new();
        for row in self.items.iter().filter(|r| self.accepts(r)) {
            if self.page == Page::Mastery {
                for part in entries(&row["components"]) {
                    if number(part, "required") > number(part, "owned")
                        && !text(part, "market_name").is_empty()
                    {
                        result.insert(text(part, "market_name").to_owned());
                    }
                }
            } else if self.page == Page::Relics {
                for part in entries(&row["rewards"]) {
                    if !text(part, "slug").is_empty() {
                        result.insert(text(part, "slug").to_owned());
                    }
                }
            } else if !text(row, "market_name").is_empty() {
                result.insert(text(row, "market_name").to_owned());
            }
        }
        let mut result = result.into_iter().collect::<Vec<_>>();
        result.sort();
        result
    }
    fn anchor(&self) -> Option<(String, u32)> {
        let first = self.offset / self.grid.stride();
        self.visible
            .get(first as usize * self.grid.columns)
            .and_then(|&i| self.items.get(i))
            .map(|row| (text(row, "id").to_owned(), self.offset % self.grid.stride()))
    }
    fn reorder(&mut self, quotes: &HashMap<String, Quote>, reset: bool) {
        self.reorder_anchored(quotes, reset, self.anchor());
    }
    fn reorder_anchored(
        &mut self,
        quotes: &HashMap<String, Quote>,
        reset: bool,
        anchor: Option<(String, u32)>,
    ) {
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, r)| self.accepts(r))
            .map(|(i, _)| i)
            .collect();
        let page = self.page;
        let mode = &self.mode;
        let sort = &self.sort;
        let descending = self.descending;
        self.visible.sort_by(|&a, &b| {
            let (a, b) = (&self.items[a], &self.items[b]);
            let comparison = match page {
                Page::Mastery => match mode.as_str() {
                    "relics" => {
                        number(b, "relic_probability").total_cmp(&number(a, "relic_probability"))
                    }
                    "platinum" => compare_known(score(a, quotes), score(b, quotes), true),
                    _ => b["owned"]
                        .as_bool()
                        .cmp(&a["owned"].as_bool())
                        .then_with(|| {
                            number(a, "missing_parts").total_cmp(&number(b, "missing_parts"))
                        })
                        .then_with(|| {
                            number(b, "potential_xp").total_cmp(&number(a, "potential_xp"))
                        }),
                },
                _ => match sort.as_str() {
                    "complete" => direction(completion(a).total_cmp(&completion(b)), descending),
                    "price" => {
                        compare_known(row_price(a, quotes), row_price(b, quotes), descending)
                    }
                    "ducats" => direction(
                        number(a, "ducats").total_cmp(&number(b, "ducats")),
                        descending,
                    ),
                    "amount" => direction(
                        number(
                            a,
                            if page == Page::Relics {
                                "amount_owned"
                            } else {
                                "quantity"
                            },
                        )
                        .total_cmp(&number(
                            b,
                            if page == Page::Relics {
                                "amount_owned"
                            } else {
                                "quantity"
                            },
                        )),
                        descending,
                    ),
                    "value" => compare_known(
                        row_price(a, quotes)
                            .filter(|p| *p > 0.0)
                            .map(|p| number(a, "ducats") / p),
                        row_price(b, quotes)
                            .filter(|p| *p > 0.0)
                            .map(|p| number(b, "ducats") / p),
                        descending,
                    ),
                    _ => direction(text(a, "name").cmp(text(b, "name")), descending),
                },
            };
            comparison
                .then_with(|| text(a, "name").cmp(text(b, "name")))
                .then_with(|| text(a, "id").cmp(text(b, "id")))
        });
        self.scroll.resize(
            self.grid.content_height(self.visible.len()) as f32,
            self.viewport.height as f32,
            Instant::now(),
        );
        if reset {
            self.scroll.to(0.0, false, Instant::now());
            self.offset = 0;
        } else if let Some((id, within)) = anchor
            && let Some(i) = self
                .visible
                .iter()
                .position(|&i| text(&self.items[i], "id") == id)
        {
            self.offset = (i / self.grid.columns) as u32 * self.grid.stride() + within;
            self.scroll.to(self.offset as f32, false, Instant::now());
        }
        self.offset = self.scroll.offset(Instant::now()).round() as u32;
    }
}

fn direction(order: Ordering, reverse: bool) -> Ordering {
    if reverse { order.reverse() } else { order }
}
fn compare_known(a: Option<f64>, b: Option<f64>, reverse: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => direction(a.total_cmp(&b), reverse),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}
fn row_price(row: &Value, quotes: &HashMap<String, Quote>) -> Option<f64> {
    quotes
        .get(text(row, "market_name"))
        .and_then(|q| q.sell)
        .or_else(|| row["expected_platinum"].as_f64())
}
fn completion(row: &Value) -> f64 {
    let parts = entries(&row["components"]);
    if parts.is_empty() {
        return 0.0;
    }
    parts
        .iter()
        .filter(|p| number(p, "owned") >= number(p, "required"))
        .count() as f64
        / parts.len() as f64
}
fn cost(row: &Value, quotes: &HashMap<String, Quote>) -> Option<f64> {
    let mut cost = 0.0;
    for part in entries(&row["components"]) {
        let missing = (number(part, "required") - number(part, "owned")).max(0.0);
        if missing == 0.0 {
            continue;
        }
        let name = text(part, "market_name");
        if name.is_empty() {
            if part["market_required"] == true {
                return None;
            }
            continue;
        }
        cost += quotes.get(name)?.sell? * missing;
    }
    Some(cost)
}
fn score(row: &Value, quotes: &HashMap<String, Quote>) -> Option<f64> {
    cost(row, quotes)
        .filter(|p| *p > 0.0)
        .map(|p| number(row, "potential_xp") / p)
}
pub(super) fn price(value: Option<f64>, known: bool) -> String {
    value
        .map(|v| format!("{v:.0}"))
        .unwrap_or_else(|| if known { "--" } else { "..." }.into())
}

impl Pages {
    pub fn capture_category(
        &mut self,
        from: usize,
        to: usize,
        elapsed: Duration,
    ) -> Result<(), String> {
        if !matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            return Err("category animation is unavailable on this page".into());
        }
        let kinds = categories(self.page);
        let (Some(from), Some(to)) = (kinds.get(from), kinds.get(to)) else {
            return Err("category index out of range".into());
        };
        let now = Instant::now();
        let listing = self.listing_mut();
        listing.category = from.0.into();
        listing.sync_category(false, now);
        listing.category = to.0.into();
        listing.sync_category(true, now);
        listing.tick(now + elapsed);
        self.resort(true);
        self.dirty = true;
        self.layout();
        Ok(())
    }

    pub(super) fn listing(&self) -> &Listing {
        match self.page {
            Page::Inventory => &self.inventory,
            Page::Mastery => &self.mastery,
            _ => &self.relics,
        }
    }
    pub(super) fn listing_mut(&mut self) -> &mut Listing {
        match self.page {
            Page::Inventory => &mut self.inventory,
            Page::Mastery => &mut self.mastery,
            _ => &mut self.relics,
        }
    }
    pub(super) fn select_listing_category(&mut self, page: Page, category: &str) {
        let listing = match page {
            Page::Inventory => &mut self.inventory,
            Page::Mastery => &mut self.mastery,
            Page::Relics => &mut self.relics,
            _ => return,
        };
        listing.category = category.into();
        listing.sync_category(true, Instant::now());
        listing.reorder(&self.quotes, true);
        self.dirty = true;
    }
    pub(super) fn resort(&mut self, reset: bool) {
        for listing in [&mut self.inventory, &mut self.mastery, &mut self.relics] {
            listing.reorder(&self.quotes, reset && listing.page == self.page);
        }
    }
    pub(super) fn start_prices(&mut self, refresh: bool) {
        if !matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            return;
        }
        self.resort(false);
        let names = self.listing().price_names();
        if self.fixture {
            return;
        }
        let new = names
            .into_iter()
            .filter(|n| !self.quote_pending.contains(n))
            .collect::<Vec<_>>();
        self.quote_pending.extend(new.iter().cloned());
        if refresh {
            self.quote_queue.extend(new);
            self.pump_prices();
            return;
        }
        for chunk in new.chunks(100) {
            let key = format!("cache:{}", chunk[0]);
            self.cache_pending += 1;
            self.cache_phase = true;
            self.queue(
                &key,
                json!({"op":"market_quote","items":chunk,"cache_only":true}),
            );
        }
    }
    fn pump_prices(&mut self) {
        let active = self
            .pending
            .keys()
            .filter(|k| k.starts_with("price:"))
            .count();
        for _ in active..2 {
            let names = self
                .quote_queue
                .drain(..self.quote_queue.len().min(4))
                .collect::<Vec<_>>();
            if names.is_empty() {
                break;
            }
            let key = format!("price:{}", names[0]);
            self.queue(&key, json!({"op":"market_quote","items":names,"ttl":60}));
        }
    }
    pub(super) fn price_reply(&mut self, request: &Request, result: Result<Value, String>) {
        let cached = request.key.starts_with("cache:");
        if let Ok(data) = &result {
            for row in entries(&data["quotes"]) {
                let name = text(row, "item");
                if name.is_empty() {
                    continue;
                }
                let quote = &row["quote"];
                if quote.is_object() {
                    self.quotes.insert(
                        name.into(),
                        Quote {
                            sell: quote["lowest_sell"].as_f64(),
                            buy: quote["highest_buy"].as_f64(),
                            resolved: true,
                        },
                    );
                } else if !cached && !self.quotes.contains_key(name) {
                    self.quotes.insert(
                        name.into(),
                        Quote {
                            resolved: true,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        let names = entries(&request.body["items"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if cached {
            self.quote_queue.extend(names);
            self.cache_pending = self.cache_pending.saturating_sub(1);
            if self.cache_pending == 0 {
                self.cache_phase = false;
                self.resort(false);
                self.pump_prices();
            }
        } else {
            for name in names {
                self.quote_pending.remove(&name);
                self.quotes.entry(name).or_default().resolved = true;
            }
            self.pump_prices();
            if self.quote_pending.is_empty() {
                self.resort(false);
                if self.page == Page::Relics && self.relics.ready {
                    self.queue("relics:values",json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"}));
                }
            }
        }
    }
    pub(super) fn layout_listing(&mut self) {
        let scale = self.ui.scale;
        let w = self.ui.size.0 as f32 / scale;
        let h = self.ui.size.1 as f32 / scale;
        let mut top = 12.0;
        if self.page == Page::Mastery {
            top = self.mastery_summary(w);
        }
        let toolbar_top = top;
        let (collapsed_width, tab_height, icon_size) = if self.page == Page::Relics {
            (40.0, 32.0, 24.0)
        } else {
            (38.0, 34.0, 20.0)
        };
        let categories = categories(self.page);
        self.listing_mut().sync_category(false, Instant::now());
        let mut x = 12.0;
        for (index, &(id, label, icon)) in categories.iter().enumerate() {
            let listing = self.listing();
            let weight = listing
                .category_motion
                .weight(category_key(index), listing.category_sampled_at);
            let width = collapsed_width + (self.ui.measure(label, 13.0, false) + 5.0) * weight;
            let r = self.ui.rect(x, top, width, tab_height);
            self.ui.fill(
                r,
                tab_height / 2.0,
                std::array::from_fn(|i| {
                    (widgets::CARD[i] as f32
                        + (ACCENT[i] as f32 - widgets::CARD[i] as f32) * weight)
                        .round() as u8
                }),
            );
            let padding = (collapsed_width - icon_size) / 2.0;
            let icon_bounds = self.ui.rect(
                x + padding,
                top + (tab_height - icon_size) / 2.0,
                icon_size,
                icon_size,
            );
            self.ui.icon(icon_bounds, icon);
            self.ui
                .region(&format!("list:category:{id}:icon"), icon_bounds);
            if weight > 0.0 {
                let mut color = TEXT;
                color[3] = (255.0 * weight).round() as u8;
                self.ui.clipped_text(
                    self.ui.rect(
                        x + padding + icon_size + 5.0,
                        top,
                        (width - collapsed_width - 5.0).max(0.0),
                        tab_height,
                    ),
                    label,
                    13.0,
                    color,
                    widgets::Weight::Regular,
                );
            }
            self.ui.hit(format!("list:category:{id}"), r, label, true);
            x += width + 5.0;
        }
        let tools = match self.page {
            Page::Inventory => 247.0,
            Page::Relics => 272.0,
            _ => 44.0,
        };
        let reserved = 12.0
            + categories.len() as f32 * (collapsed_width + 5.0)
            + categories
                .iter()
                .map(|(_, label, _)| self.ui.measure(label, 13.0, false) + 5.0)
                .fold(0.0, f32::max);
        if w - reserved - tools < 132.0 {
            top += 44.0;
            x = 12.0;
        }
        let search = self.listing().search.clone();
        self.ui.search_field(
            "list:search",
            self.ui
                .rect(x + 5.0, top, (w - x - tools - 17.0).max(80.0), 32.0),
            &search,
            if self.page == Page::Relics {
                "Search relics or rewards"
            } else {
                "Search items and components"
            },
            if self.page == Page::Relics {
                crate::search::Anchor::Trailing
            } else {
                crate::search::Anchor::Leading
            },
        );
        if self.page == Page::Inventory {
            let sort = self.inventory.sort.clone();
            self.ui.choice(
                "list:sort",
                self.ui.rect(w - 247.0, top, 155.0, 32.0),
                &sort,
                &[
                    ("name", "Name"),
                    ("price", "Platinum"),
                    ("ducats", "Ducats"),
                    ("amount", "Amount"),
                    ("value", "Ducats / platinum"),
                    ("complete", "Set completion"),
                ],
            );
            self.ui.arrow(
                self.ui.rect(w - 85.0, top + 8.0, 17.0, 17.0),
                self.listing().descending,
                TEXT,
            );
            self.ui.hit(
                "list:direction",
                self.ui.rect(w - 91.0, top, 34.0, 32.0),
                if self.listing().descending {
                    "Descending"
                } else {
                    "Ascending"
                },
                true,
            );
        }
        if self.page == Page::Relics {
            self.ui
                .icon(self.ui.rect(w - 270.0, top + 5.0, 22.0, 22.0), "traces");
            self.ui.text(
                self.ui.rect(w - 242.0, top, 104.0, 32.0),
                format!("{:.0} traces", number(&self.relics.data, "trace_count")),
                13.0,
                TEXT,
            );
            self.ui.toggle(
                "list:owned",
                self.ui.rect(w - 136.0, top, 89.0, 32.0),
                "Owned",
                self.relics.owned,
            );
            self.ui
                .icon(self.ui.rect(w - 39.0, top + 7.0, 20.0, 20.0), "refresh");
            self.ui.hit(
                "list:retry",
                self.ui.rect(w - 45.0, top, 32.0, 32.0),
                "Refresh relics",
                true,
            );
        } else {
            self.ui
                .icon(self.ui.rect(w - 45.0, top + 6.0, 20.0, 20.0), "filter");
            self.ui.hit(
                "list:filters",
                self.ui.rect(w - 53.0, top, 40.0, 32.0),
                "Filters",
                true,
            );
        }
        self.ui.region(
            "page.toolbar",
            self.ui
                .rect(12.0, toolbar_top, w - 24.0, top - toolbar_top + 34.0),
        );
        top += 42.0;
        if self.page == Page::Mastery {
            self.ui.fill(
                self.ui.rect(4.0, top - 4.0, w - 8.0, h - top + 4.0),
                8.0,
                crate::style::INSET_PANEL,
            );
            self.ui.fill(
                self.ui
                    .rect(4.0, top + 8.0, w - 8.0, (h - top - 8.0).max(0.0)),
                0.0,
                crate::style::INSET_PANEL,
            );
            self.ui.text(
                self.ui.rect(16.0, top, 230.0, 34.0),
                "Best ways to level mastery",
                18.0,
                TEXT,
            );
            let mode = self.mastery.mode.clone();
            let modes_top = if w < 610.0 { top + 37.0 } else { top };
            let mut x = (w - 359.0).max(16.0);
            self.ui
                .fill(self.ui.rect(x, modes_top, 343.0, 34.0), 10.0, widgets::CARD);
            for (id, label) in [
                ("easy", "Easy"),
                ("relics", "From relics"),
                ("platinum", "With platinum"),
            ] {
                let width = if id == "easy" { 79.0 } else { 132.0 };
                let bounds = self.ui.rect(x, modes_top, width, 34.0);
                if mode == id {
                    self.ui.fill(bounds, 10.0, crate::style::SUMMARY_HEADER);
                }
                self.ui.icon(
                    self.ui.rect(x + 8.0, modes_top + 7.0, 20.0, 20.0),
                    match id {
                        "relics" => "axi",
                        "platinum" => "platinum",
                        _ => "easy",
                    },
                );
                self.ui.aligned(
                    self.ui.rect(x + 33.0, modes_top, width - 38.0, 34.0),
                    label,
                    15.0,
                    TEXT,
                    widgets::Weight::Light,
                    0.0,
                );
                self.ui.hit(format!("list:mode:{id}"), bounds, label, true);
                x += width;
            }
            top = modes_top + 44.0;
        }
        if self.listing().filters {
            let l = self.listing();
            let (prime, mastered, vaulted) =
                (l.prime.clone(), l.mastered.clone(), l.vaulted.clone());
            let width = (w - 40.0) / 3.0;
            for (i, (id, value, label)) in [
                ("prime", prime, "Prime"),
                ("mastered", mastered, "Mastered"),
                ("vaulted", vaulted, "Vaulted"),
            ]
            .into_iter()
            .enumerate()
            {
                self.ui.choice(
                    &format!("list:{id}"),
                    self.ui
                        .rect(12.0 + i as f32 * (width + 8.0), top, width, 30.0),
                    &value,
                    &[
                        ("all", &format!("Any {label}")),
                        ("yes", label),
                        ("no", &format!("Not {}", label.to_lowercase())),
                    ],
                );
            }
            top += 38.0;
            if self.page == Page::Inventory {
                self.ui.choice(
                    "list:duplicates",
                    self.ui.rect(12.0, top, 180.0, 30.0),
                    &self.inventory.duplicates,
                    &[
                        ("all", "Any number of copies"),
                        ("yes", "More than one copy"),
                        ("no", "One copy"),
                    ],
                );
                self.ui.choice(
                    "list:complete",
                    self.ui.rect(200.0, top, 180.0, 30.0),
                    &self.inventory.complete,
                    &[
                        ("all", "Any set completion"),
                        ("yes", "Complete sets"),
                        ("no", "Incomplete sets"),
                    ],
                );
                top += 36.0;
            }
        }
        let scroll_area = self.ui.rect(10.0, top, w - 20.0, (h - top).max(0.0));
        let viewport = Rect {
            width: scroll_area.width.saturating_sub(self.ui.px(12.0)),
            ..scroll_area
        };
        self.ui.region("page.grid", viewport);
        self.ui.region(
            "page.toolbar",
            self.ui.rect(10.0, 12.0, w - 20.0, top - 12.0),
        );
        let page = self.page;
        let grid = Grid::new(
            viewport.width,
            self.ui.px(if page == Page::Relics {
                420.0
            } else if page == Page::Mastery {
                370.0
            } else {
                420.0
            }),
            self.ui.px(match page {
                Page::Mastery => 91.0,
                Page::Relics => 100.0,
                _ => 130.0,
            }),
            self.ui.px(8.0),
        );
        let listing = self.listing_mut();
        listing.viewport = viewport;
        listing.grid = grid;
        listing.scroll.resize(
            grid.content_height(listing.visible.len()) as f32,
            viewport.height as f32,
            Instant::now(),
        );
        let rows = grid
            .visible(listing.visible.len(), listing.offset, viewport.height, 0)
            .map(|index| {
                (
                    index,
                    listing.visible[index],
                    listing.items[listing.visible[index]].clone(),
                )
            })
            .collect::<Vec<_>>();
        let offset = listing.offset;
        let empty = listing.visible.is_empty();
        let ready = listing.ready;
        let error = listing.error.clone();
        self.ui.begin_scroll(viewport, offset);
        for (i, (position, index, row)) in rows.into_iter().enumerate() {
            self.ui.begin_scope((
                page as u8,
                text(&row, "id"),
                text(&row, "name"),
                row["rank"].as_u64(),
            ));
            let (x, y) = grid.origin(position);
            let rect = Rect {
                x,
                y,
                width: grid.width,
                height: grid.height,
            };
            if i == 0 {
                self.ui.region("page.card", rect);
            }
            self.ui
                .hit(format!("list:item:{index}:menu"), rect, "", true);
            match page {
                Page::Relics => self.relic_card(index, &row, rect),
                Page::Mastery => self.mastery_card(index, &row, rect),
                _ => self.inventory_card(index, &row, rect),
            }
            self.ui.end_scope();
        }
        self.ui.end_scroll();
        if empty {
            let message = error.as_deref().unwrap_or(if !ready {
                "Loading player data..."
            } else {
                "No matching items"
            });
            self.ui.text(
                self.ui.rect(24.0, top + 25.0, w - 48.0, 40.0),
                message,
                16.0,
                if error.is_some() { BAD } else { MUTED },
            );
            if error.is_some() {
                self.ui.button(
                    "list:retry",
                    self.ui.rect(24.0, top + 72.0, 100.0, 32.0),
                    "Retry",
                    false,
                    true,
                );
            }
        }
        let max = self.listing().scroll.maximum();
        self.ui
            .scrollbar("list:scroll", scroll_area, max, offset as f32);
        self.listing_menu();
    }
    fn mastery_summary(&mut self, w: f32) -> f32 {
        let summary = self.mastery.data["summary"].clone();
        let rank = &summary["rank_progress"];
        self.ui.text(
            self.ui.rect(12.0, 8.0, 66.0, 65.0),
            format!("{:.0}", number(&summary, "player_level")),
            51.0,
            TEXT,
        );
        self.ui.image(
            self.ui.rect(81.0, 10.0, 60.0, 60.0),
            asset(&self.mastery.data["profile"]["rank_asset"]).as_ref(),
        );
        let progress = (number(rank, "percent") / 100.0).clamp(0.0, 1.0) as f32;
        let bar_width = (w - 170.0).max(1.0);
        self.ui.fill(
            self.ui.rect(154.0, 46.0, bar_width, 19.0),
            9.5,
            widgets::CARD,
        );
        let favored = self
            .mastery
            .items
            .iter()
            .filter(|i| self.preferences.is_favorite(text(i, "id")))
            .map(|i| number(i, "potential_xp"))
            .sum::<f64>();
        let extension = (favored / number(rank, "total").max(1.0)) as f32;
        if extension > 0.0 {
            self.ui.fill(
                self.ui.rect(
                    154.0,
                    46.0,
                    bar_width * (progress + extension).min(1.0),
                    19.0,
                ),
                9.5,
                crate::settings::rgb(0xcfb14d),
            );
        }
        self.ui.fill(
            self.ui.rect(154.0, 46.0, bar_width * progress, 19.0),
            9.5,
            ACCENT,
        );
        self.ui.text(
            self.ui.rect(154.0, 17.0, 85.0, 25.0),
            format!("{:.0}%", number(rank, "percent")),
            17.0,
            TEXT,
        );
        let xp = if rank["available"] == true {
            format!(
                "{:.0} / {:.0} XP",
                number(rank, "current"),
                number(rank, "total")
            )
        } else {
            "Mastery XP unavailable".into()
        };
        let xp_width = retinue_ui::painter::text_width(&self.ui.regular, &xp, 17.0 * self.ui.scale)
            / self.ui.scale;
        self.ui.text(
            self.ui.rect(
                154.0 + (bar_width - xp_width).max(0.0) / 2.0,
                17.0,
                xp_width + 1.0,
                25.0,
            ),
            xp,
            17.0,
            TEXT,
        );
        let blocks = [
            (
                "Game content",
                "summary_game",
                number(&summary, "content_percent"),
                vec![
                    ("Warframes / Archwings:", &summary["warframes"], "mastered"),
                    ("Weapons:", &summary["weapons"], "mastered"),
                    ("Companions:", &summary["companions"], "mastered"),
                ],
            ),
            (
                "Star Chart",
                "summary_star",
                number(&summary["star_chart"], "percent"),
                vec![
                    ("Normal:", &summary["star_chart"]["normal"], "current"),
                    ("Junctions:", &summary["star_chart"]["junctions"], "current"),
                    ("Steel Path:", &summary["star_chart"]["steel"], "current"),
                    (
                        "Steel P. junctions:",
                        &summary["star_chart"]["steel_junctions"],
                        "current",
                    ),
                ],
            ),
            (
                "Intrinsics",
                "summary_intrinsics",
                number(&summary["intrinsics"], "percent"),
                vec![
                    ("Railjack:", &summary["intrinsics"]["railjack"], "current"),
                    ("Duviri:", &summary["intrinsics"]["duviri"], "current"),
                ],
            ),
        ];
        let widths: Vec<f32> = blocks
            .iter()
            .enumerate()
            .map(|(i, (_, _, _, rows))| {
                let label = rows
                    .iter()
                    .map(|(label, _, _)| self.ui.measure(label, 14.67, false))
                    .fold(0.0, f32::max);
                (label + 116.0).max([290.0, 252.0, 180.0][i])
            })
            .collect();
        let positions = summary_positions(w - 24.0, &widths);
        let mut bottom = 0.0_f32;
        for (i, (label, icon, percent, rows)) in blocks.into_iter().enumerate() {
            let (x, y) = (12.0 + positions[i].0, 86.0 + positions[i].1);
            let width = widths[i];
            let panel = self.ui.rect(x, y, width, 151.0);
            self.ui.region(
                [
                    "mastery.summary.game",
                    "mastery.summary.star",
                    "mastery.summary.intrinsics",
                ][i],
                panel,
            );
            self.ui.fill(panel, 10.0, widgets::CARD);
            self.ui.fill(
                self.ui.rect(x, y, width, 40.0),
                10.0,
                crate::style::SUMMARY_HEADER,
            );
            self.ui.fill(
                self.ui.rect(x, y + 30.0, width, 10.0),
                0.0,
                crate::style::SUMMARY_HEADER,
            );
            let percent = format!("{percent:.0}%");
            let title_width = self.ui.measure(label, 17.0, false);
            let percent_width = self.ui.measure(&percent, 17.0, false);
            let header_x = x + (width - 30.0 - 7.0 - title_width - 7.0 - percent_width) / 2.0;
            self.ui
                .icon(self.ui.rect(header_x, y + 5.0, 30.0, 30.0), icon);
            self.ui.text(
                self.ui
                    .rect(header_x + 37.0, y + 5.0, title_width + 1.0, 30.0),
                label,
                17.0,
                TEXT,
            );
            self.ui.text(
                self.ui.rect(
                    header_x + 44.0 + title_width,
                    y + 5.0,
                    percent_width + 1.0,
                    30.0,
                ),
                percent,
                17.0,
                crate::style::SUMMARY_PERCENT,
            );
            let percent_width = rows
                .iter()
                .map(|(_, v, _)| {
                    self.ui
                        .measure(&format!("{:.0}%", number(v, "percent")), 14.67, false)
                })
                .fold(0.0, f32::max)
                + 3.0;
            let fraction_width = rows
                .iter()
                .map(|(_, v, current)| {
                    self.ui.measure(
                        &format!("{:.0}/{:.0}", number(v, current), number(v, "total")),
                        14.67,
                        false,
                    )
                })
                .fold(0.0, f32::max)
                + 3.0;
            let label_width = width - percent_width - fraction_width - 34.0;
            let row_height = 101.0 / rows.len() as f32;
            for (j, (label, value, current)) in rows.into_iter().enumerate() {
                let top = y + 45.0 + j as f32 * row_height;
                self.ui.aligned(
                    self.ui.rect(x + 8.0, top, label_width, row_height),
                    label,
                    14.67,
                    TEXT,
                    widgets::Weight::Regular,
                    1.0,
                );
                self.ui.aligned(
                    self.ui
                        .rect(x + label_width + 15.0, top, percent_width, row_height),
                    if value["percent"].is_number() {
                        format!("{:.0}%", number(value, "percent"))
                    } else {
                        "--".into()
                    },
                    14.67,
                    TEXT,
                    widgets::Weight::Light,
                    0.5,
                );
                self.ui.aligned(
                    self.ui.rect(
                        x + width - fraction_width - 8.0,
                        top,
                        fraction_width,
                        row_height,
                    ),
                    if value["total"].is_number() {
                        format!(
                            "{:.0}/{:.0}",
                            number(value, current),
                            number(value, "total")
                        )
                    } else {
                        "--".into()
                    },
                    14.67,
                    TEXT,
                    widgets::Weight::Light,
                    0.5,
                );
            }
            bottom = bottom.max(y + 151.0);
        }
        self.ui
            .region("page.summary", self.ui.rect(12.0, 8.0, w - 24.0, bottom));
        if !self.quote_pending.is_empty() {
            self.ui
                .progress(self.ui.rect(16.0, bottom + 8.0, w - 32.0, 3.0));
        }
        bottom + 23.0
    }
    pub(super) fn listing_signal(&mut self, signal: Signal) {
        let key = &signal.key;
        if key == "list:scroll" {
            if let Some(value) = signal.value.and_then(|v| v.parse::<f32>().ok()) {
                let listing = self.listing_mut();
                listing.scroll.to(value, false, Instant::now());
                listing.tick(Instant::now());
            }
            return;
        }
        if key == "list:retry" {
            self.refresh();
            return;
        }
        if key == "list:menu:close" {
            self.listing_mut().menu = None;
            self.ui.close_context();
            return;
        }
        if let Some(action) = key.strip_prefix("list:menu:") {
            if let Some((index, component)) = self.listing().menu {
                let Some(row) = self.listing().items.get(index) else {
                    self.listing_mut().menu = None;
                    self.ui.close_context();
                    return;
                };
                let target = component
                    .and_then(|i| {
                        entries(
                            &row[if self.page == Page::Relics {
                                "rewards"
                            } else {
                                "components"
                            }],
                        )
                        .get(i)
                    })
                    .unwrap_or(row);
                let name = text(target, "full_name");
                let name = if name.is_empty() {
                    text(target, "name")
                } else {
                    name
                }
                .to_owned();
                match action {
                    "relics" => self.navigation = Some((Page::Relics, name)),
                    "foundry" => self.navigation = Some((Page::Foundry, name)),
                    "market" => self.open_market(
                        text(
                            target,
                            if self.page == Page::Relics {
                                "slug"
                            } else {
                                "market_name"
                            },
                        )
                        .to_owned(),
                        "sell",
                    ),
                    _ => (),
                }
            }
            self.listing_mut().menu = None;
            self.ui.close_context();
            return;
        }
        if let Some(rest) = key.strip_prefix("list:item:") {
            let mut parts = rest.split(':');
            let Some(index) = parts.next().and_then(|v| v.parse::<usize>().ok()) else {
                return;
            };
            if index >= self.listing().items.len() {
                return;
            }
            let action = parts.next().unwrap_or("menu");
            let component = parts.next().and_then(|v| v.parse::<usize>().ok());
            let row = &self.listing().items[index];
            if action == "favorite" {
                self.preferences.toggle_favorite(text(row, "id"));
            } else if matches!(action, "sell" | "buy" | "reward" | "component") {
                let target = if action == "reward" {
                    component
                        .and_then(|i| entries(&row["rewards"]).get(i))
                        .unwrap_or(row)
                } else if action == "component" {
                    component
                        .and_then(|i| entries(&row["components"]).get(i))
                        .unwrap_or(row)
                } else {
                    row
                };
                let name = text(
                    target,
                    if action == "reward" {
                        "slug"
                    } else {
                        "market_name"
                    },
                )
                .to_owned();
                let owned = number(target, "owned");
                let required = number(target, "required");
                if name.is_empty() {
                    self.open_listing_menu(key);
                } else {
                    self.open_market(
                        name,
                        if action == "sell"
                            || (action == "component" && owned >= required && required > 0.0)
                        {
                            "buy"
                        } else {
                            "sell"
                        },
                    );
                }
            } else {
                self.open_listing_menu(key);
            }
            return;
        }
        if let Some(category) = key.strip_prefix("list:category:") {
            self.select_listing_category(self.page, category);
            self.start_prices(false);
            return;
        }
        let listing = self.listing_mut();
        if let Some(mode) = key.strip_prefix("list:mode:") {
            listing.mode = mode.into();
        } else {
            match key.as_str() {
                "list:search" => listing.search = signal.value.unwrap_or_default(),
                "list:sort" => listing.sort = signal.value.unwrap_or_else(|| "name".into()),
                "list:direction" => listing.descending = !listing.descending,
                "list:owned" => listing.owned = !listing.owned,
                "list:filters" => listing.filters = !listing.filters,
                "list:prime" => listing.prime = signal.value.unwrap_or_else(|| "all".into()),
                "list:mastered" => listing.mastered = signal.value.unwrap_or_else(|| "all".into()),
                "list:vaulted" => listing.vaulted = signal.value.unwrap_or_else(|| "all".into()),
                "list:duplicates" => {
                    listing.duplicates = signal.value.unwrap_or_else(|| "all".into())
                }
                "list:complete" => listing.complete = signal.value.unwrap_or_else(|| "all".into()),
                _ => return,
            }
        }
        self.resort(true);
        self.start_prices(false);
    }
    pub(super) fn open_listing_menu(&mut self, key: &str) -> bool {
        let Some(rest) = key.strip_prefix("list:item:") else {
            return false;
        };
        let mut fields = rest.split(':');
        let Some(index) = fields.next().and_then(|s| s.parse::<usize>().ok()) else {
            return false;
        };
        let action = fields.next().unwrap_or("");
        let component = if matches!(action, "component" | "reward") {
            fields.next().and_then(|s| s.parse().ok())
        } else {
            None
        };
        if index >= self.listing().items.len() {
            return false;
        }
        let anchor = self.ui.pointer();
        self.ui.open_context(key.into());
        let listing = self.listing_mut();
        listing.menu = Some((index, component));
        listing.menu_anchor = anchor;
        self.dirty = true;
        self.layout();
        true
    }
    pub(super) fn dismiss_listing_menu(&mut self, point: (f64, f64)) -> bool {
        if !matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics)
            || self.listing().menu.is_none()
        {
            return false;
        }
        if self
            .ui
            .bounds("page.menu")
            .is_some_and(|r| r.contains(point.0, point.1))
        {
            return false;
        }
        self.listing_mut().menu = None;
        self.ui.close_context();
        self.dirty = true;
        self.layout();
        true
    }
    fn listing_menu(&mut self) {
        let Some((index, component)) = self.listing().menu else {
            return;
        };
        let Some(row) = self.listing().items.get(index) else {
            return;
        };
        let target = component
            .and_then(|i| {
                entries(
                    &row[if self.page == Page::Relics {
                        "rewards"
                    } else {
                        "components"
                    }],
                )
                .get(i)
            })
            .unwrap_or(row);
        let name = if text(target, "full_name").is_empty() {
            text(target, "name")
        } else {
            text(target, "full_name")
        }
        .to_owned();
        let market = !text(
            target,
            if self.page == Page::Relics {
                "slug"
            } else {
                "market_name"
            },
        )
        .is_empty();
        let anchor = self.listing().menu_anchor;
        let entries: Vec<_> = [
            ("foundry", "Show in Foundry", true),
            ("relics", "Show in Relic Planner", true),
            ("market", "View market listings", market),
        ]
        .into_iter()
        .map(|(id, label, enabled)| (format!("list:menu:{id}"), label.into(), enabled))
        .collect();
        self.ui.context_menu(
            Rect {
                x: anchor.0.max(0.0) as u32,
                y: anchor.1.max(0.0) as u32,
                ..Rect::default()
            },
            &name,
            &[],
            &entries,
            "list:menu:close",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_motion_retargets_and_stops_requesting_frames() {
        let mut listing = Listing::new(Page::Relics);
        let now = Instant::now();
        listing.category = "axi".into();
        listing.sync_category(true, now);
        let mid = now + Duration::from_millis(85);
        assert!(listing.tick(mid));
        assert_eq!(listing.category_motion.weight(category_key(0), mid), 0.5);
        assert_eq!(listing.category_motion.weight(category_key(4), mid), 0.5);
        listing.sync_category(false, mid);
        assert_eq!(listing.category_motion.weight(category_key(4), mid), 0.5);
        listing.category = "requiem".into();
        listing.sync_category(true, mid);
        assert_eq!(listing.category_motion.weight(category_key(4), mid), 0.5);
        let end = mid + Duration::from_millis(170);
        assert!(listing.tick(end));
        assert_eq!(listing.category_motion.weight(category_key(5), end), 1.0);
        assert_eq!(listing.deadline(end), None);
        assert!(!listing.tick(end + Duration::from_millis(1)));
    }

    #[test]
    fn snapshots_preserve_visible_identity_and_drop_stale_row_indices() {
        let mut listing = Listing::new(Page::Inventory);
        listing.grid = Grid::new(300, 300, 100, 8);
        listing.viewport.height = 200;
        let snapshot = |names: &[&str]| json!({"items":names.iter().map(|name|json!({"id":name,"name":name,"group":"parts"})).collect::<Vec<_>>()});
        let quotes = HashMap::new();
        listing.replace(snapshot(&["A", "B", "C", "D", "E", "F", "G", "H"]), &quotes);
        listing.offset = 2 * listing.grid.stride() + 7;
        listing
            .scroll
            .to(listing.offset as f32, false, Instant::now());
        listing.replace(
            snapshot(&["H", "G", "F", "E", "D", "C", "B", "AA", "A"]),
            &quotes,
        );
        assert_eq!(listing.offset, 3 * listing.grid.stride() + 7);
        assert_eq!(listing.anchor(), Some(("C".into(), 7)));
        listing.replace(snapshot(&["Z", "C"]), &quotes);
        assert_eq!(listing.visible, vec![1, 0]);
        assert!(listing.offset <= listing.scroll.maximum() as u32);
        listing.replace(snapshot(&[]), &quotes);
        assert!(listing.visible.is_empty());
        assert_eq!(listing.offset, 0);
    }

    #[test]
    fn platinum_order_is_xp_per_cost_not_raw_price() {
        let mut listing = Listing::new(Page::Mastery);
        listing.mode = "platinum".into();
        listing.items = vec![
            json!({"id":"weapon","name":"Lato","potential_xp":3000,"buyable":true,"components":[{"market_name":"lato","required":1}]}),
            json!({"id":"frame","name":"Voruna","potential_xp":6000,"buyable":true,"components":[{"market_name":"voruna","required":1}]}),
        ];
        let prices = HashMap::from([
            (
                "lato".into(),
                Quote {
                    sell: Some(9.0),
                    ..Default::default()
                },
            ),
            (
                "voruna".into(),
                Quote {
                    sell: Some(15.0),
                    ..Default::default()
                },
            ),
        ]);
        listing.reorder(&prices, true);
        assert_eq!(listing.visible, vec![1, 0]);
    }
    #[test]
    fn relic_search_finds_rewards_and_owned_is_independent() {
        let mut listing = Listing::new(Page::Relics);
        listing.search = "Ash Prime".into();
        let relic =
            json!({"name":"Lith A1","amount_owned":0,"rewards":[{"name":"Ash Prime Blueprint"}]});
        assert!(!listing.accepts(&relic));
        listing.owned = false;
        assert!(listing.accepts(&relic));
    }
    #[test]
    fn missing_required_market_price_is_not_free() {
        let row = json!({"components":[{"required":1,"market_required":true}]});
        assert_eq!(cost(&row, &HashMap::new()), None);
    }
}
