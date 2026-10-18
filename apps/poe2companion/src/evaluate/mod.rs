mod artwork;
pub mod model;
mod view;
mod worker;

use model::{Model, Presentation};
use retinue_client::{
    paths::{Base, Game, Paths, create_private_dir_all},
    reload::Gate,
};
use retinue_overlay::{
    focus::FocusDetector,
    surface::{Application, Input},
    window_input::{Binding, ModMask, WindowInput},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use view::{Frame, Interaction, Menu, Target, View};
use worker::{Job, Reply, Worker};

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    league: String,
    scale: f32,
    status: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            league: String::new(),
            scale: 1.0,
            status: "securable".into(),
        }
    }
}
impl Preferences {
    fn path() -> PathBuf {
        Paths::from_env()
            .game_directory(Base::Config, Game::Poe2)
            .join("companion/evaluate.json")
    }
    fn load() -> Self {
        let mut preferences = std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .unwrap_or_default();
        if !preferences.scale.is_finite() {
            preferences.scale = 1.0;
        }
        preferences.scale = preferences.scale.clamp(0.75, 2.0);
        preferences
    }
    fn save(&self) -> Result<(), String> {
        let path = Self::path();
        create_private_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .and_then(|_| std::fs::rename(&tmp, &path))
            .map_err(|e| e.to_string())
    }
}

