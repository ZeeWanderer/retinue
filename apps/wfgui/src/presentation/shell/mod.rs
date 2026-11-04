#[cfg(test)]
mod benchmark;
mod body_view;
mod composition;
mod dock_layout;
mod icons;
mod identity;
mod input;
mod navigation_view;
mod paint_view;
pub mod shell_model;
mod shell_view;
#[cfg(test)]
mod tests;
mod title_view;

pub use shell_view::ShellView;

use crate::{
    data::storage::Snapshot, presentation::foundry, presentation::foundry::Foundry,
    presentation::settings, presentation::settings::Settings, presentation::settings::rgb,
    presentation::style::focus_ring,
};
use icons::{Icon, Icons};
use input::PressOwner;
use retinue_render::{
    ImageFit, damage::Damage, painter::Painter, painter::TextLine, painter::text_width,
};
use retinue_ui::{
    Rect, desktop::accesskit, desktop::accesskit::ActionRequest, desktop::accesskit::Node,
    desktop::accesskit::NodeId, desktop::accesskit::Role, desktop::accesskit::TreeInfo,
    desktop::accesskit::TreeUpdate, desktop::winit::keyboard::ModifiersState,
    desktop::winit::window::ResizeDirection, drawing::PixelCanvas as Canvas,
    focus_visible::FocusVisible,
};
use std::{cell::Cell, time::Duration, time::Instant};

pub const BACKGROUND: [u8; 4] = rgb(0x101623);
const TEXT: [u8; 4] = rgb(0xd8dbea);
const MOTION: Duration = Duration::from_millis(170);
const FRAME: Duration = Duration::from_nanos(8_333_333);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Left,
    Smaller,
    Larger,
    Right,
    Minimize,
    Maximize,
    Close,
    Settings,
    Foundry,
    Player,
    Mastery,
    Inventory,
    Relics,
    Market,
    Builds,
}

