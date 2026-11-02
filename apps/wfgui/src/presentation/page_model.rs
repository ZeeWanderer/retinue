use super::*;

pub struct Pages {
    pub ui: ControlHost,
    pub rail: ControlHost,
    pub media: Media,
    pub requests: VecDeque<Request>,
    pub navigation: Option<(Page, String)>,
    pub external: Option<String>,
    pub market_reveal: bool,
    pub login_requested: bool,
    pub login_active: bool,
    pub page: Page,
    pub connected: bool,
    pub error: Option<String>,
    pub(crate) pending: HashMap<String, Value>,
    pub(super) inventory: Listing,
    pub(super) inventory_view: listing::ListingController,
    pub(super) mastery: Listing,
    pub(super) mastery_view: listing::ListingController,
    pub(super) relics: Listing,
    pub(super) relics_view: listing::ListingController,
    pub(super) quotes: HashMap<String, Quote>,
    pub(super) quote_queue: VecDeque<String>,
    pub(super) quote_pending: HashSet<String>,
    pub(super) quote_checked: HashMap<String, Instant>,
    pub(super) activity: Value,
    pub(super) activity_error: Option<String>,
    pub(super) notifications: Value,
    pub(super) rail_tab: bool,
    pub(super) fissure_mode: String,
    pub(super) rail_scroll: Scroll,
    pub(super) rail_offset: u32,
    pub(super) events: rail::events::Events,
    pub(super) rail_view: rail::declaration::Rail,
    pub(super) market_rail_view: market::rail::MarketRail,
    pub(super) last_activity: Instant,
    pub(super) last_prices: Instant,
    pub(super) market: market::Market,
    pub(super) market_view: market::MarketController,
    pub(super) builds: build_planner::Builds,
    pub(super) build_view: build_planner::BuildPlannerController,
    pub(super) fixture: bool,
    pub(super) dirty: bool,
    pub(super) input_dirty: (bool, bool),
    pub(super) preferences: crate::domain::preferences::Preferences,
    pub(super) stale: HashSet<&'static str>,
    #[cfg(all(test, feature = "profiling"))]
    pub(super) listing_profile: [Duration; 3],
}

