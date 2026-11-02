mod cards;
mod filters;

use super::*;
use controls::{ACCENT, BAD, MUTED, TEXT};
use retinue_protocol::{warframe::ListingItem, warframe::ListingSnapshot};
use retinue_render::Key as ElementKey;
use retinue_ui::{grid::Grid, scene::SelectionMotion};
use std::cmp::Ordering;

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

#[derive(Clone, Default, PartialEq)]
pub struct Quote {
    pub sell: Option<f64>,
    pub buy: Option<f64>,
    pub resolved: bool,
}

pub struct Listing {
    pub page: Page,
    pub data: Value,
    pub items: Vec<ListingItem>,
    pub visible: Vec<usize>,
    pub ready: bool,
    pub error: Option<String>,
    pub category: String,
    pub search: String,
    pub mode: String,
    pub sort: String,
    pub descending: bool,
    pub owned: bool,
    filters: retinue_ui::popover::Popover,
    filter_scroll: Scroll,
    filter_offset: u32,
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
    order_revision: u64,
}

impl Listing {
    pub(super) fn revision(&self) -> u64 {
        self.order_revision
    }

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
            filters: Default::default(),
            filter_scroll: Scroll::default(),
            filter_offset: 0,
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
            order_revision: 0,
        }
    }
    pub fn replace(&mut self, mut data: ListingSnapshot, quotes: &HashMap<String, Quote>) {
        let anchor = self.anchor();
        self.menu = None;
        self.items = std::mem::take(&mut data.items);
        self.data = serde_json::to_value(data).expect("listing metadata");
        self.ready = true;
        self.error = None;
        self.reorder_anchored(quotes, false, anchor);
    }
    pub fn wheel(&mut self, delta: f32, smooth: bool) {
        self.scroll.by(delta, smooth, Instant::now());
    }
    pub fn replace_fixture(&mut self, data: Value, quotes: &HashMap<String, Quote>) {
        self.replace(
            serde_json::from_value(data).expect("listing fixture"),
            quotes,
        );
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let offset = self.scroll.offset(now).round() as u32;
        let filter_offset = self.filter_scroll.offset(now).round() as u32;
        let changed = offset != self.offset
            || filter_offset != self.filter_offset
            || self
                .category_motion
                .deadline(self.category_sampled_at)
                .is_some();
        self.offset = offset;
        self.filter_offset = filter_offset;
        self.category_sampled_at = now;
        changed
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        [
            self.scroll.deadline(now),
            self.category_motion.deadline(now),
            self.filters
                .is_open()
                .then(|| self.filter_scroll.deadline(now))
                .flatten(),
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
    fn accepts(&self, row: &ListingItem) -> bool {
        if self.category != "all"
            && (if self.page == Page::Relics {
                &row.era
            } else {
                &row.group
            })
            .to_lowercase()
                != self.category
        {
            return false;
        }
        if !self.search.is_empty() {
            let search = self.search.to_lowercase();
            let matches = row.name.to_lowercase().contains(&search)
                || if self.page == Page::Relics {
                    row.rewards
                        .iter()
                        .any(|c| c.name.to_lowercase().contains(&search))
                } else {
                    row.components.iter().any(|c| {
                        c.name.to_lowercase().contains(&search)
                            || c.full_name.to_lowercase().contains(&search)
                    })
                };
            if !matches {
                return false;
            }
        }
        for (setting, value) in [
            (&self.prime, Some(row.is_prime)),
            (&self.mastered, Some(row.mastered)),
            (&self.vaulted, row.vaulted),
        ] {
            if setting != "all" && value != Some(setting == "yes") {
                return false;
            }
        }
        match self.page {
            Page::Relics => !self.owned || (row.amount_owned as f64) > 0.0,
            Page::Mastery => {
                if row.mastered {
                    return false;
                }
                match self.mode.as_str() {
                    "relics" => row.from_relics,
                    "platinum" => row.buyable,
                    _ => {
                        (row.rank as f64) < (row.max_rank as f64)
                            && (row.owned || (row.has_recipe && (row.missing_parts as f64) == 0.0))
                    }
                }
            }
            _ => {
                (self.duplicates == "all"
                    || ((row.quantity as f64) > 1.0) == (self.duplicates == "yes"))
                    && (self.complete == "all"
                        || row.group.as_str() != "sets"
                        || ((row.quantity as f64) > 0.0) == (self.complete == "yes"))
            }
        }
    }
    pub(super) fn price_names(&self) -> Vec<String> {
        let mut result = HashSet::new();
        for row in self.items.iter().filter(|r| self.accepts(r)) {
            if self.page == Page::Mastery {
                for part in &row.components {
                    if (part.required as f64) > (part.owned as f64)
                        && !part.market_name.as_deref().unwrap_or("").is_empty()
                    {
                        result.insert(part.market_name.as_deref().unwrap_or("").to_owned());
                    }
                }
            } else if self.page == Page::Relics {
                for part in &row.rewards {
                    if !part.slug.as_deref().unwrap_or("").is_empty() {
                        result.insert(part.slug.as_deref().unwrap_or("").to_owned());
                    }
                }
            } else if !row.market_name.as_deref().unwrap_or("").is_empty() {
                result.insert(row.market_name.as_deref().unwrap_or("").to_owned());
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
            .map(|row| (row.id.as_str().to_owned(), self.offset % self.grid.stride()))
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
        self.order_revision = self.order_revision.wrapping_add(1);
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
                    "relics" => b.relic_probability.total_cmp(&a.relic_probability),
                    "platinum" => compare_known(score(a, quotes), score(b, quotes), true),
                    _ => b
                        .owned
                        .cmp(&a.owned)
                        .then_with(|| (a.missing_parts as f64).total_cmp(&(b.missing_parts as f64)))
                        .then_with(|| (b.potential_xp as f64).total_cmp(&(a.potential_xp as f64))),
                },
                _ => match sort.as_str() {
                    "complete" => direction(completion(a).total_cmp(&completion(b)), descending),
                    "price" => {
                        compare_known(row_price(a, quotes), row_price(b, quotes), descending)
                    }
                    "ducats" => {
                        direction((a.ducats as f64).total_cmp(&(b.ducats as f64)), descending)
                    }
                    "amount" => direction(
                        (if page == Page::Relics {
                            a.amount_owned
                        } else {
                            a.quantity
                        })
                        .cmp(
                            &(if page == Page::Relics {
                                b.amount_owned
                            } else {
                                b.quantity
                            }),
                        ),
                        descending,
                    ),
                    "value" => compare_known(
                        row_price(a, quotes)
                            .filter(|p| *p > 0.0)
                            .map(|p| (a.ducats as f64) / p),
                        row_price(b, quotes)
                            .filter(|p| *p > 0.0)
                            .map(|p| (b.ducats as f64) / p),
                        descending,
                    ),
                    _ => direction(a.name.as_str().cmp(b.name.as_str()), descending),
                },
            };
            comparison
                .then_with(|| a.name.as_str().cmp(b.name.as_str()))
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
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
            && let Some(i) = self.visible.iter().position(|&i| self.items[i].id == id)
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

struct Target<'a> {
    name: &'a str,
    market: &'a str,
    owned: u64,
    required: u64,
}

fn target(row: &ListingItem, index: Option<usize>, reward: bool) -> Target<'_> {
    if reward {
        if let Some(part) = index.and_then(|i| row.rewards.get(i)) {
            return Target {
                name: &part.name,
                market: part.slug.as_deref().unwrap_or(""),
                owned: part.owned,
                required: 0,
            };
        }
    } else if let Some(part) = index.and_then(|i| row.components.get(i)) {
        return Target {
            name: if part.full_name.is_empty() {
                &part.name
            } else {
                &part.full_name
            },
            market: part.market_name.as_deref().unwrap_or(""),
            owned: part.owned,
            required: part.required,
        };
    }
    Target {
        name: &row.name,
        market: row.market_name.as_deref().unwrap_or(""),
        owned: u64::from(row.owned),
        required: 0,
    }
}
fn compare_known(a: Option<f64>, b: Option<f64>, reverse: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => direction(a.total_cmp(&b), reverse),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}
fn row_price(row: &ListingItem, quotes: &HashMap<String, Quote>) -> Option<f64> {
    quotes
        .get(row.market_name.as_deref().unwrap_or(""))
        .and_then(|q| q.sell)
        .or_else(|| row.expected_platinum)
}
fn completion(row: &ListingItem) -> f64 {
    let parts = &row.components;
    if parts.is_empty() {
        return 0.0;
    }
    parts
        .iter()
        .filter(|p| (p.owned as f64) >= (p.required as f64))
        .count() as f64
        / parts.len() as f64
}
fn cost(row: &ListingItem, quotes: &HashMap<String, Quote>) -> Option<f64> {
    let mut cost = 0.0;
    for part in &row.components {
        let missing = ((part.required as f64) - (part.owned as f64)).max(0.0);
        if missing == 0.0 {
            continue;
        }
        let name = part.market_name.as_deref().unwrap_or("");
        if name.is_empty() {
            if part.market_required {
                return None;
            }
            continue;
        }
        cost += quotes.get(name)?.sell? * missing;
    }
    Some(cost)
}
fn score(row: &ListingItem, quotes: &HashMap<String, Quote>) -> Option<f64> {
    cost(row, quotes)
        .filter(|p| *p > 0.0)
        .map(|p| (row.potential_xp as f64) / p)
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
        self.prune_price_queue();
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
            .filter(|n| {
                refresh
                    || self
                        .quote_checked
                        .get(n)
                        .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
            })
            .collect::<Vec<_>>();
        self.quote_pending.extend(new.iter().cloned());
        if refresh {
            self.quote_queue.extend(new);
            self.pump_prices();
            return;
        }
        for chunk in new.chunks(100) {
            let key = format!("cache:{}", chunk[0]);
            self.queue(
                &key,
                json!({"op":"market_quote","items":chunk,"cache_only":true}),
            );
        }
    }
    fn wanted_prices(&self) -> HashSet<String> {
        if matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            self.listing().price_names().into_iter().collect()
        } else {
            HashSet::new()
        }
    }
    fn prune_price_queue(&mut self) {
        let wanted = self.wanted_prices();
        self.quote_queue.retain(|name| {
            if wanted.contains(name) {
                true
            } else {
                self.quote_pending.remove(name);
                false
            }
        });
    }
    fn pump_prices(&mut self) {
        self.prune_price_queue();
        if self.pending.keys().any(|k| k.starts_with("cache:")) {
            return;
        }
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
        let mut fresh = HashSet::new();
        if let Ok(data) = &result {
            for row in entries(&data["quotes"]) {
                let name = text(row, "item");
                if name.is_empty() {
                    continue;
                }
                let quote = &row["quote"];
                if quote.is_object() {
                    let age = quote["quoted_at"]
                        .as_i64()
                        .map(|at| unix_ms().saturating_sub(at));
                    if cached
                        && quote["stale"] != true
                        && age.is_some_and(|age| (0..60_000).contains(&age))
                    {
                        fresh.insert(name.to_owned());
                    }
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
            if cached {
                fresh.extend(
                    entries(&data["missing"])
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
            }
        }
        let names = entries(&request.body["items"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if cached {
            let wanted = self.wanted_prices();
            for name in names {
                if fresh.contains(&name) {
                    self.quote_pending.remove(&name);
                    self.quote_checked.insert(name, Instant::now());
                } else if wanted.contains(&name) {
                    self.quote_queue.push_back(name);
                } else {
                    self.quote_pending.remove(&name);
                }
            }
            if !self.pending.keys().any(|k| k.starts_with("cache:")) {
                self.resort(false);
                self.pump_prices();
            }
        } else {
            for name in names {
                self.quote_pending.remove(&name);
                self.quote_checked.insert(name.clone(), Instant::now());
                self.quotes.entry(name).or_default().resolved = true;
            }
            self.pump_prices();
            if !self
                .listing()
                .price_names()
                .iter()
                .any(|name| self.quote_pending.contains(name))
            {
                self.resort(false);
                if self.page == Page::Relics && self.relics.ready {
                    self.queue("relics:values",json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"}));
                }
            }
        }
        if self.quote_pending.is_empty() {
            self.last_prices = Instant::now();
        }
    }
    pub(super) fn listing_signal(&mut self, signal: Signal) {
        if self.listing_filter_signal(&signal) {
            return;
        }
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
                let target = target(row, component, self.page == Page::Relics);
                let name = target.name.to_owned();
                match action {
                    "relics" => self.navigation = Some((Page::Relics, name)),
                    "foundry" => self.navigation = Some((Page::Foundry, name)),
                    "market" => self.open_market(target.market.to_owned(), "sell"),
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
                self.preferences.toggle_favorite(&row.id);
            } else if matches!(action, "sell" | "buy" | "reward" | "component") {
                let target = target(
                    row,
                    component.filter(|_| matches!(action, "reward" | "component")),
                    action == "reward",
                );
                let name = target.market.to_owned();
                let owned = target.owned;
                let required = target.required;
                if name.is_empty() {
                    self.open_listing_menu(key);
                } else {
                    self.open_market(
                        name,
                        if action == "sell"
                            || (action == "component" && owned >= required && required > 0)
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
        if self.listing_filters_open() {
            return false;
        }
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
}

#[cfg(test)]
mod state_tests {
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
        listing.replace_fixture(snapshot(&["A", "B", "C", "D", "E", "F", "G", "H"]), &quotes);
        listing.offset = 2 * listing.grid.stride() + 7;
        listing
            .scroll
            .to(listing.offset as f32, false, Instant::now());
        listing.replace_fixture(
            snapshot(&["H", "G", "F", "E", "D", "C", "B", "AA", "A"]),
            &quotes,
        );
        assert_eq!(listing.offset, 3 * listing.grid.stride() + 7);
        assert_eq!(listing.anchor(), Some(("C".into(), 7)));
        listing.replace_fixture(snapshot(&["Z", "C"]), &quotes);
        assert_eq!(listing.visible, vec![1, 0]);
        assert!(listing.offset <= listing.scroll.maximum() as u32);
        listing.replace_fixture(snapshot(&[]), &quotes);
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
        ].into_iter().map(|v|serde_json::from_value(v).unwrap()).collect();
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
        let relic = serde_json::from_value(
            json!({"name":"Lith A1","amount_owned":0,"rewards":[{"name":"Ash Prime Blueprint"}]}),
        )
        .unwrap();
        assert!(!listing.accepts(&relic));
        listing.owned = false;
        assert!(listing.accepts(&relic));
    }
    #[test]
    fn missing_required_market_price_is_not_free() {
        let row =
            serde_json::from_value(json!({"components":[{"required":1,"market_required":true}]}))
                .unwrap();
        assert_eq!(cost(&row, &HashMap::new()), None);
    }
}

mod card_layout;
mod grid_view;
mod inventory;
mod listing_model;
mod listing_view;
mod mastery;
mod overlays;
mod relics;
mod summary;
#[cfg(test)]
mod tests;
mod toolbar;

use crate::{
    presentation::widgets::Fonts, presentation::widgets::fill, presentation::widgets::fitted_label,
    presentation::widgets::key, presentation::widgets::place, presentation::widgets::slot,
};
use listing_model::Model;
use retinue_render::{Bounds, Key};
use retinue_ui::{layout::grid::UniformGrid, prelude::*, state::KeyedOrder, timeline::FrameTime};

const SEARCH: Key = Key::new("listing.search", 0);
const SORT: Key = Key::new("listing.sort", 0);
const DIRECTION: Key = Key::new("listing.direction", 0);
const DIRECTION_ICON: Key = Key::new("listing.direction", 1);
const FILTER: Key = Key::new("listing.filters.button", 0);
const TOOLBAR: Key = Key::new("listing.toolbar", 0);
const VIEWPORT: Key = Key::new("listing.viewport", 0);
const CONTENT: Key = Key::new("listing.content", 0);
const SCROLL: Key = Key::new("listing.scroll", 0);
const RETRY: Key = Key::new("listing.retry", 0);

#[derive(Clone, PartialEq)]
struct Metrics {
    page: Page,
    width: f32,
    height: f32,
    scale: f32,
    labels: Vec<f32>,
    summary_bottom: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            page: Page::Inventory,
            width: 0.0,
            height: 0.0,
            scale: 1.0,
            labels: Vec::new(),
            summary_bottom: 0.0,
        }
    }
}

impl Metrics {
    fn px(&self, value: f32) -> u32 {
        (value.max(0.0) * self.scale).round() as u32
    }
    fn bounds(&self, x: f32, y: f32, w: f32, h: f32) -> Bounds {
        Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
    }
    fn local(&self, parent: Bounds, child: Bounds) -> Bounds {
        let global = self.bounds(
            parent.x + child.x,
            parent.y + child.y,
            child.width,
            child.height,
        );
        Bounds::new(
            global.x - parent.x,
            global.y - parent.y,
            global.width,
            global.height,
        )
    }
    fn wrapped(&self) -> bool {
        self.width
            - (12.0
                + self.labels.len() as f32 * (self.tab_style().0 + 5.0)
                + self.labels.iter().copied().fold(0.0, f32::max)
                + 5.0)
            - self.tools_width()
            < 132.0
    }
    fn tab_style(&self) -> (f32, f32, f32) {
        if self.page == Page::Relics {
            (40.0, 32.0, 24.0)
        } else {
            (38.0, 34.0, 20.0)
        }
    }
    fn tools_width(&self) -> f32 {
        match self.page {
            Page::Inventory => 247.0,
            Page::Relics => 272.0,
            _ => 44.0,
        }
    }
    fn tab_top(&self) -> f32 {
        if self.page == Page::Mastery {
            self.summary_bottom + 23.0
        } else {
            12.0
        }
    }
    fn tools_y(&self) -> f32 {
        self.tab_top() + if self.wrapped() { 44.0 } else { 0.0 }
    }
    fn modes_y(&self) -> f32 {
        self.tools_y() + 42.0 + if self.width < 610.0 { 37.0 } else { 0.0 }
    }
    fn grid_y(&self) -> f32 {
        if self.page == Page::Mastery {
            self.modes_y() + 44.0
        } else {
            self.tools_y() + 42.0
        }
    }
    fn scroll_area(&self) -> Bounds {
        self.bounds(
            10.0,
            self.grid_y(),
            self.width - 20.0,
            self.height - self.grid_y(),
        )
    }
    fn viewport(&self) -> Bounds {
        let mut r = self.scroll_area();
        r.width = (r.width - 12.0).max(0.0);
        r
    }
    fn pixel_rect(&self, b: Bounds) -> Rect {
        Rect {
            x: self.px(b.x),
            y: self.px(b.y),
            width: self.px(b.width),
            height: self.px(b.height),
        }
    }
    fn grid(&self) -> Grid {
        Grid::new(
            self.px(self.viewport().width),
            self.px(if self.page == Page::Mastery {
                370.0
            } else {
                420.0
            }),
            self.px(match self.page {
                Page::Inventory => 130.0,
                Page::Mastery => 91.0,
                _ => 100.0,
            }),
            self.px(8.0),
        )
    }
    fn uniform_grid(&self) -> UniformGrid {
        let g = self.grid();
        UniformGrid::new(
            g.columns,
            LayoutSize::new(
                (g.width as f32 / self.scale).max(1.0),
                g.height as f32 / self.scale,
            ),
            LayoutSize::new(g.gap as f32 / self.scale, g.gap as f32 / self.scale),
            Insets::default(),
        )
        .expect("listing grid geometry")
    }
}

#[derive(Clone, PartialEq)]
struct Cell {
    index: usize,
    item: std::rc::Rc<ListingItem>,
    quote: Quote,
    cost: Option<f64>,
    favorite: bool,
    hovered: bool,
}

#[derive(Clone, PartialEq, Default)]
struct CardContext {
    owned_only: bool,
    query: String,
    prices_ready: bool,
}

pub(in crate::presentation) struct ListingController {
    page: Page,
    model: Model,
    summary: summary::Summary,
    overlays: overlays::Overlays,
    revision: Option<u64>,
    pub installed: bool,
}

impl ListingController {
    pub fn new(page: Page) -> Self {
        Self {
            page,
            model: Model::default(),
            summary: summary::Summary::new(),
            overlays: overlays::Overlays::new(),
            revision: None,
            installed: false,
        }
    }

    fn view(&self, ui: &ControlHost) -> Element {
        let fonts = Fonts::new(ui);
        listing_view::ListingPageView {
            model: self.model.clone(),
            summary: self.summary.view(self.model.metrics.clone(), fonts.clone()),
            overlays: self.overlays.view(fonts.regular.clone()),
            fonts,
        }
        .into_element()
    }

    fn sync(
        &mut self,
        ui: &ControlHost,
        listing: &mut Listing,
        quotes: &HashMap<String, Quote>,
        pending: &std::collections::HashSet<String>,
        preferences: &crate::domain::preferences::Preferences,
        now: Instant,
    ) {
        let summary_bottom = if self.page == Page::Mastery {
            self.summary.sync(ui, listing, pending, preferences)
        } else {
            0.0
        };
        let m = Metrics {
            page: self.page,
            width: ui.size.0 as f32 / ui.scale,
            height: ui.size.1 as f32 / ui.scale,
            scale: ui.scale,
            labels: categories(self.page)
                .iter()
                .map(|(_, label, _)| ui.measure(label, 13.0, false))
                .collect(),
            summary_bottom,
        };
        self.model.metrics.set(m.clone());
        listing.sync_category(false, now);
        self.model.weights.set(
            (0..categories(self.page).len())
                .map(|i| {
                    listing
                        .category_motion
                        .weight(category_key(i), listing.category_sampled_at)
                })
                .collect(),
        );
        self.model.mode.set(listing.mode.clone());
        self.model.context.set(CardContext {
            owned_only: listing.owned,
            query: listing.search.to_lowercase(),
            prices_ready: pending.is_empty(),
        });
        if self.revision != Some(listing.order_revision) {
            let mut seen = HashMap::new();
            let keys = listing.visible.iter().map(|&index| {
                let row = &listing.items[index];
                let identity = (
                    if row.id.is_empty() {
                        &row.name
                    } else {
                        &row.id
                    },
                    row.rank,
                    &row.group,
                );
                let occurrence = seen.entry(identity).or_insert(0_u32);
                let id = key("listing.item", (self.page as u8, identity, *occurrence));
                *occurrence += 1;
                id
            });
            self.model
                .order
                .set(KeyedOrder::new(keys).expect("unique listing identities"));
            self.revision = Some(listing.order_revision);
        }
        listing.viewport = m.pixel_rect(m.viewport());
        listing.grid = m.grid();
        listing.scroll.resize(
            listing.grid.content_height(listing.visible.len()) as f32,
            listing.viewport.height as f32,
            now,
        );
        listing.offset = listing.scroll.offset(now).round() as u32;
        let order = self.model.order.get();
        let offset = listing.offset as f32 / ui.scale;
        let window = m
            .uniform_grid()
            .visible(order.len(), offset, m.viewport().height, 0)
            .expect("listing visible range");
        self.model.offset.set(offset);
        let hovered = ui
            .hover
            .as_deref()
            .and_then(|key| key.strip_prefix("list:item:"))
            .and_then(|key| key.split(':').next())
            .and_then(|s| s.parse::<usize>().ok());
        self.model
            .cells
            .replace(window.map(|position| {
                let index = listing.visible[position];
                let source = &listing.items[index];
                let item = self
                    .model
                    .cells
                    .item(order.keys()[position])
                    .and_then(|cell| {
                        let cell = cell.read();
                        (cell.item.as_ref() == source).then(|| cell.item.clone())
                    })
                    .unwrap_or_else(|| std::rc::Rc::new(source.clone()));
                let quote = quotes
                    .get(item.market_name.as_deref().unwrap_or(""))
                    .cloned()
                    .unwrap_or_default();
                let cost = if self.page == Page::Mastery {
                    cost(&item, quotes)
                } else {
                    None
                };
                let favorite = self.page == Page::Mastery && preferences.is_favorite(&item.id);
                (
                    order.keys()[position],
                    Cell {
                        index,
                        item,
                        quote,
                        cost,
                        favorite,
                        hovered: hovered == Some(index),
                    },
                )
            }))
            .expect("mounted listing cells");
        self.model.message.set((
            listing.error.clone().unwrap_or_else(|| {
                if listing.ready {
                    "No matching items"
                } else {
                    "Loading player data..."
                }
                .into()
            }),
            listing.error.is_some(),
        ));
    }

    fn bind(&self, ui: &mut ControlHost, listing: &Listing) {
        toolbar::bind(self, ui, listing);
        if self.page == Page::Mastery {
            self.summary.bind(ui);
        }
        ui.view_control(RETRY, |ui, r| {
            ui.button("list:retry", r, "Retry", false, true)
        });
        for (id, name) in [(TOOLBAR, "page.toolbar"), (VIEWPORT, "page.grid")] {
            if let Some(bounds) = ui.view_bounds(id) {
                ui.region(name, bounds);
            }
        }
        for &id in self.model.cells.order().keys() {
            let source = self.model.cells.item(id).unwrap();
            let cell = source.read();
            if let Some(r) = ui.view_bounds(id) {
                ui.first_region("page.card", r);
            }
            match self.page {
                Page::Inventory => inventory::bind(ui, id, &cell),
                Page::Mastery => mastery::bind(ui, id, &cell),
                Page::Relics => relics::bind(ui, id, &cell),
                _ => unreachable!(),
            }
        }
        ui.view_control(SCROLL, |ui, r| {
            ui.scrollbar(
                "list:scroll",
                r,
                listing.scroll.maximum(),
                listing.offset as f32,
            )
        });
        self.overlays.bind(ui, listing);
    }
}

impl Pages {
    pub(in crate::presentation) fn layout_listing(&mut self) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        let (view, listing) = match self.page {
            Page::Inventory => (&mut self.inventory_view, &mut self.inventory),
            Page::Mastery => (&mut self.mastery_view, &mut self.mastery),
            Page::Relics => (&mut self.relics_view, &mut self.relics),
            _ => unreachable!(),
        };
        view.sync(
            &self.ui,
            listing,
            &self.quotes,
            &self.quote_pending,
            &self.preferences,
            Instant::now(),
        );
        if !view.installed {
            self.ui.install_view(view.view(&self.ui));
            view.installed = true;
        }
        view.overlays
            .sync(&self.ui, listing, &view.model.metrics.get());
        #[cfg(all(test, feature = "profiling"))]
        let synced = Instant::now();
        self.ui.update_view(FrameTime::now());
        #[cfg(all(test, feature = "profiling"))]
        let projected = Instant::now();
        view.bind(&mut self.ui, listing);
        #[cfg(all(test, feature = "profiling"))]
        {
            self.listing_profile = [synced - started, projected - synced, projected.elapsed()];
        }
    }
}