pub struct Service {
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}
impl Service {
    pub fn start(
        updates: watch::Receiver<Option<crate::Observation>>,
        stopping: Arc<AtomicBool>,
        gate: Gate,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let local = stop.clone();
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
                    let mut model = Model::new(preferences.league.clone());
                    model.status = preferences.status.clone();
                    let app = Overlay {
                        model,
                        open_after_search: false,
                        preferences,
                        view: View::new()?,
                        interaction: Interaction::default(),
                        worker: Worker::start()?,
                        updates,
                        focus: FocusDetector::connect("steam_app_2694490").ok(),
                        window_input,
                        game_window: None,
                        input_error: None,
                        activate_requested: false,
                        return_focus_requested: false,
                        pending_copy: None,
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
                    retinue_overlay::surface::run("poe2companion", Box::new(app), local)
                })();
                gate.interaction(false);
                if let Err(error) = result {
                    eprintln!("poe2companion: overlay.unavailable: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            thread: Some(thread),
            stop,
        })
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Overlay {
    model: Model,
    open_after_search: bool,
    preferences: Preferences,
    view: View,
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
        if self.model.loading {
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
            Ok(request) => self.send(Job::Search(self.model.generation, request)),
            Err(error) => self.model.error = Some(error),
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
        if self.interaction.focus.is_some() && self.view.editor.insert(text, model::numeric_edit) {
            self.sync_editor();
        }
    }
    fn change(&mut self) {
        self.open_after_search = false;
        self.model.invalidate();
        self.interaction.scroll = 0;
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
                let choice = self
                    .model
                    .evaluation
                    .as_ref()
                    .and_then(|e| e.search_options.get(menu.key()))
                    .and_then(|options| options.get(index))
                    .cloned();
                if let Some(choice) = choice {
                    let refresh = self.model.results.is_some();
                    match menu {
                        Menu::Status => {
                            self.model.status = choice.id.unwrap_or_else(|| "any".into())
                        }
                        Menu::Rarity => self.model.rarity = choice.id,
                        Menu::Currency => self.model.currency = choice.id,
                        Menu::Indexed => self.model.indexed = choice.id,
                        Menu::Settings => {}
                    }
                    self.change();
                    self.save_preferences();
                    if refresh && menu != Menu::Rarity {
                        self.search();
                    }
                }
                self.interaction.menu = None;
            }
            Target::Sort => {
                self.model.descending = !self.model.descending;
                self.change();
                self.search();
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
            Target::League => {
                let was_open = self.interaction.league_open;
                self.interaction.close_popup();
                self.interaction.league_open = !was_open;
            }
            Target::LeagueOption(index) => {
                if let Some(league) = self
                    .model
                    .evaluation
                    .as_ref()
                    .and_then(|e| e.leagues.get(index))
                {
                    self.model.league = league.id.clone();
                    self.change();
                    self.save_preferences();
                    self.predict();
                }
                self.interaction.league_open = false;
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
    fn pointer_regions(&self) -> Vec<retinue_overlay::Rect> {
        self.view.pointer_regions()
    }
    fn tick(&mut self) -> bool {
        if self.stopping.load(Ordering::Acquire) {
            self.local.store(true, Ordering::Release);
            return false;
        }
        let observation = self.updates.borrow().clone();
        let pid = observation
            .as_ref()
            .and_then(|o| o.process.as_ref())
            .map(|p| p.pid);
        let mut dirty = false;
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
            dirty = true;
        }
        if let Some((generation, pid, started)) = self.pending_copy {
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
                    let auto_search = id == self.model.generation && e.auto_search();
                    let current = id == self.model.generation;
                    let previous = self.model.league.clone();
                    self.model.evaluated(id, e);
                    if self.model.league != previous {
                        self.save_preferences();
                    }
                    if current {
                        self.predict();
                    }
                    if auto_search {
                        self.search();
                    }
                }
                Reply::Searched(id, r) => {
                    if cfg!(debug_assertions) {
                        eprintln!("poe2companion: evaluate.results: id={id} total={}", r.total);
                    }
                    self.model.searched(id, r);
                    if id == self.model.generation && self.open_after_search {
                        self.open_after_search = false;
                        self.click(Target::Trade);
                    }
                }
                Reply::Predicted(id, result) => {
                    if let Err(error) = &result {
                        eprintln!("poe2companion: evaluate.prediction: {error}");
                    }
                    self.model.predicted(id, result.ok().flatten());
                }
                Reply::Error(id, error) => {
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
                    }
                }
            }
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
        if self.visible()
            && self.model.loading
            && self.last_animation.elapsed() > Duration::from_millis(70)
        {
            self.last_animation = Instant::now();
            dirty = true;
        }
        if self.interaction.hover.is_some()
            && !self.interaction.tooltip
            && self.hover_since.elapsed() >= Duration::from_millis(600)
        {
            self.interaction.tooltip = true;
            dirty = true;
        }
        if self.visible() && self.interaction.focus.is_some() {
            dirty |= self.view.editor.tick();
        }
        dirty
    }
    fn input(&mut self, event: Input) -> bool {
        let physical = |x: f64, y: f64| (x * self.dpr as f64, y * self.dpr as f64);
        match event {
            Input::FocusLost => {
                self.return_focus_requested = false;
                if cfg!(debug_assertions) {
                    eprintln!("poe2companion: evaluate.focus_lost");
                }
                self.presentation.blur();
                self.last_focus = Instant::now() - Duration::from_secs(1);
                self.interaction.focus = None;
                self.interaction.selecting = false;
                self.interaction.drag = None;
                self.interaction.league_open = false;
                self.interaction.close_popup();
                self.interaction.tooltip = false;
            }
            Input::PointerLeave => {
                self.return_focus_requested = self.presentation.keyboard;
                self.interaction.leave_panel();
            }
            Input::Motion(x, y) => {
                let (x, y) = physical(x, y);
                let previous = self.interaction.hover;
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
                return previous != self.interaction.hover
                    || self.interaction.drag.is_some()
                    || self.interaction.selecting;
            }
            Input::Press { x, y, shift } => {
                let (x, y) = physical(x, y);
                let hit = self.view.hit(x, y);
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
                            .and_then(|menu| {
                                self.model
                                    .evaluation
                                    .as_ref()
                                    .and_then(|e| e.search_options.get(menu.key()))
                            })
                            .map_or(0, Vec::len);
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
                            return false;
                        }
                        if let Some(field) = self.field() {
                            let value = model::parse_number(field).ok().flatten().unwrap_or(0.0);
                            *field = model::number(
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
                    Some(Target::Results) => {
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
                    if self.interaction.focus.is_none()
                        && self.interaction.menu.is_none()
                        && !self.interaction.league_open
                    {
                        self.search();
                    }
                }
                0xff09 | 0xfe20 => {
                    if self.interaction.menu.is_some() || self.interaction.league_open {
                        return false;
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
        true
    }
    fn paint(&mut self, pixels: &mut [u8], size: (u32, u32), scale: u32) -> Result<(), String> {
        self.dpr = scale;
        let phase = (self.started.elapsed().as_secs_f32() * 0.6) % 2.0;
        self.view.paint(
            pixels,
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
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum PreviewState {
    Ready,
    Predicting,
    Estimate,
    EstimateError,
    Searching,
    Results,
    ListingPreview,
    Empty,
    SearchError,
}

pub fn preview(
    output: &std::path::Path,
    input: Option<&std::path::Path>,
    scale: f32,
    (width, height): (u32, u32),
    full_frame: bool,
    state: PreviewState,
) -> Result<(), String> {
    if !(640..=7680).contains(&width)
        || !(640..=4320).contains(&height)
        || !scale.is_finite()
        || !(0.5..=3.0).contains(&scale)
    {
        return Err("invalid preview dimensions or scale".into());
    }
    let source = if let Some(path) = input {
        std::fs::read(path).map_err(|e| e.to_string())?
    } else {
        include_bytes!("../../tests/fixtures/evaluate-preview.json").to_vec()
    };
    let data: serde_json::Value = serde_json::from_slice(&source).map_err(|e| e.to_string())?;
    let evaluation: model::Evaluation =
        serde_json::from_value(data["evaluation"].clone()).map_err(|e| e.to_string())?;
    let mut model = Model::new(evaluation.league.clone());
    model.evaluated(0, evaluation);
    if let Some(profile) = data.get("profile") {
        model.apply_profile(serde_json::from_value(profile.clone()).map_err(|e| e.to_string())?);
    }
    model.visible = true;
    match state {
        PreviewState::Ready => {}
        PreviewState::Predicting => model.predicting = true,
        PreviewState::Estimate | PreviewState::EstimateError => {
            model.estimate = Some(
                serde_json::from_value(data.get("estimate").cloned().unwrap_or_else(|| {
                    serde_json::json!({"amount":40,"currency":"exalted",
                    "uncertainty":{"low":30,"high":50,"confidence":75}})
                }))
                .map_err(|e| e.to_string())?,
            );
            if matches!(state, PreviewState::EstimateError) {
                model.error = Some("Trade service unavailable".into());
            }
        }
        PreviewState::Searching => model.loading = true,
        PreviewState::Results | PreviewState::ListingPreview => {
            let results = data.get("results").ok_or("fixture has no results")?;
            model.results =
                Some(serde_json::from_value(results.clone()).map_err(|e| e.to_string())?);
        }
        PreviewState::Empty => model.results = Some(Default::default()),
        PreviewState::SearchError => model.error = Some("Trade service unavailable".into()),
    }
    let mut view = View::new()?;
    view.preview_time = Some(1791885600);
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut interaction = Interaction::default();
    if matches!(state, PreviewState::ListingPreview) {
        if model
            .results
            .as_ref()
            .is_none_or(|results| results.listings.len() < 2 || results.listings[1].item.is_none())
        {
            return Err("listing preview needs a second listing with item details".into());
        }
        interaction.hover = Some(Target::Listing(1));
        interaction.tooltip = true;
    }
    interaction.menu = match data["menu"].as_str() {
        Some("settings") => Some(Menu::Settings),
        Some("rarity") => Some(Menu::Rarity),
        Some("status") => Some(Menu::Status),
        Some("currency") => Some(Menu::Currency),
        Some("indexed") => Some(Menu::Indexed),
        _ => None,
    };
    view.paint(
        &mut pixels,
        Frame {
            size: (width, height),
            scale,
            zoom: scale,
            phase: 0.4,
        },
        &model,
        &interaction,
    )?;
    if let Some(edit) = data.get("editor") {
        let index = edit["modifier"]
            .as_u64()
            .ok_or("editor.modifier must be an index")? as usize;
        let field = model
            .filters
            .get_mut(index)
            .ok_or("editor.modifier is out of range")?;
        field.enabled = true;
        let (target, value) = match edit["bound"].as_str() {
            Some("minimum") => (Target::Minimum(index), &field.min),
            Some("maximum") => (Target::Maximum(index), &field.max),
            _ => return Err("editor.bound must be minimum or maximum".into()),
        };
        interaction.focus = Some(target);
        view.begin_edit(target, value, edit["select_all"].as_bool().unwrap_or(false));
        view.paint(
            &mut pixels,
            Frame {
                size: (width, height),
                scale,
                zoom: scale,
                phase: 0.4,
            },
            &model,
            &interaction,
        )?;
    }
    let image = retinue_overlay::painter::straight_rgba(&pixels, width, height)?;
    let rect = view.bounds;
    if full_frame {
        image.save(output).map_err(|e| e.to_string())?;
        let metadata = serde_json::json!({
            "screen": {"width": width, "height": height}, "scale": scale,
            "state": format!("{state:?}"), "fixture": input,
            "bounds": {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height},
        });
        std::fs::write(
            output.with_extension("json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .map_err(|e| e.to_string())
    } else {
        retinue_overlay::image::imageops::crop_imm(&image, rect.x, rect.y, rect.width, rect.height)
            .to_image()
            .save(output)
            .map_err(|e| e.to_string())
    }
}
