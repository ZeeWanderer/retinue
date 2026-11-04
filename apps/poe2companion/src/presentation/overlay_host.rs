use super::evaluate::panel;
use super::evaluate::{
    evaluate_model::{self, EvaluateModel, Presentation},
    panel::{EvaluatePanel, Frame, Interaction, Menu, Target},
};
use crate::domain::{
    preferences::Preferences,
    trade::{Job, Reply, Worker},
};

use retinue_client::reload::Gate;
use retinue_render::damage::Damage;
use retinue_ui::{
    focus::FocusDetector,
    schedule::{Deadlines, Ping, make_ping},
    surface::{Application, Input},
    window_input::{Binding, ModMask, WindowInput},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub struct Service {
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    wake: Ping,
    notifications: tokio::task::JoinHandle<()>,
}
impl Service {
    pub fn start(
        updates: watch::Receiver<Option<crate::Observation>>,
        stopping: Arc<AtomicBool>,
        gate: Gate,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let local = stop.clone();
        let (wake, source) = make_ping().map_err(|e| e.to_string())?;
        let worker_wake = wake.clone();
        let mut notices = updates.clone();
        let thread = thread::Builder::new()
            .name("poe2-evaluate".into())
            .spawn(move || {
                let result = (|| {
                    let window_input = WindowInput::connect(&[
                        Binding {
                            keysym: b'e' as u32,
                            modifiers: ModMask::M1,
                        },
                        Binding {
                            keysym: 0xff1b,
                            modifiers: ModMask::default(),
                        },
                    ])?;
                    let preferences = Preferences::load();
                    let mut model = EvaluateModel::new(preferences.league.clone());
                    model.status = preferences.status.clone();
                    let app = Overlay {
                        model,
                        open_after_search: false,
                        preferences,
                        view: EvaluatePanel::new()?,
                        interaction: Interaction::default(),
                        worker: Worker::start(worker_wake)?,
                        updates,
                        focus: FocusDetector::connect("steam_app_2694490").ok(),
                        window_input,
                        game_window: None,
                        input_error: None,
                        activate_requested: false,
                        return_focus_requested: false,
                        pending_copy: None,
                        next_copy_check: Instant::now(),
                        gate: gate.clone(),
                        stopping,
                        local: local.clone(),
                        last_animation: Instant::now(),
                        last_focus: Instant::now() - Duration::from_secs(1),
                        presentation: Presentation::default(),
                        hover_since: Instant::now(),
                        started: Instant::now(),
                        dpr: 1,
                    };
                    retinue_ui::surface::run("poe2companion", Box::new(app), local, source)
                })();
                gate.interaction(false);
                if let Err(error) = result {
                    eprintln!("poe2companion: overlay.unavailable: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        let notice_wake = wake.clone();
        let notifications = tokio::spawn(async move {
            while notices.changed().await.is_ok() {
                notice_wake.ping();
            }
        });
        Ok(Self {
            thread: Some(thread),
            stop,
            wake,
            notifications,
        })
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.notifications.abort();
        self.wake.ping();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Overlay {
    model: EvaluateModel,
    open_after_search: bool,
    preferences: Preferences,
    view: EvaluatePanel,
    interaction: Interaction,
    worker: Worker,
    updates: watch::Receiver<Option<crate::Observation>>,
    focus: Option<FocusDetector>,
    window_input: WindowInput,
    game_window: Option<u32>,
    input_error: Option<String>,
    activate_requested: bool,
    return_focus_requested: bool,
    pending_copy: Option<(u64, Option<u32>, Instant)>,
    next_copy_check: Instant,
    gate: Gate,
    stopping: Arc<AtomicBool>,
    local: Arc<AtomicBool>,
    last_animation: Instant,
    last_focus: Instant,
    presentation: Presentation,
    hover_since: Instant,
    started: Instant,
    dpr: u32,
}
impl Overlay {
    fn predict(&mut self) {
        if let Some((id, request)) = self.model.predict() {
            if self.worker.submit(Job::Predict(id, request)).is_err() {
                self.model.predicted(id, None);
            }
        }
    }
    fn send(&mut self, job: Job) {
        if self.worker.submit(job).is_err() {
            self.model.failed(
                self.model.generation,
                "Evaluate worker is busy; try again".into(),
            );
        }
    }
    fn search(&mut self) {
        self.search_with_refresh(true);
    }
    fn restore(&mut self, requested: bool) {
        if let Some(request) = self.model.restore() {
            self.send(Job::Restore(self.model.generation, request, requested));
        }
    }
    fn search_with_refresh(&mut self, refresh: bool) {
        if self.model.loading || self.model.retry_after() > 0 {
            return;
        }
        if self.model.evaluation.is_none() && !self.model.text.is_empty() {
            self.model.loading = true;
            self.model.error = None;
            self.send(Job::Evaluate(
                self.model.generation,
                self.model.text.clone(),
            ));
            return;
        }
        match self.model.search() {
            Ok(mut request) => {
                request["refresh"] = serde_json::json!(refresh);
                self.send(Job::Search(self.model.generation, request));
            }
            Err(error) => self.model.error = Some(error),
        }
    }
    fn fetch(&mut self) {
        if let Some(request) = self
            .model
            .fetch(self.interaction.scroll, self.view.visible_listings)
        {
            self.send(Job::Fetch(self.model.generation, request));
        }
    }
    fn field(&mut self) -> Option<&mut String> {
        match self.interaction.focus? {
            Target::LevelMinimum => Some(&mut self.model.level.min),
            Target::LevelMaximum => Some(&mut self.model.level.max),
            Target::Minimum(i) => self.model.filters.get_mut(i).map(|f| &mut f.min),
            Target::Maximum(i) => self.model.filters.get_mut(i).map(|f| &mut f.max),
            _ => None,
        }
    }
    fn begin_edit(&mut self, target: Target, select: bool) -> bool {
        if !self.interaction.focus_field(target, &self.model) {
            return false;
        }
        if let Some(value) = self.field().cloned() {
            self.view.begin_edit(target, &value, select);
        }
        true
    }
    fn sync_editor(&mut self) {
        let value = self.view.editor.text();
        if let Some(field) = self.field()
            && *field != value
        {
            *field = value;
            self.change();
        }
    }
    fn insert_text(&mut self, text: &str) {
        if self.interaction.focus.is_some()
            && self.view.editor.insert(text, evaluate_model::numeric_edit)
        {
            self.sync_editor();
        }
    }
    fn change(&mut self) {
        self.open_after_search = false;
        self.model.invalidate();
        self.interaction.scroll = 0;
        self.restore(false);
    }
    fn escape(&mut self) {
        if !self.interaction.close_popup() {
            self.open_after_search = false;
            self.model.dismiss();
        }
    }
    fn save_preferences(&mut self) {
        self.preferences.league = self.model.league.clone();
        self.preferences.status = self.model.status.clone();
        if let Err(e) = self.preferences.save() {
            eprintln!("poe2companion: evaluate.preferences: {e}");
        }
    }
    fn click(&mut self, target: Target) {
        if self.model.evaluation.is_none()
            && !matches!(
                target,
                Target::Close
                    | Target::ScaleUp
                    | Target::ScaleDown
                    | Target::Search
                    | Target::Menu(Menu::Settings)
                    | Target::DismissMenu
            )
        {
            return;
        }
        match target {
            Target::Close => self.model.dismiss(),
            Target::Menu(menu) => {
                let was_open = self.interaction.menu == Some(menu);
                self.interaction.close_popup();
                if !was_open {
                    self.interaction.menu = Some(menu);
                    self.interaction.menu_anchor = self.view.field_rect(target);
                }
            }
            Target::DismissMenu => {
                self.interaction.close_popup();
            }
            Target::Option(menu, index) => {
                let choice = menu
                    .choices(&self.model)
                    .nth(index)
                    .map(|(id, _)| id.map(str::to_owned));
                if let Some(choice) = choice {
                    let refresh = self.model.results.is_some();
                    match menu {
                        Menu::Status => self.model.status = choice.unwrap_or_else(|| "any".into()),
                        Menu::League => self.model.league = choice.unwrap_or_default(),
                        Menu::Rarity => self.model.rarity = choice,
                        Menu::Currency => self.model.currency = choice,
                        Menu::Indexed => self.model.indexed = choice,
                        Menu::Settings => {}
                    }
                    if refresh && !matches!(menu, Menu::Rarity | Menu::League) {
                        self.open_after_search = false;
                        self.model.refresh_results();
                        self.interaction.scroll = 0;
                        self.restore(true);
                    } else {
                        self.change();
                    }
                    self.save_preferences();
                    if menu == Menu::League {
                        self.predict();
                    }
                }
                self.interaction.menu = None;
            }
            Target::Sort => {
                self.model.descending = !self.model.descending;
                self.open_after_search = false;
                self.model.refresh_results();
                self.interaction.scroll = 0;
                self.restore(true);
            }
            Target::Level => {
                self.model.level.enabled = !self.model.level.enabled;
                self.change();
            }
            Target::Condition(index) => {
                if let Some(enabled) = self.model.conditions.get_mut(index) {
                    *enabled = !*enabled;
                    self.change();
                }
            }
            Target::Base => {
                self.model.base = !self.model.base;
                self.change();
            }
            Target::Modifier(i) => {
                if let Some(f) = self.model.filters.get_mut(i) {
                    f.enabled = !f.enabled;
                    self.change();
                }
            }
            Target::Include(i) => {
                if let Some(filter) = self.model.filters.get_mut(i)
                    && !filter.enabled
                {
                    filter.enabled = true;
                    self.change();
                }
            }
            Target::Clear => {
                for f in &mut self.model.filters {
                    f.enabled = false;
                }
                self.change();
            }
            Target::Profile(profile) => {
                self.model.apply_profile(profile);
                self.change();
            }
            Target::Search => self.search(),
            Target::Trade | Target::TradeLink => {
                if let Some(r) = &self.model.results {
                    let url = r.url.clone();
                    self.model.dismiss();
                    self.send(Job::Open(url));
                } else {
                    self.search();
                    self.open_after_search = self.model.loading;
                }
            }
            Target::ScaleDown | Target::ScaleUp => {
                let delta = if target == Target::ScaleUp {
                    0.05
                } else {
                    -0.05
                };
                self.preferences.scale = ((self.preferences.scale + delta) * 20.0).round() / 20.0;
                self.preferences.scale = self.preferences.scale.clamp(0.75, 2.0);
                self.save_preferences();
            }
            _ => {}
        }
    }
}

impl Application for Overlay {
    fn visible(&self) -> bool {
        self.presentation.visible(&self.model)
    }
    fn keyboard_interactive(&self) -> bool {
        self.model.visible && self.interaction.focus.is_some()
    }
    fn text_cursor(&self) -> bool {
        match self.interaction.hover {
            Some(Target::LevelMinimum | Target::LevelMaximum) => self.model.level.enabled,
            Some(Target::Minimum(index) | Target::Maximum(index)) => self
                .model
                .filters
                .get(index)
                .is_some_and(|filter| filter.enabled),
            _ => false,
        }
    }
    fn clipboard_text(&self) -> Option<String> {
        self.interaction
            .focus
            .and_then(|_| self.view.editor.selected_text())
    }
    fn pointer_regions(&self) -> Vec<retinue_ui::Rect> {
        self.view.pointer_regions()
    }
    fn event_fds(&self) -> std::io::Result<Vec<std::os::fd::OwnedFd>> {
        Ok(vec![self.window_input.event_fd()?])
    }
    fn deadlines(&self, now: Instant) -> Deadlines {
        let mut deadlines = Deadlines::default();
        deadlines.control(self.view.maintenance_deadline());
        deadlines.control(Some(self.last_focus + Duration::from_millis(100)));
        deadlines.control(
            self.pending_copy
                .map(|(_, _, started)| started + Duration::from_secs(2)),
        );
        if self.pending_copy.is_some() {
            deadlines.control(Some(self.next_copy_check));
        }
        if self.visible() {
            if self.model.loading {
                deadlines.visual(Some(self.last_animation + Duration::from_millis(70)));
            }
            if self.interaction.hover.is_some() && !self.interaction.tooltip {
                deadlines.visual(Some(self.hover_since + Duration::from_millis(600)));
            }
            if self.interaction.focus.is_some() {
                deadlines.visual(Some(self.view.editor.next_blink(now)));
            }
            deadlines.visual(self.model.cooldown_deadline(now));
            deadlines.visual(self.model.budget.deadline(now));
            deadlines.visual(self.view.time_deadline(now));
        }
        deadlines
    }
    fn tick(&mut self, drawable: bool) -> Damage {
        self.view.maintain(Instant::now());
        if self.stopping.load(Ordering::Acquire) {
            self.local.store(true, Ordering::Release);
            return Damage::None;
        }
        let (observation_changed, observation) = {
            let current = self.updates.borrow_and_update();
            (current.has_changed(), current.clone())
        };
        let pid = observation
            .as_ref()
            .and_then(|o| o.process.as_ref())
            .map(|p| p.pid);
        let mut dirty = false;
        let mut damage = Damage::None;
        if observation_changed
            && let Some(snapshot) = observation.as_ref().and_then(|o| o.snapshot.as_ref())
            && self.model.observe_session(snapshot)
        {
            self.open_after_search = false;
            self.interaction.scroll = 0;
            self.save_preferences();
            if self.model.visible {
                self.predict();
                self.restore(false);
            }
            dirty = true;
        }
        if std::mem::take(&mut self.return_focus_requested) && self.presentation.keyboard {
            if let Some(focus) = &self.focus {
                match focus.request_game_focus_at_pointer(pid) {
                    Ok(requested) if cfg!(debug_assertions) => {
                        eprintln!("poe2companion: evaluate.focus_return: requested={requested}");
                    }
                    Err(error) => eprintln!("poe2companion: evaluate.focus_return: {error}"),
                    _ => {}
                }
            }
        }
        match self.window_input.poll() {
            Ok(keys) => {
                for key in keys {
                    match key {
                        0 => self.activate_requested = true,
                        1 if self.visible() => {
                            self.escape();
                            dirty = true;
                            if cfg!(debug_assertions) {
                                eprintln!("poe2companion: evaluate.escape: game-window");
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => eprintln!("poe2companion: evaluate.input: {error}"),
        }
        if self.last_focus.elapsed() >= Duration::from_millis(100) {
            self.last_focus = Instant::now();
            let window = self
                .focus
                .as_ref()
                .and_then(|focus| focus.active_game_window(pid));
            let active = window.is_some();
            self.game_window = window;
            if active != self.presentation.game_active && cfg!(debug_assertions) {
                eprintln!(
                    "poe2companion: evaluate.focus: active={active} visible={} copying={}",
                    self.model.visible, self.model.copying
                );
            }
            self.presentation.game_active = active;
        }
        if std::mem::take(&mut self.activate_requested)
            && !self.model.copying
            && self.gate.try_interaction()
        {
            let generation = self.model.copy();
            self.open_after_search = false;
            self.presentation.blur();
            if cfg!(debug_assertions) {
                eprintln!("poe2companion: evaluate.copy: id={generation}");
            }
            self.interaction = Interaction::default();
            self.pending_copy = Some((generation, pid, Instant::now()));
            self.next_copy_check = Instant::now();
            dirty = true;
        }
        if let Some((generation, pid, started)) = self.pending_copy
            && (Instant::now() >= self.next_copy_check
                || started.elapsed() >= Duration::from_secs(2))
        {
            self.next_copy_check = Instant::now() + Duration::from_millis(16);
            if self
                .focus
                .as_ref()
                .is_some_and(|focus| focus.is_active(pid))
            {
                self.pending_copy = None;
                self.send(Job::Copy(generation, pid));
            } else if started.elapsed() >= Duration::from_secs(2) {
                self.pending_copy = None;
                self.model.dismiss();
                eprintln!("poe2companion: evaluate.copy: game did not regain focus");
            }
        }
        while let Ok(reply) = self.worker.receive.try_recv() {
            dirty = true;
            match reply {
                Reply::Currencies(request, icons) => {
                    if request
                        == self
                            .model
                            .currency_request(self.preferences.scale * self.dpr as f32)
                    {
                        if self.view.currencies.len() + icons.len() > 64 {
                            self.view.currencies.clear();
                        }
                        self.view.currencies.extend(icons);
                    }
                }
                Reply::Copied(id, text) => {
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "poe2companion: evaluate.copied: id={id} bytes={}",
                            text.len()
                        );
                    }
                    if id == self.model.generation
                        && !self
                            .focus
                            .as_ref()
                            .is_some_and(|focus| focus.is_active(pid))
                    {
                        self.model.dismiss();
                        continue;
                    }
                    if self.model.copied(id, text.clone()) {
                        self.send(Job::Evaluate(id, text));
                    }
                }
                Reply::Evaluated(id, e) => {
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "poe2companion: evaluate.item: id={id} modifiers={}",
                            e.item.modifiers.len()
                        );
                    }
                    let current = id == self.model.generation;
                    let previous = self.model.league.clone();
                    self.model.evaluated(id, e);
                    if self.model.league != previous {
                        self.save_preferences();
                    }
                    if current {
                        self.predict();
                        self.restore(false);
                    }
                }
                Reply::Restored(id, reply, requested) => {
                    if self.model.restored(id, reply, requested) {
                        self.search_with_refresh(false);
                    }
                }
                Reply::Searched(id, r) => {
                    if cfg!(debug_assertions) {
                        eprintln!("poe2companion: evaluate.results: id={id} total={}", r.total);
                    }
                    self.model.searched(id, r);
                    if id == self.model.generation {
                        self.interaction.scroll = 0;
                    }
                    if id == self.model.generation && self.open_after_search {
                        self.open_after_search = false;
                        self.click(Target::Trade);
                    }
                }
                Reply::Fetched(id, page) => self.model.fetched(id, page),
                Reply::Predicted(id, result) => {
                    if let Err(error) = &result {
                        eprintln!("poe2companion: evaluate.prediction: {error}");
                    }
                    self.model.predicted(id, result.ok().flatten());
                }
                Reply::Error(id, error, retry) => {
                    if id == self.model.generation {
                        self.open_after_search = false;
                    }
                    eprintln!("poe2companion: evaluate.error: id={id} {error}");
                    if id == self.model.generation
                        && self.model.copying
                        && !self
                            .focus
                            .as_ref()
                            .is_some_and(|focus| focus.is_active(pid))
                    {
                        self.model.dismiss();
                    } else {
                        self.model.failed(id, error);
                        if id == self.model.generation && retry > 0 {
                            self.model.retry_at =
                                Some(Instant::now() + Duration::from_secs(retry.min(86400)));
                        }
                    }
                }
            }
        }
        if self.worker.budget.has_changed().unwrap_or(false) {
            let snapshot = self.worker.budget.borrow_and_update().clone();
            if let Some(snapshot) = snapshot {
                self.model.budget.observe(snapshot, Instant::now());
                self.model.retry_at = self.model.budget.retry_at();
                dirty = true;
            }
        }
        if self.model.visible {
            let request = self
                .model
                .currency_request(self.preferences.scale * self.dpr as f32);
            self.worker.currencies.send_if_modified(|previous| {
                if *previous == request {
                    false
                } else {
                    *previous = request;
                    true
                }
            });
        }
        if observation
            .as_ref()
            .is_some_and(|o| o.status == crate::Status::Waiting)
            && self.model.visible
        {
            self.model.dismiss();
            dirty = true;
        }
        self.gate
            .interaction(self.model.visible || self.model.copying);
        if !self.model.visible {
            self.view.release_layers();
            self.presentation.blur();
            self.interaction.focus = None;
            self.interaction.selecting = false;
        }
        let window = if self.presentation.keyboard || self.model.copying {
            None
        } else {
            self.game_window
        };
        let bindings: &[usize] = if self.visible() { &[0, 1] } else { &[0] };
        let error = self.window_input.bind(window, bindings).err();
        if error != self.input_error {
            if let Some(error) = &error {
                eprintln!("poe2companion: evaluate.input: {error}");
            }
            self.input_error = error;
        }
        if drawable
            && self.visible()
            && self.model.loading
            && self.last_animation.elapsed() >= Duration::from_millis(70)
        {
            self.last_animation = Instant::now();
            damage = damage.union(self.view.animation_damage());
        }
        if self.visible() {
            if drawable {
                damage = damage.union(self.view.time_damage(panel::current_time()));
                dirty |= self.model.tick_cooldown();
                let now = Instant::now();
                dirty |= self.model.budget.tick(now);
                damage = damage.union(self.view.budget_damage(self.model.budget.fill(now)));
            }
            let was_fetching = self.model.fetching;
            self.fetch();
            dirty |= was_fetching != self.model.fetching;
        }
        if drawable
            && self.visible()
            && self.interaction.hover.is_some()
            && !self.interaction.tooltip
            && self.hover_since.elapsed() >= Duration::from_millis(600)
        {
            self.interaction.tooltip = true;
            dirty = true;
        }
        if drawable && self.visible() && self.interaction.focus.is_some() {
            if self.view.editor.tick() {
                damage = damage.union(self.view.target_damage(self.interaction.focus));
            }
        }
        if dirty { Damage::Full } else { damage }
    }
    fn input(&mut self, event: Input) -> Damage {
        let physical = |x: f64, y: f64| (x * self.dpr as f64, y * self.dpr as f64);
        match event {
            Input::FocusLost => {
                self.view.cancel_pointer();
                self.return_focus_requested = false;
                if cfg!(debug_assertions) {
                    eprintln!("poe2companion: evaluate.focus_lost");
                }
                self.presentation.blur();
                self.last_focus = Instant::now() - Duration::from_secs(1);
                self.interaction.focus = None;
                self.interaction.selecting = false;
                self.interaction.drag = None;
                self.interaction.close_popup();
                self.interaction.tooltip = false;
            }
            Input::PointerLeave => {
                self.view
                    .pointer(retinue_ui::input::PointerPhase::Leave, None);
                self.return_focus_requested = self.presentation.keyboard;
                self.interaction.leave_panel();
            }
            Input::Motion(x, y) => {
                let (x, y) = physical(x, y);
                self.view
                    .pointer(retinue_ui::input::PointerPhase::Move, Some((x, y)));
                let previous = self.interaction.hover;
                let popup = self.interaction.tooltip || self.interaction.menu.is_some();
                self.interaction.hover = self.view.hit(x, y);
                if previous != self.interaction.hover {
                    self.interaction.tooltip = false;
                    self.hover_since = Instant::now();
                }
                if let Some((start_x, start_y, ox, oy)) = self.interaction.drag {
                    self.interaction.position = Some((
                        (ox as f64 + x - start_x).max(0.0) as u32,
                        (oy as f64 + y - start_y).max(0.0) as u32,
                    ));
                }
                if self.interaction.selecting
                    && let Some(target) = self.interaction.focus
                {
                    self.view.edit_drag(target, x);
                }
                return if self.interaction.drag.is_some() {
                    // Preparation contributes the new scene's damage before buffer repair.
                    Damage::rect(self.view.bounds)
                } else if self.interaction.selecting {
                    self.view.target_damage(self.interaction.focus)
                } else if previous == self.interaction.hover {
                    Damage::None
                } else if popup {
                    Damage::Full
                } else {
                    self.view
                        .target_damage(previous)
                        .union(self.view.target_damage(self.interaction.hover))
                };
            }
            Input::Press { x, y, shift } => {
                let (x, y) = physical(x, y);
                let hit = self.view.pointer(
                    retinue_ui::input::PointerPhase::Down(retinue_ui::input::Button::Primary),
                    Some((x, y)),
                );
                if !matches!(
                    hit,
                    Some(
                        Target::Minimum(_)
                            | Target::Maximum(_)
                            | Target::LevelMinimum
                            | Target::LevelMaximum
                    )
                ) {
                    self.interaction.focus = None;
                    self.interaction.selecting = false;
                }
                if hit == Some(Target::Drag) {
                    self.interaction.drag = Some((x, y, self.view.bounds.x, self.view.bounds.y));
                } else if let Some(
                    target @ (Target::Minimum(_)
                    | Target::Maximum(_)
                    | Target::LevelMinimum
                    | Target::LevelMaximum),
                ) = hit
                {
                    if self.interaction.focus == Some(target) {
                        self.view.edit_click(target, x, shift);
                        self.interaction.selecting = true;
                    } else if self.begin_edit(target, true) {
                        self.interaction.selecting = true;
                    }
                } else if let Some(hit) = hit {
                    self.click(hit);
                }
            }
            Input::Release => {
                self.view.pointer(
                    retinue_ui::input::PointerPhase::Up(retinue_ui::input::Button::Primary),
                    None,
                );
                self.interaction.drag = None;
                self.interaction.selecting = false;
            }
            Input::Scroll {
                x,
                y,
                amount,
                shift,
                alt,
            } => {
                let (x, y) = physical(x, y);
                match self.view.hit(x, y) {
                    Some(Target::Popup | Target::Option(_, _)) => {
                        let count = self
                            .interaction
                            .menu
                            .map_or(0, |menu| menu.choices(&self.model).count());
                        self.interaction.menu_scroll = (self.interaction.menu_scroll as i64
                            + amount.round() as i64)
                            .clamp(0, count.saturating_sub(self.view.visible_options) as i64)
                            as usize;
                    }
                    Some(
                        target @ (Target::Minimum(_)
                        | Target::Maximum(_)
                        | Target::LevelMinimum
                        | Target::LevelMaximum),
                    ) => {
                        if !self.begin_edit(target, false) {
                            return Damage::None;
                        }
                        if let Some(field) = self.field() {
                            let value = evaluate_model::parse_number(field)
                                .ok()
                                .flatten()
                                .unwrap_or(0.0);
                            *field = evaluate_model::number(
                                value
                                    - amount
                                        * if shift {
                                            5.0
                                        } else if alt {
                                            0.1
                                        } else {
                                            1.0
                                        },
                            );
                        }
                        if let Some(value) = self.field().cloned() {
                            self.view.begin_edit(target, &value, false);
                        }
                        self.change();
                    }
                    Some(Target::Results | Target::ListingCurrency(_) | Target::Listing(_)) => {
                        let total = self
                            .model
                            .results
                            .as_ref()
                            .map(|r| r.listings.len())
                            .unwrap_or(0);
                        self.interaction.scroll = (self.interaction.scroll as i64
                            + amount.round() as i64)
                            .clamp(0, total.saturating_sub(self.view.visible_listings) as i64)
                            as usize;
                    }
                    Some(Target::ItemScroll) | Some(Target::Modifier(_)) => {
                        self.interaction.item_scroll = (self.interaction.item_scroll as i64
                            + amount.round() as i64)
                            .clamp(0, self.view.item_rows.saturating_sub(1) as i64)
                            as usize;
                    }
                    _ => {}
                }
            }
            Input::Paste(text) => self.insert_text(&text),
            Input::Cut => {
                if self.interaction.focus.is_some() && self.view.editor.delete_selection() {
                    self.sync_editor();
                }
            }
            Input::Key {
                symbol,
                text,
                control,
                alt,
                shift,
            } => match symbol {
                0x65 | 0x45 if alt && !control => self.activate_requested = true,
                0xff1b => self.escape(),
                0xff0d | 0xff8d => {
                    if self.interaction.focus.is_none() && self.interaction.menu.is_none() {
                        self.search();
                    }
                }
                0xff09 | 0xfe20 => {
                    if self.interaction.menu.is_some() {
                        return Damage::None;
                    }
                    if let Some(next) = self.view.next_field(
                        self.interaction.focus,
                        shift || symbol == 0xfe20,
                        &self.model,
                    ) {
                        self.begin_edit(next, true);
                    }
                }
                _ if self.interaction.focus.is_some() => {
                    if self.view.editor.key(symbol, control, shift) {
                        self.sync_editor();
                    } else if !control
                        && !alt
                        && let Some(text) = text
                    {
                        self.insert_text(&text);
                    }
                }
                _ => {}
            },
        }
        // Keep the panel mapped until the compositor confirms keyboard release.
        if self.interaction.focus.is_some() {
            self.presentation.keyboard = true;
        }
        self.gate
            .interaction(self.model.visible || self.model.copying);
        Damage::Full
    }
    fn prepare(&mut self, size: (u32, u32), scale: u32) -> Result<Damage, String> {
        self.dpr = scale;
        let phase = (self
            .last_animation
            .saturating_duration_since(self.started)
            .as_secs_f32()
            * 0.6)
            % 2.0;
        self.view.prepare(
            Frame {
                size,
                scale: scale as f32 * self.preferences.scale,
                zoom: self.preferences.scale,
                phase: if phase > 1.0 { 2.0 - phase } else { phase },
            },
            &self.model,
            &self.interaction,
        )
    }

    fn paint_scene(&self) -> Option<retinue_render::surface::Snapshot> {
        self.view.paint_scene()
    }
    fn render_frame(&self) -> Option<retinue_render::RenderPass> {
        Some(self.view.begin_frame())
    }

    fn paint(
        &mut self,
        painter: &mut retinue_render::painter::Painter<'_>,
        _size: (u32, u32),
        _scale: u32,
        repair: &retinue_render::damage::Region,
    ) -> Result<(), String> {
        self.view.draw_prepared(painter, repair)
    }
}