impl Control {
    fn id(self) -> NodeId {
        NodeId(10 + self as u64)
    }
    fn icon(self, maximized: bool) -> Icon {
        match self {
            Self::Left => Icon::Left,
            Self::Right => Icon::Right,
            Self::Smaller => Icon::Smaller,
            Self::Larger => Icon::Larger,
            Self::Minimize => Icon::Minimize,
            Self::Close => Icon::Close,
            Self::Maximize if maximized => Icon::Restore,
            Self::Maximize => Icon::Maximize,
            Self::Settings => Icon::Settings,
            Self::Foundry => Icon::Foundry,
            Self::Player => Icon::Mastery,
            Self::Mastery => Icon::Mastery,
            Self::Inventory => Icon::Inventory,
            Self::Relics => Icon::Relics,
            Self::Market => Icon::Market,
            Self::Builds => Icon::Builds,
        }
    }
    fn page(self) -> Option<Page> {
        Some(match self {
            Self::Foundry => Page::Foundry,
            Self::Mastery | Self::Player => Page::Mastery,
            Self::Inventory => Page::Inventory,
            Self::Relics => Page::Relics,
            Self::Market => Page::Market,
            Self::Builds => Page::Builds,
            Self::Settings => Page::Settings,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Chrome(Control),
    Page(settings::Action),
    Foundry(foundry::Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Settings,
    Foundry,
    Mastery,
    Inventory,
    Relics,
    Market,
    Builds,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Geometry {
    pub title: Rect,
    pub left: Rect,
    pub center: Rect,
    pub right: Rect,
}

pub struct Shell {
    #[cfg(all(test, feature = "profiling"))]
    pub paint_profile: Cell<[Duration; 4]>,
    #[cfg(feature = "profiling")]
    reflow_profile: [Duration; 4],
    #[cfg(feature = "profiling")]
    tick_profile: [Duration; 4],
    pub preferences: crate::domain::preferences::Preferences,
    pub settings: Settings,
    pub foundry: Foundry,
    pub pages: crate::presentation::Pages,
    pub page: Page,
    pub close_requested: bool,
    pub geometry: Geometry,
    size: (u32, u32),
    display_scale: f32,
    left_collapsed: bool,
    right_collapsed: bool,
    right_available: bool,
    force_right: bool,
    compact_left: bool,
    maximized: bool,
    fullscreen: bool,
    pointer: (f64, f64),
    modifiers: ModifiersState,
    hover: Option<Control>,
    press: Option<PressOwner>,
    rail_focus: bool,
    focus: Option<Focus>,
    focus_visible: FocusVisible,
    window_focused: bool,
    hover_since: Instant,
    tooltip: bool,
    last_title_press: Option<(Instant, (f64, f64))>,
    window_action: Option<Control>,
    icons: Icons,
    composition: composition::Composition,
    render_host: retinue_render::RenderHost,
    preparation_deadline: Cell<Option<Instant>>,
}

impl Shell {
    pub fn new(snapshot: Option<Snapshot>, scale: f32) -> Result<Self, String> {
        let mut values = crate::data::preferences::Values::default();
        values.ui_scale = (scale * 100.0).round() as u16;
        Self::with_preferences(
            snapshot,
            crate::domain::preferences::Preferences::new(values),
        )
    }

    pub fn with_preferences(
        snapshot: Option<Snapshot>,
        preferences: crate::domain::preferences::Preferences,
    ) -> Result<Self, String> {
        let faces = retinue_render::painter::FontFaces::default();
        let mut shell = Self {
            #[cfg(all(test, feature = "profiling"))]
            paint_profile: Default::default(),
            #[cfg(feature = "profiling")]
            reflow_profile: [Duration::ZERO; 4],
            #[cfg(feature = "profiling")]
            tick_profile: [Duration::ZERO; 4],
            settings: Settings::with_faces(snapshot, preferences.scale(), &faces)?,
            foundry: Foundry::with_faces(preferences.clone(), &faces)?,
            pages: crate::presentation::Pages::with_faces(preferences.clone(), &faces)?,
            preferences,
            page: Page::Settings,
            close_requested: false,
            geometry: Geometry::default(),
            size: (1440, 900),
            display_scale: 1.0,
            left_collapsed: false,
            right_collapsed: false,
            right_available: true,
            force_right: false,
            compact_left: false,
            maximized: false,
            fullscreen: false,
            pointer: (0.0, 0.0),
            modifiers: ModifiersState::empty(),
            hover: None,
            press: None,
            rail_focus: false,
            focus: None,
            focus_visible: FocusVisible::default(),
            window_focused: false,
            hover_since: Instant::now(),
            tooltip: false,
            last_title_press: None,
            window_action: None,
            icons: Icons::default(),
            composition: composition::Composition::default(),
            render_host: retinue_render::RenderHost::new(retinue_render::HostPolicy::desktop()),
            preparation_deadline: Cell::new(None),
        };
        let host = &shell.render_host;
        shell.settings.set_render_host(host.clone());
        shell.foundry.set_render_host(host.clone());
        shell.pages.ui.set_render_host(host.clone());
        shell.pages.rail.set_render_host(host.clone());
        shell.composition.renderer.get_mut().set_host(host.clone());
        shell.foundry.set_active(false);
        shell.resize(shell.size, 1.0)?;
        Ok(shell)
    }

    fn scale(&self) -> f32 {
        self.settings.ui_scale * self.display_scale
    }
    fn px(&self, value: f32) -> u32 {
        (value * self.scale()).round().max(0.0) as u32
    }

    pub fn resize(&mut self, size: (u32, u32), display_scale: f64) -> Result<(), String> {
        self.size = size;
        self.display_scale = display_scale as f32;
        self.icons.prepare(self.scale())?;
        self.reflow(Instant::now());
        Ok(())
    }

    fn reflow(&mut self, now: Instant) -> Damage {
        #[cfg(feature = "profiling")]
        let start = Instant::now();
        self.compact_left = self.size.0 < self.px(720.0);
        let right_fits = self.size.0 >= self.px(142.0 + 400.0 + 520.0 + 26.0);
        self.right_available = right_fits || self.force_right;
        let previous = self.geometry;
        let mut damage = self.composition.update_at(
            composition::Layout {
                size: self.size,
                scale: self.scale(),
                left: u8::from(!self.left_collapsed) as f32,
                right: u8::from(!self.right_collapsed) as f32,
                right_fits,
                compact_left: self.compact_left,
                right_available: self.right_available,
                tooltip: self.tooltip,
                page: self.page,
            },
            now,
        );
        self.geometry = self.composition.geometry();
        if self.right_collapsed
            && self.geometry.right.width == 0
            && !self.composition.rail_animating(composition::RIGHT)
        {
            self.force_right = false;
            self.right_available = right_fits;
        }
        // Chrome is recorded at paint time, so resized panels need their new envelope too.
        for (old, new) in [
            (previous.title, self.geometry.title),
            (previous.left, self.geometry.left),
            (previous.center, self.geometry.center),
            (previous.right, self.geometry.right),
        ] {
            if old != new {
                damage = damage.union(Damage::rect(old)).union(Damage::rect(new));
            }
        }
        #[cfg(feature = "profiling")]
        let shell_done = Instant::now();
        self.settings.resize(
            (self.geometry.center.width, self.geometry.center.height),
            self.display_scale as f64,
        );
        #[cfg(feature = "profiling")]
        let settings_done = Instant::now();
        self.foundry.resize(
            (self.geometry.center.width, self.geometry.center.height),
            self.scale(),
        );
        #[cfg(feature = "profiling")]
        let foundry_done = Instant::now();
        self.pages.configure(
            (self.geometry.center.width, self.geometry.center.height),
            (self.geometry.right.width, self.geometry.right.height),
            self.scale(),
        );
        #[cfg(feature = "profiling")]
        {
            self.reflow_profile = [
                shell_done - start,
                settings_done - shell_done,
                foundry_done - settings_done,
                foundry_done.elapsed(),
            ];
        }
        damage
    }

    pub fn set_page(&mut self, page: Page) {
        self.cancel_pointer();
        self.foundry.close_popup();
        self.foundry.set_active(page == Page::Foundry);
        self.page = page;
        self.pages.set_page(page);
        self.rail_focus = false;
        self.settings.modal = None;
        self.set_focus(None);
        self.reflow(Instant::now());
        self.hover = self.hit(self.pointer);
    }

    pub fn show_market_rail(&mut self) {
        self.force_right = true;
        self.right_collapsed = false;
        self.reflow(Instant::now());
    }

    pub fn minimum_size(&self) -> (u32, u32) {
        (
            (1024.0 * self.settings.ui_scale).ceil().max(960.0) as u32,
            (600.0 * self.settings.ui_scale).ceil().max(600.0) as u32,
        )
    }

    pub fn set_collapsed(&mut self, left: bool, right: bool) {
        self.left_collapsed = left;
        self.right_collapsed = right;
        let now = Instant::now();
        self.composition.set_motion_enabled(false);
        self.reflow(now);
        self.composition.set_motion_enabled(true);
    }

    pub fn sample_animation(&mut self, left: bool, right: bool, elapsed: Duration) {
        let now = Instant::now();
        if left {
            self.activate(Control::Left, now);
        }
        if right {
            self.activate(Control::Right, now);
        }
        self.tick(now + elapsed);
    }

    fn controls(&self) -> Vec<(Control, Rect, bool)> {
        composition::CONTROLS
            .into_iter()
            .map(|control| {
                (
                    control,
                    self.composition.rect(composition::control_key(control)),
                    self.control_enabled(control),
                )
            })
            .collect()
    }

    fn control_enabled(&self, control: Control) -> bool {
        match control {
            Control::Left => !self.compact_left,
            Control::Smaller => self.settings.ui_scale > 0.75,
            Control::Larger => self.settings.ui_scale < 2.0,
            Control::Right => self.right_available,
            _ => true,
        }
    }

    fn label(&self, control: Control) -> String {
        if control == Control::Player {
            let profile = &self.foundry.snapshot.profile;
            let name = profile
                .as_ref()
                .map(|p| p.player_name.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Player");
            return match profile.as_ref().and_then(|p| p.player_level) {
                Some(rank) => format!("{name} - Mastery Rank {rank}"),
                None => format!("{name} - Player data unavailable"),
            };
        }
        match control {
            Control::Left if self.left_collapsed => "Show navigation rail",
            Control::Left => "Collapse navigation rail",
            Control::Right if !self.right_available => "Activity rail needs a wider window",
            Control::Right if self.right_collapsed => "Show activity rail",
            Control::Right => "Hide activity rail",
            Control::Smaller => "Decrease UI scale",
            Control::Larger => "Increase UI scale",
            Control::Minimize => "Minimize",
            Control::Close => "Close",
            Control::Maximize if self.maximized => "Restore",
            Control::Maximize => "Maximize",
            Control::Settings if self.settings.preference_error.is_some() => {
                "Preferences could not be saved; open Settings"
            }
            Control::Settings => "Settings",
            Control::Foundry => "Foundry",
            Control::Player => unreachable!(),
            Control::Mastery => "Mastery Helper",
            Control::Inventory => "Inventory",
            Control::Relics => "Relic Planner",
            Control::Market => "Market",
            Control::Builds => "Build Planner",
        }
        .into()
    }

    fn activate(&mut self, control: Control, now: Instant) {
        if self.settings.modal.is_some() || !self.control_enabled(control) {
            return;
        }
        self.tooltip = false;
        match control {
            Control::Left => {
                self.left_collapsed = !self.left_collapsed;
                self.reflow(now);
            }
            Control::Right => {
                self.right_collapsed = !self.right_collapsed;
                self.reflow(now);
            }
            Control::Smaller | Control::Larger => {
                let step = if control == Control::Smaller {
                    -5.0
                } else {
                    5.0
                };
                let scale =
                    ((self.settings.ui_scale * 100.0).round() + step).clamp(75.0, 200.0) / 100.0;
                self.settings.set_scale(scale);
                self.preferences.set_scale((scale * 100.0).round() as u16);
                self.window_action = Some(control);
                if let Err(error) = self.icons.prepare(self.scale()) {
                    self.settings.error = Some(error);
                }
                self.reflow(now);
            }
            Control::Close => self.close_requested = true,
            Control::Settings => {
                self.set_page(Page::Settings);
                self.set_focus(Some(Focus::Page(settings::Action::Refresh)));
            }
            Control::Foundry => {
                self.set_page(Page::Foundry);
                self.set_focus(Some(Focus::Foundry(foundry::Action::Search)));
            }
            Control::Player
            | Control::Mastery
            | Control::Inventory
            | Control::Relics
            | Control::Market
            | Control::Builds => self.set_page(control.page().unwrap()),
            Control::Minimize | Control::Maximize => self.window_action = Some(control),
        }
    }

    pub fn image_memory(&self) -> (usize, usize) {
        [
            self.foundry.memory(),
            self.pages.ui.raster_memory(),
            self.pages.rail.raster_memory(),
        ]
        .into_iter()
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    }

    pub fn render_memory(&self) -> retinue_render::HostStats {
        self.render_host.stats()
    }

    pub fn render_host(&self) -> retinue_render::RenderHost {
        self.render_host.clone()
    }

    pub fn maintenance_deadline(&self) -> Option<Instant> {
        self.render_host.maintenance_deadline()
    }

    pub fn maintain(&self, now: Instant) {
        self.render_host.maintain(now);
    }

    pub fn tick(&mut self, now: Instant) -> Damage {
        #[cfg(feature = "profiling")]
        let start = Instant::now();
        if self.page == Page::Settings {
            self.settings.set_image_memory(self.image_memory());
        }
        let tooltip = self.hover.is_some()
            && self.settings.modal.is_none()
            && now.saturating_duration_since(self.hover_since) >= Duration::from_millis(600);
        let mut changed = tooltip != self.tooltip;
        self.tooltip = tooltip;
        #[cfg(feature = "profiling")]
        let prepared = Instant::now();
        let layout = self.reflow(now);
        let hover = self.hit(self.pointer);
        if self.hover != hover {
            self.hover = hover;
            self.hover_since = now;
            self.tooltip = false;
            changed = true;
        }
        #[cfg(feature = "profiling")]
        let laid_out = Instant::now();
        let page = match self.page {
            Page::Settings => self.settings.tick(now),
            Page::Foundry => self.foundry.tick(now),
            _ => Damage::None,
        };
        #[cfg(feature = "profiling")]
        let page_done = Instant::now();
        let (pages, rail) = self.pages.tick(now);
        #[cfg(feature = "profiling")]
        {
            self.tick_profile = [
                prepared - start,
                laid_out - prepared,
                page_done - laid_out,
                page_done.elapsed(),
            ];
        }
        if changed
            || self
                .preparation_deadline
                .get()
                .is_some_and(|deadline| now >= deadline)
        {
            return Damage::Full;
        }
        let mut damage = layout.union(self.page_damage(page));
        if rail != Damage::None {
            damage = damage.union(Damage::rect(self.geometry.right));
        }
        if !matches!(self.page, Page::Foundry | Page::Settings) {
            damage = damage.union(self.page_damage(pages));
        }
        damage
    }

    pub fn deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        [
            self.preparation_deadline.get(),
            self.composition.deadline(now, FRAME),
            match self.page {
                Page::Settings => self.settings.deadline(),
                Page::Foundry => self.foundry.deadline(),
                _ => None,
            },
            self.pages.deadline(),
            (self.hover.is_some() && !self.tooltip && self.settings.modal.is_none())
                .then_some(self.hover_since + Duration::from_millis(600)),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn page_damage(&self, damage: Damage) -> Damage {
        self.composition.invalidate(composition::CENTER, damage)
    }

    pub fn image_damage(
        &self,
        images: &[(
            retinue_render::thumbnails::Key,
            retinue_render::resources::Resource<retinue_render::painter::RasterImage>,
        )],
    ) -> Damage {
        let keys = images.iter().map(|(key, _)| *key).collect();
        let mut damage = Damage::None;
        if !matches!(self.page, Page::Foundry | Page::Settings) {
            damage = self.page_damage(self.pages.ui.image_damage(&keys, &self.pages.media));
        }
        if let Some(r) = self
            .pages
            .rail
            .image_damage(&keys, &self.pages.media)
            .bounds((self.geometry.right.width, self.geometry.right.height))
        {
            damage = damage.union(Damage::rect(Rect {
                x: r.x + self.geometry.right.x,
                y: r.y + self.geometry.right.y,
                ..r
            }));
        }
        if images
            .iter()
            .any(|(key, _)| self.foundry.rank_image_key() == Some(*key))
        {
            damage = damage.union(Damage::rect(self.identity_bounds()));
        }
        damage
    }

    pub fn capture_bounds(&self, target: &str) -> Result<Rect, String> {
        let rect = match target {
            "window" => Rect {
                x: 0,
                y: 0,
                width: self.size.0,
                height: self.size.1,
            },
            "titlebar" => self.geometry.title,
            "left-rail" => self.geometry.left,
            "right-rail" => self.geometry.right,
            "center-rail" => self.geometry.center,
            "player" => self.identity_bounds(),
            "settings.scale" => {
                let buttons = self.controls();
                buttons
                    .iter()
                    .find(|(c, _, _)| *c == Control::Smaller)
                    .unwrap()
                    .1
                    .union(
                        buttons
                            .iter()
                            .find(|(c, _, _)| *c == Control::Larger)
                            .unwrap()
                            .1,
                    )
            }
            target => {
                let rect = if target.starts_with("foundry.") {
                    self.foundry.capture_bounds(target)?
                } else if target.starts_with("settings.") {
                    self.settings.capture_bounds(target)?
                } else if let Some(rect) = target
                    .strip_prefix("rail.")
                    .and_then(|key| self.pages.rail.bounds(key))
                {
                    return Ok(Rect {
                        x: rect.x + self.geometry.right.x,
                        y: rect.y + self.geometry.right.y,
                        ..rect
                    });
                } else {
                    self.pages
                        .ui
                        .bounds(target)
                        .ok_or_else(|| format!("Capture target is not visible: {target}"))?
                };
                Rect {
                    x: rect.x + self.geometry.center.x,
                    y: rect.y + self.geometry.center.y,
                    ..rect
                }
            }
        };
        if rect.is_empty() {
            Err("Capture target is not visible".into())
        } else {
            Ok(rect)
        }
    }

    pub fn layout_report(&self) -> serde_json::Value {
        serde_json::json!({"scale":self.scale(),"declaration":self.composition.declaration_report(),"nodes":self.composition.tree.borrow().scene().geometry().iter().map(|node| {
            serde_json::json!({"scope":node.key.namespace,"id":node.key.value,
                "parent":node.parent.map(|key|(key.namespace,key.value)),
                "layout":[node.layout.x,node.layout.y,node.layout.width,node.layout.height],
                "hit":[node.hit.x,node.hit.y,node.hit.width,node.hit.height],
                "paint":[node.paint.x,node.paint.y,node.paint.width,node.paint.height]})
        }).collect::<Vec<_>>()})
    }

    pub fn paint(&self, painter: &mut Painter<'_>) {
        self.prepare_paint();
        self.draw_prepared(painter);
    }

    pub fn prepare_paint(&self) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        use composition::*;
        let mut paints = Vec::new();
        for key in [TITLE, LEFT, CENTER, RIGHT, TOOLTIP] {
            if key == TOOLTIP && !self.tooltip {
                continue;
            }
            let mut canvas = Canvas::new(self.scale());
            match key {
                TITLE => self.paint_title(&mut canvas),
                LEFT => self.paint_navigation(&mut canvas),
                CENTER | RIGHT => {
                    let rect = self.composition.rect(key);
                    let rect = Rect { x: 0, y: 0, ..rect };
                    self.panel(&mut canvas, rect);
                    if key == RIGHT {
                        canvas.external(key, rect);
                    }
                }
                TOOLTIP => self.paint_tooltip(&mut canvas),
                _ => unreachable!(),
            }
            paints.push((key, canvas.commands));
        }
        for control in [
            Control::Left,
            Control::Smaller,
            Control::Larger,
            Control::Right,
            Control::Minimize,
            Control::Maximize,
            Control::Close,
        ] {
            let key = control_key(control);
            let rect = self.composition.rect(key);
            let mut canvas = Canvas::new(self.scale());
            self.paint_control(
                &mut canvas,
                control,
                Rect { x: 0, y: 0, ..rect },
                self.control_enabled(control),
            );
            paints.push((key, canvas.commands));
        }
        #[cfg(all(test, feature = "profiling"))]
        let recorded = Instant::now();
        self.composition.record(paints);
        match self.page {
            Page::Settings => self.settings.prepare_paint(self.focus_visible),
            Page::Foundry => self.foundry.prepare_paint(self.focus_visible),
            _ => self.pages.prepare_paint(),
        }
        if !self.geometry.right.is_empty() {
            self.pages.prepare_rail();
        }
        #[cfg(all(test, feature = "profiling"))]
        self.paint_profile.set([
            recorded - started,
            recorded.elapsed(),
            Duration::ZERO,
            Duration::ZERO,
        ]);
    }

    pub fn paint_scene(&self) -> retinue_render::surface::Snapshot {
        use composition::*;
        let mut scene = retinue_render::surface::Snapshot::new(
            self.size,
            retinue_render::Color::from_bgra(BACKGROUND),
        );
        scene.record_rendered(
            self.composition.tree.borrow().frame(),
            &self.composition.renderer.borrow(),
            retinue_render::RasterMode::Replay,
            |key, scene| {
                match key {
                    RIGHT => self.pages.record_rail(scene),
                    _ => match page_for_key(key).expect("only page hosts use live shell paint") {
                        Page::Settings => self.settings.record_surface(scene),
                        Page::Foundry => self.foundry.record_surface(scene),
                        _ => self.pages.record_surface(scene),
                    },
                }
                true
            },
        );
        scene
    }

    pub fn draw_prepared(&self, painter: &mut Painter<'_>) {
        use composition::*;
        let _pass = self.render_host.begin_pass(painter.thread_count());
        #[cfg(all(test, feature = "profiling"))]
        let updated = Instant::now();
        let tree = self.composition.tree.borrow();
        let mut renderer = self.composition.renderer.borrow_mut();
        renderer.prepare_for(
            tree.frame(),
            retinue_render::RasterMode::Replay,
            &painter.visible_regions(),
        );
        #[cfg(all(test, feature = "profiling"))]
        let prepared = Instant::now();
        renderer.draw_with(
            tree.frame(),
            painter,
            Rect {
                width: self.size.0,
                height: self.size.1,
                ..Rect::default()
            },
            |key, painter| match key {
                RIGHT => {
                    self.pages.draw_rail(painter);
                }
                _ => match page_for_key(key).expect("only page hosts use live shell paint") {
                    Page::Settings => self.settings.draw_prepared(painter),
                    Page::Foundry => self.foundry.draw_prepared(painter),
                    _ => self.pages.draw_prepared(painter),
                },
            },
        );
        #[cfg(all(test, feature = "profiling"))]
        self.paint_profile.set([
            self.paint_profile.get()[0],
            self.paint_profile.get()[1],
            prepared - updated,
            prepared.elapsed(),
        ]);
        self.preparation_deadline.set(
            self.render_host
                .has_deferred()
                .then(|| Instant::now() + FRAME),
        );
    }

    fn paint_title(&self, painter: &mut Canvas) {
        let scale = self.scale();
        let title = self.geometry.title;
        if let Some(gradient) = &self.icons.gradient {
            painter.draw_image(&gradient.image, 0, 0, gradient.width, gradient.height);
        }
        self.icon(
            painter,
            Icon::Mastery,
            Rect {
                x: self.px(7.0),
                y: self.px(7.0),
                width: self.px(26.0),
                height: self.px(26.0),
            },
        );
        self.text(
            painter,
            "Warframe",
            self.px(69.0),
            0,
            title.height,
            14.0,
            TEXT,
        );
        let smaller = self
            .composition
            .rect(composition::control_key(Control::Smaller));
        let larger = self
            .composition
            .rect(composition::control_key(Control::Larger));
        let label = format!("{}%", (self.settings.ui_scale * 100.0).round() as u32);
        let width = text_width(&self.settings.regular, &label, 13.0 * scale).round() as u32;
        self.text(
            painter,
            &label,
            (smaller.x + smaller.width + larger.x).saturating_sub(width) / 2,
            0,
            title.height,
            13.0,
            TEXT,
        );
    }

    fn paint_control(&self, painter: &mut Canvas, control: Control, rect: Rect, enabled: bool) {
        if self.hover == Some(control) && enabled || self.press == Some(PressOwner::Chrome(control))
        {
            fill(
                painter,
                rect,
                self.px(6.0),
                if control == Control::Close {
                    rgb(0xa73746)
                } else {
                    rgb(0x313a58)
                },
            );
        }
        self.icon_state(painter, control.icon(self.maximized), rect, enabled);
        if enabled
            && self
                .focus_visible
                .shows(self.focus == Some(Focus::Chrome(control)), false)
        {
            focus_ring(painter, rect, self.scale());
        }
    }

    fn paint_tooltip(&self, painter: &mut Canvas) {
        let scale = self.scale();
        let title = self.geometry.title;
        if self.tooltip {
            if let Some(control) = self.hover {
                let text = self.label(control);
                let width = text_width(&self.settings.regular, &text, 13.0 * scale).ceil() as u32
                    + self.px(16.0);
                let button = self
                    .controls()
                    .into_iter()
                    .find(|(c, _, _)| *c == control)
                    .unwrap()
                    .1;
                let rect = Rect {
                    x: button
                        .x
                        .min(self.size.0.saturating_sub(width + self.px(8.0))),
                    y: if control.page().is_some() {
                        button.y.saturating_sub(self.px(34.0))
                    } else {
                        title.height
                    },
                    width,
                    height: self.px(30.0),
                };
                fill(painter, rect, self.px(4.0), rgb(0x323c58));
                self.text(
                    painter,
                    &text,
                    rect.x + self.px(8.0),
                    rect.y,
                    rect.height,
                    13.0,
                    TEXT,
                );
            }
        }
    }

    fn paint_navigation(&self, painter: &mut Canvas) {
        let left = self.geometry.left;
        painter.viewport(Rect { x: 0, y: 0, ..left }, (0, 0), |p| {
            self.paint_identity(p);
            let controls = self.controls();
            for (control, label) in [
                (Control::Foundry, "Foundry"),
                (Control::Mastery, "Mastery\nHelper"),
                (Control::Inventory, "Inventory"),
                (Control::Relics, "Relic\nPlanner"),
                (Control::Market, "Market"),
                (Control::Builds, "Build\nPlanner"),
                (Control::Settings, "Settings"),
            ]
            .into_iter()
            {
                let bounds = controls.iter().find(|(c, _, _)| *c == control).unwrap().1;
                let y = bounds.y - left.y;
                let rect = Rect {
                    x: self.px(8.0),
                    y,
                    width: self.px(36.0),
                    height: bounds.height,
                };
                let active = control.page() == Some(self.page);
                let hovered = self.hover == Some(control);
                let color = if active {
                    TEXT
                } else if hovered {
                    crate::presentation::style::NAV_HOVER
                } else {
                    crate::presentation::style::NAV_TEXT
                };
                self.icon_state(p, control.icon(false), rect, active || hovered);
                if active || hovered {
                    fill(
                        p,
                        Rect {
                            x: left.width.saturating_sub(self.px(3.0)),
                            y,
                            width: self.px(3.0),
                            height: bounds.height,
                        },
                        0,
                        color,
                    );
                }
                if left.width > self.px(100.0) {
                    let label = if bounds.height < self.px(38.0) {
                        label.split('\n').next().unwrap_or(label)
                    } else {
                        label
                    };
                    p.draw_text(
                        &self.settings.regular,
                        self.px(46.0),
                        y + if label.contains('\n') {
                            self.px(3.0)
                        } else {
                            bounds.height.saturating_sub(self.px(18.0)) / 2
                        },
                        14.0 * self.scale(),
                        label,
                        color,
                    );
                }
                if control == Control::Settings && self.settings.preference_error.is_some() {
                    fill(
                        p,
                        Rect {
                            x: rect.x + self.px(26.0),
                            y: y + self.px(5.0),
                            width: self.px(6.0),
                            height: self.px(6.0),
                        },
                        self.px(3.0),
                        crate::presentation::style::BAD,
                    );
                }
            }
        });
        for (control, bounds, _) in self.controls() {
            if control.page().is_some()
                && self
                    .focus_visible
                    .shows(self.focus == Some(Focus::Chrome(control)), false)
            {
                focus_ring(
                    painter,
                    Rect {
                        x: bounds.x - left.x,
                        y: bounds.y - left.y,
                        ..bounds
                    },
                    self.scale(),
                );
            }
        }
    }

    fn panel(&self, painter: &mut Canvas, rect: Rect) {
        let radius = self.px(10.0);
        painter.fill_rounded_rect(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            radius,
            settings::BACKGROUND,
        );
        let lower = Rect {
            y: rect.y + radius,
            height: rect.height.saturating_sub(radius),
            ..rect
        };
        painter.fill_rounded_rect(
            lower.x,
            lower.y,
            lower.width,
            lower.height,
            0,
            settings::BACKGROUND,
        );
    }

    fn icon(&self, painter: &mut Canvas, icon: Icon, rect: Rect) {
        self.icon_state(painter, icon, rect, true);
    }

    fn icon_state(&self, painter: &mut Canvas, icon: Icon, rect: Rect, enabled: bool) {
        let image = self.icons.get(icon, enabled);
        painter.draw_image(
            &image.image,
            rect.x + rect.width.saturating_sub(image.width) / 2,
            rect.y + rect.height.saturating_sub(image.height) / 2,
            image.width,
            image.height,
        );
    }

    fn text(
        &self,
        painter: &mut Canvas,
        text: &str,
        x: u32,
        y: u32,
        height: u32,
        size: f32,
        color: [u8; 4],
    ) {
        painter.draw_text_vertically_centered(
            &self.settings.regular,
            TextLine::new(x, y, height),
            size * self.scale(),
            text,
            color,
        );
    }

    pub fn accessibility(&self) -> TreeUpdate {
        let mut update = match self.page {
            Page::Settings => self.settings.accessibility(),
            Page::Foundry => self.foundry.accessibility(),
            _ => self.pages.ui.accessibility(2),
        };
        for (_, node) in &mut update.nodes {
            if let Some(r) = node.bounds() {
                node.set_bounds(accesskit::Rect {
                    x0: r.x0 + self.geometry.center.x as f64,
                    y0: r.y0 + self.geometry.center.y as f64,
                    x1: r.x1 + self.geometry.center.x as f64,
                    y1: r.y1 + self.geometry.center.y as f64,
                });
            }
        }
        let mut children = Vec::new();
        if self.settings.modal.is_none() && !self.foundry.popup_open() {
            for (control, rect, enabled) in self.controls() {
                let mut node = Node::new(Role::Button);
                node.set_label(self.label(control));
                node.set_bounds(access_rect(rect));
                if enabled {
                    node.add_action(accesskit::Action::Click);
                    node.add_action(accesskit::Action::Focus);
                } else {
                    node.set_disabled();
                }
                children.push(control.id());
                update.nodes.push((control.id(), node));
            }
            if let Some(Focus::Chrome(control)) = self.focus {
                update.focus = control.id();
            }
        }
        children.push(NodeId(2));
        let mut rail = self.pages.rail.accessibility(3);
        for (_, node) in &mut rail.nodes {
            if let Some(r) = node.bounds() {
                node.set_bounds(accesskit::Rect {
                    x0: r.x0 + self.geometry.right.x as f64,
                    y0: r.y0 + self.geometry.right.y as f64,
                    x1: r.x1 + self.geometry.right.x as f64,
                    y1: r.y1 + self.geometry.right.y as f64,
                });
            }
        }
        if self.rail_focus {
            update.focus = rail.focus;
        }
        update.nodes.extend(rail.nodes);
        children.push(NodeId(3));
        let mut root = Node::new(Role::Window);
        root.set_label("Warframe");
        root.set_children(children);
        update.nodes.push((NodeId(1), root));
        update.tree = Some(TreeInfo::new(NodeId(1)));
        update
    }

    pub fn access(&mut self, request: ActionRequest) {
        if matches!(
            request.action,
            accesskit::Action::Focus | accesskit::Action::Click | accesskit::Action::SetValue
        ) {
            self.focus_visible.keyboard_input();
        }
        if let Some((control, _, enabled)) = self
            .controls()
            .into_iter()
            .find(|(c, _, _)| c.id() == request.target_node)
        {
            if enabled && self.settings.modal.is_none() && !self.foundry.popup_open() {
                match request.action {
                    accesskit::Action::Click => self.activate(control, Instant::now()),
                    accesskit::Action::Focus => self.set_focus(Some(Focus::Chrome(control))),
                    _ => (),
                }
            }
        } else {
            if self.pages.rail.access(request.clone(), 3) {
                self.rail_focus = true;
                return;
            }
            match self.page {
                Page::Settings => {
                    self.settings.access(request);
                    self.focus = self.settings.focus.map(Focus::Page);
                }
                Page::Foundry => {
                    self.foundry.access(request);
                    self.focus = self.foundry.focus.map(Focus::Foundry);
                }
                _ => {
                    self.pages.ui.access(request, 2);
                }
            }
        }
    }
}

fn fill(p: &mut Canvas, r: Rect, radius: u32, color: [u8; 4]) {
    p.fill_rounded_rect(r.x, r.y, r.width, r.height, radius, color);
}

fn access_rect(r: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: r.x as f64,
        y0: r.y as f64,
        x1: (r.x + r.width) as f64,
        y1: (r.y + r.height) as f64,
    }
}
