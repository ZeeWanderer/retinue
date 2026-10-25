mod component;
pub mod filters;
pub mod images;
mod input;
pub mod model;
mod search;
pub use search::Transfer;
#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod tests;
mod toolbar;
mod view;
pub mod worker;

use crate::settings::rgb;
use filters::{Filter, FilterPanel, Filters};
use model::{Asset, Card, Descriptor, Snapshot};
use retinue_ui::{
    Rect,
    damage::Damage,
    desktop::{
        accesskit::{self, Node, NodeId, Role, TreeInfo, TreeUpdate},
        winit::keyboard::{Key as InputKey, ModifiersState, NamedKey},
    },
    grid::Grid,
    line_edit::{Colors, LineEdit},
    painter::{CachedFont, GlyphCache, Painter, RasterImage, TextLine, text_width},
    resources::Resource,
    scroll::Scroll,
    thumbnails::{Key, Request, WorkStats},
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::style::{CARD, CARD_HOVER, MUTED, PANEL, TEXT};
#[cfg(test)]
use crate::style::{COMPONENT_GLOW, COMPONENT_READY};
const CATEGORIES: [&str; 8] = [
    "All",
    "Warframe",
    "Primary",
    "Secondary",
    "Melee",
    "Modular",
    "Arch",
    "Companion",
];
const GROUPS: [&str; 8] = [
    "all",
    "warframe",
    "primary",
    "secondary",
    "melee",
    "modular",
    "arch",
    "companion",
];

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Action {
    Category(usize),
    Search,
    ClearSearch,
    Filters,
    FilterChoice(Filter, bool),
    ClearFilters,
    CloseFilters,
    FilterScroll,
    Refresh,
    Favorite(usize),
    Component(usize, usize),
    ComponentRelic(usize, usize),
    ComponentFavorite,
    ComponentSearch,
    ComponentMarket,
    ComponentRelics,
    CloseComponent,
    Scroll,
}

pub enum Destination {
    Market { item: String, buyers: bool },
    Relics(String),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Badge {
    Mastered,
    Favorite,
    FavoriteOn,
    Pending,
    Subsumed,
    Vaulted,
    Unvaulted,
    Relic,
    Category(usize),
    Refresh,
    Filters,
    Close,
    Search,
    ClearSearch,
}

pub struct Foundry {
    pub snapshot: Snapshot,
    pub ready: bool,
    pub error: Option<String>,
    pub input_error: Option<String>,
    pub clipboard_request: Option<(Transfer, retinue_ui::desktop::clipboard::Operation)>,
    pub destination: Option<Destination>,
    pub viewport: Rect,
    pub grid: Grid,
    pub offset: u32,
    pub refresh: u64,
    pub focus: Option<Action>,
    pub image_errors: Vec<String>,
    pub cache_warnings: Vec<String>,
    pub image_work: WorkStats,
    pub asset_errors: HashMap<String, String>,
    pub cache_stats: (
        retinue_ui::resources::CacheStats,
        retinue_ui::resources::CacheStats,
    ),
    size: (u32, u32),
    scale: f32,
    category: usize,
    category_motion: toolbar::CategoryMotion,
    filters: Filters,
    filter_panel: FilterPanel,
    component_panel: component::ComponentPanel,
    filtered: Vec<usize>,
    preferences: crate::preferences::Preferences,
    scroll: Scroll,
    drag: Option<f64>,
    pressed: Option<Action>,
    pointer: (f64, f64),
    hover: Option<String>,
    hover_since: Instant,
    tooltip: bool,
    search: RefCell<LineEdit>,
    search_motion: search::SearchMotion,
    regular: CachedFont,
    icons: HashMap<Badge, Request>,
    descriptors: HashMap<String, Descriptor>,
    previous_descriptors: HashMap<String, Descriptor>,
    glyphs: GlyphCache,
    images: HashMap<Key, Resource<RasterImage>>,
    component_glow: Option<RasterImage>,
}

impl Foundry {
    pub fn new(preferences: crate::preferences::Preferences) -> Result<Self, String> {
        let bytes = include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let glyphs = GlyphCache::new(8 * 1024 * 1024);
        Ok(Self {
            snapshot: Snapshot::default(),
            ready: false,
            error: None,
            input_error: None,
            clipboard_request: None,
            destination: None,
            viewport: Rect::default(),
            grid: Grid::new(600, 270, 142, 8),
            offset: 0,
            refresh: 0,
            focus: None,
            image_errors: Vec::new(),
            cache_warnings: Vec::new(),
            image_work: WorkStats::default(),
            asset_errors: HashMap::new(),
            cache_stats: Default::default(),
            size: (0, 0),
            scale: 0.0,
            category: 1,
            category_motion: toolbar::CategoryMotion::new(1),
            filters: Filters::default(),
            filter_panel: FilterPanel::default(),
            component_panel: component::ComponentPanel::default(),
            filtered: Vec::new(),
            preferences,
            scroll: Scroll::default(),
            drag: None,
            pressed: None,
            pointer: (-1.0, -1.0),
            hover: None,
            hover_since: Instant::now(),
            tooltip: false,
            search: RefCell::new(LineEdit::new(bytes, "Roboto")),
            search_motion: search::SearchMotion::default(),
            regular: CachedFont::from_bytes(bytes, Default::default(), &glyphs)?,
            glyphs,
            icons: HashMap::new(),
            descriptors: HashMap::new(),
            previous_descriptors: HashMap::new(),
            images: HashMap::new(),
            component_glow: None,
        })
    }
    fn px(&self, v: f32) -> u32 {
        (v * self.scale).round().max(0.0) as u32
    }
    pub fn count(&self) -> usize {
        self.filtered.len()
    }
    pub fn maximum(&self) -> f32 {
        self.scroll.maximum()
    }

    pub fn resize(&mut self, size: (u32, u32), scale: f32) {
        if self.size == size && self.scale == scale {
            return;
        }
        let anchor = self.anchor();
        if self.scale != scale {
            self.scale = scale;
            self.prepare_icons();
            let diameter = self.px(39.0);
            self.component_glow = match crate::style::component_glow(diameter, scale) {
                Ok(image) => Some(image),
                Err(error) => {
                    self.image_errors.push(format!("component glow: {error}"));
                    None
                }
            };
        }
        self.size = size;
        self.layout_search(Instant::now(), false);
        let top = self
            .controls()
            .iter()
            .map(|(_, r)| r.y + r.height)
            .max()
            .unwrap_or(0)
            + self.px(30.0);
        self.viewport = Rect {
            x: self.px(10.0),
            y: top,
            width: size.0.saturating_sub(self.px(28.0)),
            height: size.1.saturating_sub(top),
        };
        self.grid = Grid::new(
            self.viewport.width,
            self.px(270.0),
            self.px(142.0),
            self.px(8.0),
        );
        self.restore_anchor(anchor);
        self.layout_filters();
        self.layout_component();
    }

    fn anchor(&self) -> Option<(String, f32)> {
        let row = self.offset / self.grid.stride();
        self.filtered
            .get(row as usize * self.grid.columns)
            .map(|&i| {
                (
                    self.snapshot.items[i].id.clone(),
                    (self.offset % self.grid.stride()) as f32 / self.grid.stride() as f32,
                )
            })
    }
    fn restore_anchor(&mut self, anchor: Option<(String, f32)>) {
        let now = Instant::now();
        self.scroll.resize(
            self.grid.content_height(self.count()) as f32,
            self.viewport.height as f32,
            now,
        );
        if let Some((id, fraction)) = anchor {
            if let Some(i) = self
                .filtered
                .iter()
                .position(|&i| self.snapshot.items[i].id == id)
            {
                self.scroll.to(
                    (i / self.grid.columns) as f32 * self.grid.stride() as f32
                        + fraction * self.grid.stride() as f32,
                    false,
                    now,
                );
            }
        }
        self.offset = self.scroll.offset(now).round() as u32;
    }
    pub fn update(&mut self, snapshot: Snapshot) -> Damage {
        if self.ready
            && self.error.is_none()
            && self.snapshot.items == snapshot.items
            && self.snapshot.profile == snapshot.profile
        {
            self.snapshot.revision = snapshot.revision;
            return Damage::None;
        }
        let anchor = self.anchor();
        self.pressed = None;
        let old = std::mem::take(&mut self.snapshot);
        let old_filtered = self.filtered.clone();
        let old_offset = self.offset;
        let full = !self.ready
            || self.error.is_some()
            || self.tooltip
            || self.popup_open()
            || old.profile != snapshot.profile;
        self.snapshot = snapshot;
        self.ready = true;
        self.error = None;
        self.filter(false);
        self.restore_anchor(anchor);
        self.remap_item_focus(&old);
        self.layout_component();
        if full
            || self.filtered != old_filtered
            || self.offset != old_offset
            || old.items.len() != self.snapshot.items.len()
        {
            return Damage::Full;
        }
        self.visible(0)
            .filter(|&i| old.items[self.filtered[i]] != self.snapshot.items[self.filtered[i]])
            .fold(Damage::None, |damage, i| {
                damage.union(Damage::rect(self.screen_card(i)))
            })
    }

    fn screen_card(&self, index: usize) -> Rect {
        let rect = self.card_rect(index);
        let top = rect.y as i64 - self.offset as i64;
        let bottom = top + rect.height as i64;
        let y = top.max(self.viewport.y as i64) as u32;
        Rect {
            x: rect.x,
            y,
            width: rect.width,
            height: (bottom.min((self.viewport.y + self.viewport.height) as i64) - y as i64).max(0)
                as u32,
        }
    }
    pub fn assets(&mut self, assets: Vec<Descriptor>) {
        for asset in assets {
            self.asset_errors.remove(&asset.id);
            if let Some(old) = self
                .descriptors
                .get(&asset.id)
                .filter(|old| old.digest != asset.digest)
            {
                if self.images.keys().any(|key| {
                    retinue_ui::thumbnails::parse_digest(&old.digest) == Some(key.digest)
                }) {
                    self.previous_descriptors
                        .insert(asset.id.clone(), old.clone());
                }
            }
            self.descriptors.insert(asset.id.clone(), asset);
        }
    }
    pub fn assets_ready(&self) -> bool {
        self.desired_assets()
            .iter()
            .all(|a| self.descriptors.contains_key(&a.id) || self.asset_errors.contains_key(&a.id))
    }
    fn filter(&mut self, reset: bool) {
        let query = self.search.borrow().text().to_lowercase();
        self.filtered = self
            .snapshot
            .items
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                (self.category == 0 || c.group == GROUPS[self.category])
                    && self.filters.matches(
                        c,
                        self.snapshot.profile["player_level"].as_u64(),
                        &self.preferences,
                    )
                    && (c.name.to_lowercase().contains(&query)
                        || c.components
                            .iter()
                            .any(|p| p.name.to_lowercase().contains(&query)))
            })
            .map(|(i, _)| i)
            .collect();
        self.scroll.resize(
            self.grid.content_height(self.count()) as f32,
            self.viewport.height as f32,
            Instant::now(),
        );
        if reset {
            self.scroll.to(0.0, false, Instant::now());
            self.offset = 0;
        }
        self.tooltip = false;
        self.hover = None;
    }
    pub fn set_search(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(256).collect();
        self.search.borrow_mut().set_text(&text, false);
        self.layout_search(Instant::now(), false);
        self.filter(true);
    }
    pub fn set_category(&mut self, category: usize) {
        self.category = category.min(7);
        self.category_motion
            .select(self.category, Instant::now(), false);
        self.filter(true);
        let size = self.size;
        self.size = (0, 0);
        self.resize(size, self.scale);
    }
    fn visible(&self, overscan: u32) -> std::ops::Range<usize> {
        self.grid
            .visible(self.count(), self.offset, self.viewport.height, overscan)
    }

    pub fn desired_assets(&self) -> Vec<Asset> {
        let mut seen = HashSet::new();
        self.prioritized_rows()
            .flat_map(|index| {
                let c = &self.snapshot.items[self.filtered[index]];
                c.asset
                    .iter()
                    .chain(c.components.iter().filter_map(|p| p.asset.as_ref()))
            })
            .filter(|a| seen.insert(a.id.clone()))
            .cloned()
            .chain(self.rank_asset())
            .collect()
    }
    pub fn rank_asset(&self) -> Option<Asset> {
        serde_json::from_value(self.snapshot.profile["rank_asset"].clone()).ok()
    }
    pub fn rank_image(&self) -> Option<&Resource<RasterImage>> {
        self.displayed_image(self.rank_asset().as_ref(), self.px(50.0))
    }
    pub fn rank_image_key(&self) -> Option<Key> {
        self.request(self.rank_asset().as_ref(), self.px(50.0))
            .map(|r| r.key)
    }
    fn request(&self, asset: Option<&Asset>, size: u32) -> Option<Request> {
        let asset = asset?;
        Some(
            self.descriptors
                .get(&asset.id)?
                .request(size)?
                .persistent(&asset.source, &asset.image_name),
        )
    }

    fn displayed_image(&self, asset: Option<&Asset>, size: u32) -> Option<&Resource<RasterImage>> {
        let asset = asset?;
        self.descriptors
            .get(&asset.id)
            .and_then(|descriptor| descriptor.key(size))
            .and_then(|key| self.images.get(&key))
            .or_else(|| {
                self.previous_descriptors
                    .get(&asset.id)?
                    .key(size)
                    .and_then(|key| self.images.get(&key))
            })
    }
    pub fn image_requests(&self) -> Vec<Request> {
        let mut icons = self.icons.iter().collect::<Vec<_>>();
        icons.sort_by_key(|(id, _)| **id);
        let mut requests = icons
            .into_iter()
            .map(|(_, request)| request.clone())
            .collect::<Vec<_>>();
        if let Some(request) = self.request(self.rank_asset().as_ref(), self.px(50.0)) {
            requests.push(request);
        }
        for index in self.prioritized_rows() {
            let card = &self.snapshot.items[self.filtered[index]];
            if let Some(request) = self.request(
                card.asset.as_ref(),
                self.px(if card.group == "warframe" {
                    105.0
                } else {
                    86.0
                }),
            ) {
                requests.push(request);
            }
            for part in card.components.iter().take(6) {
                if let Some(request) = self.request(part.asset.as_ref(), self.px(35.0)) {
                    requests.push(request);
                }
            }
        }
        let mut seen = HashSet::new();
        requests.retain(|r| seen.insert(r.key));
        requests
    }
    fn prioritized_rows(&self) -> impl Iterator<Item = usize> + '_ {
        let visible = self.visible(0);
        visible
            .clone()
            .chain(self.visible(1).filter(move |i| !visible.contains(i)))
    }
    pub fn accept_images(
        &mut self,
        images: Vec<(Key, Resource<RasterImage>)>,
        errors: Vec<String>,
    ) -> Damage {
        let mut wanted = self
            .image_requests()
            .iter()
            .map(|r| r.key)
            .collect::<HashSet<_>>();
        let changed = images
            .iter()
            .filter(|(key, _)| wanted.contains(key) && !self.images.contains_key(key))
            .map(|(key, _)| *key)
            .collect::<HashSet<_>>();
        self.images
            .extend(images.into_iter().filter(|(k, _)| wanted.contains(k)));
        for asset in self.desired_assets() {
            if let (Some(current), Some(previous)) = (
                self.descriptors.get(&asset.id),
                self.previous_descriptors.get(&asset.id),
            ) {
                for size in [self.px(35.0), self.px(50.0), self.px(86.0), self.px(105.0)] {
                    if current.request(size).is_some_and(|r| {
                        wanted.contains(&r.key) && !self.images.contains_key(&r.key)
                    }) {
                        if let Some(r) = previous.request(size) {
                            wanted.insert(r.key);
                        }
                    }
                }
            }
        }
        self.images.retain(|k, _| wanted.contains(k));
        for error in errors {
            if !self.image_errors.contains(&error) {
                self.image_errors.push(error);
            }
        }
        if self.icons.values().any(|r| changed.contains(&r.key)) {
            return Damage::Full;
        }
        self.visible(0)
            .filter(|&index| {
                let card = &self.snapshot.items[self.filtered[index]];
                let main = self.request(
                    card.asset.as_ref(),
                    self.px(if card.group == "warframe" {
                        105.0
                    } else {
                        86.0
                    }),
                );
                main.is_some_and(|r| changed.contains(&r.key))
                    || card.components.iter().any(|part| {
                        self.request(part.asset.as_ref(), self.px(35.0))
                            .is_some_and(|r| changed.contains(&r.key))
                    })
            })
            .fold(Damage::None, |damage, index| {
                damage.union(Damage::rect(self.screen_card(index)))
            })
    }
    pub fn clear_images(&mut self) {
        self.images.clear();
        self.glyphs.clear();
    }
    pub fn prepared_count(&self) -> usize {
        self.images.len()
    }
    pub fn memory(&self) -> (usize, usize) {
        let (a, b) = self.cache_stats;
        let glyphs = self.glyphs.stats();
        let decorations = self
            .component_glow
            .as_ref()
            .map_or(0, RasterImage::byte_len);
        (
            a.live_bytes + b.live_bytes + glyphs.live_bytes + decorations,
            a.budget_bytes + b.budget_bytes + glyphs.budget_bytes + decorations,
        )
    }

    fn prepare_icons(&mut self) {
        macro_rules! ui {
            ($name:literal) => {
                include_bytes!(concat!("../resources/ui/", $name, ".png")) as &'static [u8]
            };
        }
        macro_rules! category {
            ($name:literal) => {
                include_bytes!(concat!("../resources/categories/", $name, ".png"))
                    as &'static [u8]
            };
        }
        let specs = [
            (
                Badge::Mastered,
                include_bytes!("../../../assets/warframe/ui/mastered.png") as &[u8],
                24.0,
                None,
            ),
            (Badge::Favorite, ui!("favorite"), 16.0, None),
            (Badge::FavoriteOn, ui!("favorite_active"), 16.0, None),
            (Badge::Pending, ui!("pending"), 11.0, None),
            (
                Badge::Subsumed,
                include_bytes!("../../../assets/warframe/ui/helminth.png"),
                17.0,
                None,
            ),
            (
                Badge::Vaulted,
                include_bytes!("../../wfcompanion/assets/vaulted.png"),
                18.0,
                Some([38, 51, 106, 80]),
            ),
            (
                Badge::Unvaulted,
                include_bytes!("../../wfcompanion/assets/unvaulted.png"),
                17.0,
                Some([29, 29, 122, 130]),
            ),
            (Badge::Relic, ui!("fissure"), 17.5, None),
            (Badge::Refresh, ui!("refresh"), 19.0, None),
            (Badge::Filters, ui!("filter"), 17.0, None),
            (Badge::Close, ui!("window-close"), 20.0, None),
            (Badge::Search, ui!("search"), 17.0, None),
            (Badge::ClearSearch, ui!("window-close"), 16.0, None),
            (Badge::Category(0), category!("all"), 22.0, None),
            (Badge::Category(1), category!("warframe"), 22.0, None),
            (Badge::Category(2), category!("primary"), 22.0, None),
            (Badge::Category(3), category!("secondary"), 22.0, None),
            (Badge::Category(4), category!("melee"), 22.0, None),
            (Badge::Category(5), category!("modular"), 22.0, None),
            (Badge::Category(6), category!("arch"), 22.0, None),
            (Badge::Category(7), category!("companion"), 22.0, None),
        ];
        self.icons = specs
            .into_iter()
            .map(|(id, bytes, size, crop)| (id, Request::embedded(bytes, self.px(size), crop)))
            .collect();
    }

    fn scrollbar(&self) -> Rect {
        let height = (self.viewport.height as f32 * self.viewport.height as f32
            / (self.scroll.maximum() + self.viewport.height as f32).max(1.0))
        .round() as u32;
        let height = height.max(self.px(24.0)).min(self.viewport.height);
        let travel = self.viewport.height - height;
        Rect {
            x: self.size.0.saturating_sub(self.px(12.0)),
            y: self.viewport.y
                + (travel as f32 * self.offset as f32 / self.scroll.maximum().max(1.0)).round()
                    as u32,
            width: self.px(6.0),
            height,
        }
    }
    fn card_rect(&self, index: usize) -> Rect {
        let (x, y) = self.grid.origin(index);
        Rect {
            x: self.viewport.x + x,
            y: self.viewport.y + y,
            width: self.grid.width,
            height: self.grid.height,
        }
    }
    fn title_rects(&self, card: &Card) -> (Rect, Rect, Rect) {
        let pad = self.px(10.0);
        let fixed = self.px(if card.mastered { 58.0 } else { 26.0 });
        let room = self.grid.width.saturating_sub(2 * pad + self.px(48.0));
        let text = (text_width(&self.regular, &card.name, 20.0 * self.scale).ceil() as u32)
            .min(room.saturating_sub(fixed));
        let x = self.grid.width.saturating_sub(text + fixed) / 2;
        (
            Rect {
                x,
                y: self.px(15.0),
                width: self.px(16.0),
                height: self.px(16.0),
            },
            Rect {
                x: x + self.px(20.0),
                y: self.px(10.0),
                width: text,
                height: self.px(27.0),
            },
            Rect {
                x: x + self.px(28.0) + text,
                y: self.px(11.0),
                width: self.px(24.0),
                height: self.px(24.0),
            },
        )
    }
    fn parts(&self, count: usize) -> Vec<Rect> {
        let count = count.min(6);
        let size = self.px(39.0);
        let gap = self.px(8.0);
        let width = 3 * size + 2 * gap;
        let left = self.grid.width.saturating_sub(width + self.px(10.0));
        let rows = count.div_ceil(3) as u32;
        let total = rows * size + rows.saturating_sub(1) * gap;
        let top = self.px(43.0) + (self.px(86.0).saturating_sub(total)) / 2;
        (0..count)
            .map(|i| {
                let n = (count - i / 3 * 3).min(3) as u32;
                Rect {
                    x: left
                        + (width - (n * size + (n - 1) * gap)) / 2
                        + (i % 3) as u32 * (size + gap),
                    y: top + (i / 3) as u32 * (size + gap),
                    width: size,
                    height: size,
                }
            })
            .collect()
    }
    fn relic_badge(&self, component: Rect) -> Rect {
        let diameter = self.px(21.0);
        Rect {
            x: component.x.saturating_sub(self.px(2.0)),
            y: component.y + component.height + self.px(3.0) - diameter,
            width: diameter,
            height: diameter,
        }
    }
    fn count_badge(&self, component: Rect, count: &str, has_relic: bool) -> Rect {
        let width =
            text_width(&self.regular, count, 12.5 * self.scale).ceil() as u32 + self.px(8.0);
        let width = if has_relic {
            width.min(self.px(27.0))
        } else {
            width
        };
        let width = width.min(component.x + component.width + self.px(3.0));
        Rect {
            x: component.x + component.width + self.px(3.0) - width,
            y: component.y + component.height + self.px(2.0) - self.px(18.0),
            width,
            height: self.px(18.0),
        }
    }
    fn hovered_card(&self) -> Option<usize> {
        if self.popup_open() {
            return None;
        }
        self.visible(0)
            .find(|&i| self.screen_card(i).contains(self.pointer.0, self.pointer.1))
    }
    pub fn capture_bounds(&self, target: &str) -> Result<Rect, String> {
        match target {
            "foundry.component" if self.component_open() => Ok(self.component_panel.popup.bounds),
            "foundry.filters" if self.filters_open() => Ok(self.filter_panel.popup.bounds),
            "foundry.toolbar" => Ok(Rect {
                x: 0,
                y: 0,
                width: self.size.0,
                height: self.viewport.y,
            }),
            "foundry.grid" => Ok(self.viewport),
            "foundry.card" if self.count() > 0 => Ok(Rect {
                x: self.viewport.x,
                y: self.viewport.y,
                width: self.grid.width,
                height: self.grid.height.min(self.viewport.height),
            }),
            _ => Err("Unknown or empty Foundry capture target".into()),
        }
    }
    pub fn layout_report(&self) -> serde_json::Value {
        use serde_json::json;
        let logical = |r: Rect| {
            json!({"x":r.x as f32/self.scale,"y":r.y as f32/self.scale,
            "width":r.width as f32/self.scale,"height":r.height as f32/self.scale})
        };
        let mut regions = serde_json::Map::new();
        for key in ["foundry.toolbar", "foundry.grid", "foundry.card"] {
            if let Ok(r) = self.capture_bounds(key) {
                regions.insert(key.into(), logical(r));
            }
        }
        if let Some(i) = self.visible(0).next() {
            let card = self.screen_card(i);
            if let Some(part) = self
                .parts(self.snapshot.items[self.filtered[i]].components.len())
                .first()
            {
                regions.insert(
                    "foundry.part".into(),
                    logical(Rect {
                        x: card.x + part.x,
                        y: card.y + part.y,
                        ..*part
                    }),
                );
            }
        }
        json!({"units":"logical_pixels","scale":self.scale,"regions":regions})
    }
}