impl Pages {
    #[cfg(test)]
    pub fn new(preferences: crate::domain::preferences::Preferences) -> Result<Self, String> {
        Self::with_faces(preferences, &Default::default())
    }
    pub fn with_faces(
        preferences: crate::domain::preferences::Preferences,
        faces: &retinue_render::painter::FontFaces,
    ) -> Result<Self, String> {
        Ok(Self {
            ui: ControlHost::with_faces(faces)?,
            rail: ControlHost::with_faces(faces)?,
            media: Media::default(),
            requests: VecDeque::new(),
            navigation: None,
            external: None,
            market_reveal: false,
            login_requested: false,
            login_active: false,
            page: Page::Foundry,
            connected: false,
            error: None,
            pending: HashMap::new(),
            inventory: Listing::new(Page::Inventory),
            inventory_view: listing::ListingController::new(Page::Inventory),
            mastery: Listing::new(Page::Mastery),
            mastery_view: listing::ListingController::new(Page::Mastery),
            relics: Listing::new(Page::Relics),
            relics_view: listing::ListingController::new(Page::Relics),
            quotes: HashMap::new(),
            quote_queue: VecDeque::new(),
            quote_pending: HashSet::new(),
            quote_checked: HashMap::new(),
            activity: Value::Null,
            activity_error: None,
            notifications: Value::Null,
            rail_tab: false,
            fissure_mode: "all".into(),
            rail_scroll: Default::default(),
            rail_offset: 0,
            events: rail::events::Events::new(),
            rail_view: rail::declaration::Rail::new(),
            market_rail_view: market::rail::MarketRail::new(),
            last_activity: Instant::now(),
            last_prices: Instant::now(),
            market: Default::default(),
            market_view: market::MarketController::new(),
            builds: Default::default(),
            build_view: build_planner::BuildPlannerController::new(),
            fixture: false,
            dirty: true,
            input_dirty: (false, false),
            preferences,
            stale: HashSet::new(),
            #[cfg(all(test, feature = "profiling"))]
            listing_profile: Default::default(),
        })
    }
    pub fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.close_listing_filters();
            self.ui.reset_view();
            self.inventory_view.installed = false;
            self.mastery_view.installed = false;
            self.relics_view.installed = false;
            self.market_view.installed = false;
            self.build_view.installed = false;
        }
        self.page = page;
        self.ui.cancel_pointer();
        self.ui.focus = None;
        self.dirty = true;
        self.ensure();
        self.start_prices(false);
    }
    #[cfg(test)]
    pub fn capture_reply(&mut self, request: Request, result: Result<Value, String>) {
        let result =
            result.and_then(|data| crate::domain::session::Reply::fixture(&request.key, data));
        self.capture_native_reply(request, result);
    }
    pub fn capture_native_reply(
        &mut self,
        request: Request,
        result: Result<crate::domain::session::Reply, String>,
    ) {
        self.pending
            .insert(request.key.clone(), request.body.clone());
        self.update(Update::Reply { request, result });
    }
    pub fn configure(&mut self, center: (u32, u32), right: (u32, u32), scale: f32) {
        let center_changed = self.ui.size != center || self.ui.scale != scale;
        let rail_changed = self.rail.size != right || self.rail.scale != scale;
        self.ui.size = center;
        self.rail.size = right;
        self.ui.set_scale(scale);
        self.rail.set_scale(scale);
        if self.dirty {
            self.layout();
        } else {
            if center_changed {
                self.layout_center();
            }
            if rail_changed {
                self.layout_rail();
            }
        }
    }
    pub(super) fn queue(&mut self, key: &str, body: Value) {
        if self.fixture {
            return;
        }
        if self.pending.get(key) == Some(&body) {
            return;
        }
        self.pending.insert(key.into(), body.clone());
        self.requests.push_back(Request {
            key: key.into(),
            body,
        });
    }
    pub fn refresh(&mut self) {
        match self.page {
            Page::Inventory=>self.queue("inventory",json!({"op":"inventory_view"})),
            Page::Mastery=>self.queue("mastery",json!({"op":"mastery_view"})),
            Page::Relics=>self.queue("relics",json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"})),
            Page::Market=>self.queue("market",json!({"op":"market_account"})),
            Page::Builds=>{
                self.queue("equipment",json!({"op":"build_equipment"}));
                self.queue("groups",json!({"op":"build_group_list"}));
                self.queue("overframe",json!({"op":"overframe_account"}));
            }
            _=>(),
        }
    }
    pub(super) fn ensure(&mut self) {
        if !self.connected || self.fixture {
            return;
        }
        let loaded = match self.page {
            Page::Inventory => self.inventory.ready,
            Page::Mastery => self.mastery.ready,
            Page::Relics => self.relics.ready,
            Page::Market => self.market.account["authenticated"].is_boolean(),
            Page::Builds => self.builds.equipment.is_some(),
            _ => true,
        };
        let key = match self.page {
            Page::Inventory => "inventory",
            Page::Mastery => "mastery",
            Page::Relics => "relics",
            Page::Market => "market",
            Page::Builds => "equipment",
            _ => "",
        };
        if !loaded || self.stale.contains(key) {
            self.refresh();
        }
    }
    pub fn update(&mut self, update: Update) -> Damage {
        match update {
            Update::Connected => {
                self.connected = true;
                self.error = None;
                self.queue("activity", json!({"op":"activity_view"}));
                self.queue("notifications", json!({"op":"notification_settings"}));
                self.queue("market", json!({"op":"market_account"}));
                self.queue(
                    "diagnostics",
                    json!({"op":"diagnostics_report","issues":self.media.issues()}),
                );
                self.refresh();
            }
            Update::Disconnected(error) => {
                self.connected = false;
                self.error = Some(error);
                self.pending.clear();
                self.quote_pending.clear();
                self.quote_queue.clear();
                self.requests.clear();
                self.stale
                    .extend(["inventory", "mastery", "relics", "equipment", "market"]);
            }
            Update::Event(event) => {
                if event["event"] == "asset" {
                    if let Some(asset) = crate::domain::foundry::descriptor(&event["data"]) {
                        self.media.assets(vec![asset]);
                    }
                } else if event["event"] == "market_presence" {
                    self.market.account["presence"] = event["data"].clone();
                } else if event["event"] == "build_group" {
                    self.build_event(&event);
                } else if matches!(event["event"].as_str(), Some("dataset" | "update")) {
                    if event["dataset"] == "worldstate" {
                        self.queue("activity", json!({"op":"activity_view"}));
                    } else if event["dataset"] == "player" {
                        self.stale.extend([
                            "inventory",
                            "mastery",
                            "relics",
                            "equipment",
                            "market",
                        ]);
                        self.refresh();
                    }
                }
            }
            Update::Reply { request, result } => {
                if self.pending.get(&request.key) != Some(&request.body) {
                    return Damage::None;
                }
                self.pending.remove(&request.key);
                let is_price =
                    request.key.starts_with("price:") || request.key.starts_with("cache:");
                if is_price {
                    self.price_reply(
                        &request,
                        result.and_then(crate::domain::session::Reply::json),
                    );
                } else {
                    match result {
                        Err(error) => {
                            self.error = Some(error.clone());
                            if request.key.starts_with("market") {
                                self.market_error(&request.key, error.clone());
                            }
                            match request.key.as_str() {
                                "inventory" => self.inventory.error = Some(error),
                                "mastery" => self.mastery.error = Some(error),
                                "relics" => self.relics.error = Some(error),
                                "activity" => self.activity_error = Some(error),
                                _ => (),
                            }
                        }
                        Ok(data) => self.data_reply(&request, data),
                    }
                }
            }
        }
        self.dirty = true;
        self.layout();
        Damage::Full
    }
    pub(super) fn data_reply(&mut self, request: &Request, reply: crate::domain::session::Reply) {
        if !request.key.starts_with("market:comparison:") {
            self.error = None;
        }
        self.stale.remove(request.key.as_str());
        let data = match reply {
            crate::domain::session::Reply::Listing(data) => {
                self.ui.cancel_activation();
                let listing = match request.key.as_str() {
                    "inventory" => &mut self.inventory,
                    "mastery" => &mut self.mastery,
                    _ => &mut self.relics,
                };
                listing.replace(data, &self.quotes);
                self.start_prices(false);
                return;
            }
            crate::domain::session::Reply::Equipment(data) => {
                self.builds.equipment = Some(data);
                self.builds.invalidate();
                return;
            }
            crate::domain::session::Reply::Json(data) => data,
        };
        match request.key.as_str() {
            "activity" => {
                self.activity = data;
                self.activity_error = None;
                self.last_activity = Instant::now();
            }
            "notifications" => self.notifications = data,
            "market" => self.market_reply("market", data),
            "groups" => {
                self.builds.groups = data;
                self.builds.invalidate();
            }
            "overframe" => self.builds.account = data,
            _ => {
                if request.key.starts_with("market:") {
                    self.market_reply(&request.key, data);
                } else if request.key.starts_with("build:") {
                    self.build_reply(request, data);
                }
            }
        }
    }
    pub fn assets(&self) -> Vec<Asset> {
        let mut a = self.ui.assets();
        a.extend(self.rail.assets());
        let mut seen = HashSet::new();
        a.retain(|a| seen.insert(a.id.clone()));
        a
    }
    pub fn descriptors(&mut self, assets: Vec<Descriptor>) {
        self.media.assets(assets);
        self.dirty = true;
    }
    pub fn image_requests(&self) -> Vec<ImageRequest> {
        let mut r = self.ui.requests(&self.media);
        r.extend(self.rail.requests(&self.media));
        let mut seen = HashSet::new();
        r.retain(|r| seen.insert(r.key));
        r
    }
    pub fn images(&mut self, images: &[(Key, Resource<RasterImage>)]) {
        self.media.accept(images, &self.image_requests());
    }
    pub fn image_health(
        &mut self,
        images: &[(Key, Resource<RasterImage>)],
        errors: &[(Key, String)],
    ) {
        if self.media.image_health(images, errors) {
            self.queue(
                "diagnostics",
                json!({"op":"diagnostics_report","issues":self.media.issues()}),
            );
        }
    }
    pub fn ready(&self) -> bool {
        match self.page {
            Page::Inventory => self.inventory.ready || self.inventory.error.is_some(),
            Page::Mastery => self.mastery.ready || self.mastery.error.is_some(),
            Page::Relics => self.relics.ready || self.relics.error.is_some(),
            Page::Market => {
                self.error.is_some() || self.market.account["authenticated"].is_boolean()
            }
            Page::Builds => self.error.is_some() || self.builds.equipment.is_some(),
            _ => self.error.is_some() || self.connected || self.fixture,
        }
    }
    pub fn rail_capture_ready(&self) -> bool {
        if self.fixture {
            return true;
        }
        if !self.connected {
            return self.error.is_some();
        }
        !self.pending.keys().any(|key| {
            if self.rail_tab {
                matches!(
                    key.as_str(),
                    "market:resolve" | "market:describe" | "market:quote"
                )
            } else {
                key == "activity" || key == "notifications"
            }
        })
    }
    pub fn paint(&self, p: &mut Painter<'_>) {
        self.ui.paint(p, &self.media);
    }
    pub fn paint_rail(&self, p: &mut Painter<'_>) {
        self.rail.paint(p, &self.media);
    }
    pub(super) fn layout(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.layout_center();
        self.layout_rail();
    }
    pub(super) fn layout_center(&mut self) {
        let size = self.ui.size;
        let scale = self.ui.scale;
        self.ui.begin(size, scale);
        if matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            self.layout_listing();
            self.ui.finish();
            return;
        }
        if self.page == Page::Market {
            self.layout_market();
            self.ui.finish();
            return;
        }
        if self.page == Page::Builds {
            self.layout_builds();
            self.ui.finish();
            return;
        }
        self.ui.fill(
            Rect {
                x: 0,
                y: 0,
                width: size.0,
                height: size.1,
            },
            10.0,
            controls::PANEL,
        );
        self.ui.fill(
            Rect {
                x: 0,
                y: self.ui.px(10.0),
                width: size.0,
                height: size.1.saturating_sub(self.ui.px(10.0)),
            },
            0.0,
            controls::PANEL,
        );
        self.ui.finish();
    }
    pub fn pointer(&mut self, rail: bool, point: (f64, f64)) -> bool {
        let changed = if rail {
            self.rail.pointer_move(point)
        } else {
            self.ui.pointer_move(point)
        };
        if changed {
            if rail {
                self.input_dirty.1 = true;
            } else {
                self.input_dirty.0 = true;
            }
        }
        changed
    }
    pub fn button(&mut self, rail: bool, down: bool) {
        if rail && self.ui.modal_open() {
            return;
        }
        if !rail
            && down
            && (self.dismiss_listing_filters(self.ui.pointer())
                || self.dismiss_listing_menu(self.ui.pointer()))
        {
            return;
        }
        if rail {
            self.rail.pointer_button(down);
        } else {
            self.ui.pointer_button(down);
        }
        self.dirty = true;
        self.signals();
        self.layout();
    }
    pub fn dismiss_popups_at(&mut self, center: (f64, f64), rail: (f64, f64)) -> bool {
        let dismissed = self.ui.dismiss_popup_at(center) | self.rail.dismiss_popup_at(rail);
        let dismissed = self.dismiss_listing_filters(center) || dismissed;
        let dismissed = self.dismiss_listing_menu(center) || dismissed;
        if dismissed {
            self.dirty = true;
        }
        dismissed
    }
    pub fn context_menu(&mut self) -> bool {
        if !matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            return false;
        }
        let key = self.ui.hover.clone().or_else(|| self.ui.focus.clone());
        if let Some(key) = key {
            return self.open_listing_menu(&key);
        }
        false
    }
    pub fn keyboard_context_menu(&mut self) -> bool {
        if !matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            return false;
        }
        let Some(key) = self.ui.focus.clone() else {
            return false;
        };
        let anchor = self
            .ui
            .buttons
            .iter()
            .find(|b| b.key == key)
            .map(|b| b.rect);
        if !self.open_listing_menu(&key) {
            return false;
        }
        if let Some(rect) = anchor {
            self.listing_mut().menu_anchor = (rect.x as f64, (rect.y + rect.height) as f64);
            self.dirty = true;
            self.layout();
        }
        true
    }
    pub fn dismiss_context_menu(&mut self) {
        self.dismiss_listing_menu((-1.0, -1.0));
    }
    pub fn key(
        &mut self,
        rail: bool,
        key: retinue_ui::desktop::winit::keyboard::Key<&str>,
        mods: retinue_ui::desktop::winit::keyboard::ModifiersState,
    ) -> bool {
        use retinue_ui::{desktop::winit::keyboard::Key, desktop::winit::keyboard::NamedKey};
        if !rail && self.listing_filter_key(&key, mods) {
            return true;
        }
        if rail && self.market_search_key(key.clone()) {
            return true;
        }
        if !rail && key == Key::Named(NamedKey::Escape) && !self.ui.popup_open() {
            self.market.dialog = None;
            self.builds.dismiss();
            for listing in [&mut self.inventory, &mut self.mastery, &mut self.relics] {
                listing.menu = None;
            }
            self.dirty = true;
        }
        if mods.control_key() && key == Key::Character("r") {
            self.refresh();
            return true;
        }
        let changed = if rail {
            self.rail.key(key.clone(), mods)
        } else {
            self.ui.key(key.clone(), mods)
        };
        self.dirty |= changed;
        self.signals();
        self.layout();
        if changed {
            return true;
        }
        let height = if rail {
            self.rail.size.1
        } else {
            self.ui.size.1
        } as f32;
        let delta = match key {
            Key::Named(NamedKey::PageDown) => height * 0.8,
            Key::Named(NamedKey::PageUp) => -height * 0.8,
            Key::Named(NamedKey::ArrowDown) => 48.0 * self.ui.scale,
            Key::Named(NamedKey::ArrowUp) => -48.0 * self.ui.scale,
            Key::Named(NamedKey::Home) => -f32::MAX,
            Key::Named(NamedKey::End) => f32::MAX,
            _ => return false,
        };
        self.wheel(rail, delta, true);
        true
    }
    pub fn cancel_pointer(&mut self) {
        self.ui.cancel_pointer();
        self.rail.cancel_pointer();
    }
    pub fn wheel(&mut self, rail: bool, delta: f32, smooth: bool) {
        if self.listing_filters_open() {
            if !rail {
                self.listing_filter_wheel(delta, smooth);
            }
            return;
        }
        if self.ui.modal_open() {
            return;
        }
        if rail {
            self.rail_scroll.by(delta, smooth, Instant::now());
        } else {
            match self.page {
                Page::Inventory => self.inventory.wheel(delta, smooth),
                Page::Mastery => self.mastery.wheel(delta, smooth),
                Page::Relics => self.relics.wheel(delta, smooth),
                Page::Market => self.market.scroll.by(delta, smooth, Instant::now()),
                Page::Builds => self.builds.wheel(self.ui.pointer(), delta, smooth),
                _ => (),
            }
        }
    }
    pub fn tick(&mut self, now: Instant) -> (Damage, Damage) {
        let time = retinue_ui::timeline::FrameTime::now().at(now);
        let mut changed = self.ui.tick(now);
        let mut rail_changed = self.rail.tick(now);
        for listing in [&mut self.inventory, &mut self.mastery, &mut self.relics] {
            changed |= listing.tick(now) && listing.page == self.page;
        }
        let offset = self.rail_scroll.offset(now).round() as u32;
        rail_changed |= offset != self.rail_offset;
        self.rail_offset = offset;
        rail_changed |= !self.rail_tab && self.events.expired(rail::events::milliseconds(time));
        if self.connected && now.duration_since(self.last_activity) >= Duration::from_secs(60) {
            self.queue("activity", json!({"op":"activity_view"}));
            self.last_activity = now;
        }
        if self.connected
            && self.quote_pending.is_empty()
            && now.duration_since(self.last_prices) >= Duration::from_secs(60)
        {
            self.start_prices(false);
            self.last_prices = now;
        }
        changed |= self.market.tick(now) | self.builds.tick(now);
        self.market_tick(now);
        self.signals();
        let input = std::mem::take(&mut self.input_dirty);
        changed |= input.0;
        rail_changed |= input.1;
        if self.dirty {
            changed = true;
            rail_changed = true;
            self.layout();
        } else {
            if changed {
                self.layout_center();
            }
            if rail_changed {
                self.layout_rail_at(time);
            }
        }
        (
            if changed {
                Damage::Full
            } else {
                self.ui.animation_damage()
            },
            if rail_changed {
                Damage::Full
            } else {
                self.rail
                    .animation_damage()
                    .union(self.rail.update_view(time))
            },
        )
    }
    pub fn deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        [
            self.ui.deadline(),
            self.rail.deadline(),
            match self.page {
                Page::Inventory | Page::Mastery | Page::Relics => self.listing().deadline(now),
                _ => None,
            },
            self.rail_scroll.deadline(now),
            self.market.deadline(now),
            self.rail_tab
                .then(|| self.market.quote_deadline())
                .flatten(),
            self.builds.scroll.deadline(now),
            self.builds.deadline(now),
            (!self.rail_tab)
                .then(|| {
                    self.events
                        .expiry_deadline(retinue_ui::timeline::FrameTime::now())
                })
                .flatten(),
            self.connected
                .then_some(self.last_activity + Duration::from_secs(60)),
            (self.connected && self.quote_pending.is_empty())
                .then_some(self.last_prices + Duration::from_secs(60)),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub(super) fn signals(&mut self) -> bool {
        let signals = self
            .ui
            .signals
            .drain(..)
            .chain(self.rail.signals.drain(..))
            .collect::<Vec<_>>();
        for signal in signals {
            match signal.key.as_str() {
                "list:scroll" => self.input_dirty.0 = true,
                "rail:scroll" => self.input_dirty.1 = true,
                _ => self.dirty = true,
            }
            if signal.key.starts_with("rail:") {
                self.rail_signal(signal);
            } else if signal.key.starts_with("market:") {
                self.market_signal(signal);
            } else if signal.key.starts_with("build:") {
                self.build_signal(signal);
            } else {
                self.listing_signal(signal);
            }
        }
        self.dirty
    }
    pub fn search(&mut self, page: Page, text: &str) {
        match page {
            Page::Inventory => self.inventory.search = text.into(),
            Page::Mastery => self.mastery.search = text.into(),
            Page::Relics => {
                self.relics.search = text.into();
                self.relics.category = "all".into();
            }
            _ => (),
        }
        self.page = page;
        self.ensure();
        self.resort(true);
        self.dirty = true;
        self.layout();
    }
    pub fn capture_state(
        &mut self,
        view: &str,
        select: &str,
        search: &str,
        market: Option<&str>,
    ) -> Result<(), String> {
        if view == "filters" && matches!(self.page, Page::Inventory | Page::Mastery) {
            self.listing_filter_signal(&Signal {
                key: "list:filters".into(),
                value: None,
            });
            self.dirty = true;
            self.layout();
            return Ok(());
        }
        if view == "loading" {
            self.activity = Value::Null;
            self.market.account = Value::Null;
            self.quote_pending.extend(self.mastery.price_names());
            self.dirty = true;
            self.layout();
            return Ok(());
        }
        if matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            self.search(self.page, search);
        } else if self.page == Page::Market {
            self.market_signal(Signal {
                key: "market:search".into(),
                value: Some(search.into()),
            });
        }
        if !view.is_empty() {
            if view == "context-menu"
                && matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics)
            {
                let index = *self
                    .listing()
                    .visible
                    .first()
                    .ok_or("No item for context menu")?;
                self.ui
                    .pointer_move((self.ui.px(140.0) as f64, self.ui.px(140.0) as f64));
                self.open_listing_menu(&format!("list:item:{index}:menu"));
                return Ok(());
            }
            match self.page {
                Page::Inventory => self.inventory.category = view.into(),
                Page::Relics => self.relics.category = view.into(),
                Page::Mastery => self.mastery.mode = view.into(),
                Page::Builds => self.capture_build(view, select)?,
                Page::Market => self.capture_market(view, select)?,
                _ => return Err(format!("Unsupported capture view: {view}")),
            }
        } else if self.page == Page::Builds && !select.is_empty() {
            self.capture_build("equipment", select)?;
        }
        if matches!(self.page, Page::Inventory | Page::Mastery | Page::Relics) {
            self.resort(true);
            self.start_prices(false);
        }
        if let Some(item) = market {
            self.open_market(item.into(), "sell");
        }
        self.dirty = true;
        self.layout();
        Ok(())
    }
    pub fn capture_scroll(&mut self, offset: u32) {
        self.wheel(false, offset as f32, false);
        self.tick(Instant::now());
    }
    pub fn settle_capture(&mut self) {
        self.ui.settle_searches();
        self.rail.settle_searches();
        self.dirty = true;
        self.layout();
    }
    pub fn capture_hover(&mut self, key: &str) -> Result<(), String> {
        let rect = self
            .ui
            .bounds(key)
            .ok_or_else(|| format!("Control is not visible: {key}"))?;
        self.pointer(
            false,
            (
                (rect.x + rect.width / 2) as f64,
                (rect.y + rect.height / 2) as f64,
            ),
        );
        self.dirty = true;
        self.layout();
        self.builds.sample_preview();
        self.dirty = true;
        self.layout();
        Ok(())
    }
    pub fn report(&self) -> Value {
        json!({"connected":self.connected,"error":self.error,"inventory":self.inventory.report(),"mastery":self.mastery.report(),"relics":self.relics.report(),"pending":self.pending.keys().collect::<Vec<_>>(),"assets":self.media.errors,
            "price_work":{"queued":self.quote_queue.len(),"pending":self.quote_pending.len(),"requests":self.pending.keys().filter(|k|k.starts_with("price:")).count(),"cache_requests":self.pending.keys().filter(|k|k.starts_with("cache:")).count()},
            "layout":self.ui.layout_report(),"rail_layout":self.rail.layout_report()})
    }
}
