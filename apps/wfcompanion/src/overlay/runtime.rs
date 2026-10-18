use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::delegate_compositor;
use smithay_client_toolkit::delegate_layer;
use smithay_client_toolkit::delegate_output;
use smithay_client_toolkit::delegate_pointer;
use smithay_client_toolkit::delegate_registry;
use smithay_client_toolkit::delegate_seat;
use smithay_client_toolkit::delegate_shm;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopSignal};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use super::renderer::{FrameKey, LOADING_FRAME_INTERVAL, Renderer};
use super::scene::Presentation;
use super::screens::{STATUS_HEIGHT, STATUS_INSET, STATUS_WIDTH, StatusView};
use crate::UiEvent;
use crate::incident;
use crate::runtime::presentation;
use crate::ui::{HitTarget, Rect};
use retinue_overlay::focus::FocusDetector;
use retinue_overlay::layer::OverlayLayerState;
use retinue_overlay::painter::Painter;

const STATUS_SURFACE_WIDTH: u32 = STATUS_WIDTH + STATUS_INSET;
const STATUS_SURFACE_HEIGHT: u32 = STATUS_HEIGHT + STATUS_INSET;
const EVENT_INTERVAL: Duration = Duration::from_millis(50);
const FOCUS_INTERVAL: Duration = Duration::from_millis(100);
const MAX_SURFACE_BUFFERS: usize = 3;
const DEBUG_HUD: bool = cfg!(debug_assertions);

pub(crate) fn run(
    events: &presentation::Receiver,
    relic: crate::relic::Sender,
    daemon: crate::daemon::OutboundSender,
    shortcut: crate::shortcut::Controller,
    stopping: Arc<AtomicBool>,
    reload: retinue_client::reload::Gate,
) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&connection)?;
    let queue_handle = event_queue.handle();
    let mut event_loop: EventLoop<Overlay> = EventLoop::try_new()?;
    WaylandSource::new(connection.clone(), event_queue).insert(event_loop.handle())?;

    let compositor = CompositorState::bind(&globals, &queue_handle)?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle)?;
    let shm = Shm::bind(&globals, &queue_handle)?;
    let surface = compositor.create_surface(&queue_handle);
    let layer = layer_shell.create_layer_surface(
        &queue_handle,
        surface,
        Layer::Overlay,
        Some("wfcompanion"),
        None,
    );
    apply_layer_state(&layer, false);
    let empty_input_region = Region::new(&compositor)?;
    let interactive_input_region = Region::new(&compositor)?;
    interactive_input_region.add(0, 0, i32::MAX, i32::MAX);
    layer
        .wl_surface()
        .set_input_region(Some(empty_input_region.wl_region()));
    layer.commit();

    let pool = SlotPool::new(
        (STATUS_SURFACE_WIDTH * STATUS_SURFACE_HEIGHT * 4) as usize,
        &shm,
    )?;
    let renderer = Renderer::load()?;
    let loop_signal = event_loop.get_signal();
    let mut overlay = Overlay {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &queue_handle),
        output_state: OutputState::new(&globals, &queue_handle),
        compositor,
        shm,
        layer,
        empty_input_region,
        interactive_input_region,
        themed_pointer: None,
        renderer,
        asset_loader: super::assets::Loader::spawn(),
        pool,
        buffers: Vec::new(),
        width: STATUS_SURFACE_WIDTH,
        height: STATUS_SURFACE_HEIGHT,
        scale: 1,
        configured: false,
        closed: false,
        configure_pending: true,
        mapped: false,
        redraw: Redraw::Full,
        overlay_enabled: true,
        hud: HudVisibility::for_build(DEBUG_HUD),
        contextual_surface: false,
        warframe_active: false,
        warframe_pid: None,
        connected: false,
        daemon_version: String::from("unknown"),
        player: presentation::PlayerStatus::default(),
        connection_error: None,
        relic_scene: None,
        relic_context: None,
        suggestion_dismissed: false,
        last_loading_frame: Instant::now() - LOADING_FRAME_INTERVAL,
        frame_pending: false,
        interaction_active: false,
        reload,
        shortcut_scope: false,
        pending_asset_refreshes: super::assets::PendingRefreshes::default(),
        reported_asset_issues: None,
        presentation: Presentation::default(),
        relic,
        daemon,
        shortcut,
        stopping,
        focus: FocusDetector::connect("steam_app_230410").ok(),
        last_focus_check: Instant::now() - FOCUS_INTERVAL,
        loop_signal,
    };

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        while !overlay.stopping.load(Ordering::Relaxed) && !overlay.closed {
            event_loop.dispatch(overlay.dispatch_interval(), &mut overlay)?;
            overlay.tick(&queue_handle, events);
        }
        if overlay.closed {
            Err("compositor closed overlay surface".into())
        } else {
            Ok(())
        }
    })();
    overlay.shortcut.set_enabled(false);
    overlay.unmap();
    result
}

fn copy_frame_rect(source: &[u8], target: &mut [u8], frame_width: u32, bounds: Rect) {
    let stride = frame_width as usize * 4;
    let row_bytes = bounds.width as usize * 4;
    let x = bounds.x as usize * 4;
    for row in bounds.y as usize..(bounds.y + bounds.height) as usize {
        let start = row * stride + x;
        target[start..start + row_bytes].copy_from_slice(&source[start..start + row_bytes]);
    }
}

struct Overlay {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    shm: Shm,
    layer: LayerSurface,
    empty_input_region: Region,
    interactive_input_region: Region,
    themed_pointer: Option<ThemedPointer>,
    renderer: Renderer,
    asset_loader: super::assets::Loader,
    pool: SlotPool,
    buffers: Vec<SurfaceBuffer>,
    width: u32,
    height: u32,
    scale: i32,
    configured: bool,
    closed: bool,
    configure_pending: bool,
    mapped: bool,
    redraw: Redraw,
    overlay_enabled: bool,
    hud: HudVisibility,
    contextual_surface: bool,
    warframe_active: bool,
    warframe_pid: Option<u32>,
    connected: bool,
    daemon_version: String,
    player: presentation::PlayerStatus,
    connection_error: Option<String>,
    relic_scene: Option<(crate::relic::Scene, Option<Instant>, Instant)>,
    relic_context: Option<crate::relic::Context>,
    suggestion_dismissed: bool,
    last_loading_frame: Instant,
    frame_pending: bool,
    interaction_active: bool,
    reload: retinue_client::reload::Gate,
    shortcut_scope: bool,
    pending_asset_refreshes: super::assets::PendingRefreshes,
    reported_asset_issues: Option<Vec<Value>>,
    presentation: Presentation,
    relic: crate::relic::Sender,
    daemon: crate::daemon::OutboundSender,
    shortcut: crate::shortcut::Controller,
    stopping: Arc<AtomicBool>,
    focus: Option<FocusDetector>,
    last_focus_check: Instant,
    loop_signal: LoopSignal,
}

struct SurfaceBuffer {
    buffer: Buffer,
    frame_key: Option<FrameKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Redraw {
    None,
    Loading,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HudVisibility {
    Startup,
    Shown,
    Hidden,
}

impl HudVisibility {
    fn for_build(debug: bool) -> Self {
        if debug { Self::Startup } else { Self::Hidden }
    }

    fn visible(self) -> bool {
        self != Self::Hidden
    }

    fn set_visible(&mut self, visible: bool) {
        *self = if visible { Self::Shown } else { Self::Hidden };
    }

    fn observe_phase(&mut self, phase: Option<&str>) -> bool {
        if *self == Self::Startup && phase == Some("game") {
            *self = Self::Hidden;
            true
        } else {
            false
        }
    }
}

impl Overlay {
    fn report_asset_issues(&mut self, issues: Vec<Value>) {
        if self.reported_asset_issues.as_ref() == Some(&issues) {
            return;
        }
        for issue in &issues {
            if self
                .reported_asset_issues
                .as_ref()
                .is_none_or(|previous| !previous.contains(issue))
            {
                incident::warn(
                    "overlay.asset_decode_failed",
                    format!("id={} error={}", issue["identity"], issue["reason"]),
                );
            }
        }
        crate::daemon::report_diagnostics(&self.daemon, issues.clone());
        self.reported_asset_issues = Some(issues);
    }

    fn dispatch_interval(&self) -> Duration {
        if self.loading_active() && !self.frame_pending {
            EVENT_INTERVAL
                .min(LOADING_FRAME_INTERVAL.saturating_sub(self.last_loading_frame.elapsed()))
        } else {
            EVENT_INTERVAL
        }
    }

    fn loading_active(&self) -> bool {
        self.overlay_enabled
            && self.warframe_active
            && self
                .relic_scene
                .as_ref()
                .is_some_and(|(scene, _, _)| matches!(scene, crate::relic::Scene::Reading))
    }

    fn request_full_redraw(&mut self) {
        self.redraw = Redraw::Full;
    }

    fn request_loading_redraw(&mut self) {
        if self.redraw == Redraw::None {
            self.redraw = Redraw::Loading;
        }
    }

    fn tick(&mut self, queue_handle: &QueueHandle<Self>, events: &presentation::Receiver) {
        self.apply_events(events);
        self.expire_scene();
        if let Some(assets) = self.asset_loader.poll() {
            self.report_asset_issues(assets.issues.clone());
            self.renderer.install_scene_assets(assets);
            for buffer in &mut self.buffers {
                buffer.frame_key = None;
            }
            self.request_full_redraw();
        }
        self.update_focus();
        self.sync_shortcut_scope();
        self.animate_loading();
        self.sync_surface(queue_handle);
    }

    fn apply_events(&mut self, events: &presentation::Receiver) {
        for event in events.drain(32) {
            match event {
                UiEvent::Connected(version) => {
                    self.connected = true;
                    self.connection_error = None;
                    self.daemon_version = version;
                    self.request_full_redraw();
                }
                UiEvent::Disconnected(error) => {
                    self.connected = false;
                    self.connection_error = Some(error);
                    self.request_full_redraw();
                }
                UiEvent::Player(player) => {
                    if self.hud.observe_phase(player.phase.as_deref()) {
                        incident::info("overlay.hud", "visible=false reason=game_started");
                    }
                    self.warframe_pid = player.pid;
                    self.player = player;
                    self.request_full_redraw();
                }
                UiEvent::AssetRefreshed(refresh) => {
                    let applied = self
                        .relic_scene
                        .as_mut()
                        .and_then(|(scene, _, _)| scene.apply_asset_refresh(&refresh));
                    match applied {
                        Some(changed) => {
                            if changed {
                                let scene = &self.relic_scene.as_ref().unwrap().0;
                                self.asset_loader.request(scene.clone());
                                self.request_full_redraw();
                            }
                        }
                        None => {
                            self.pending_asset_refreshes.insert(refresh);
                        }
                    }
                }
                UiEvent::OverlayVisible(visible) => {
                    self.overlay_enabled = visible;
                    incident::info("overlay.visibility", format!("enabled={visible}"));
                    self.request_full_redraw();
                }
                UiEvent::HudVisible(visible) => {
                    self.hud.set_visible(visible);
                    incident::info("overlay.hud", format!("visible={visible}"));
                    self.request_full_redraw();
                }
                UiEvent::RelicScene {
                    context,
                    mut scene,
                    deadline,
                } => {
                    if !context.is_current()
                        || self
                            .relic_context
                            .as_ref()
                            .is_none_or(|current| current.generation != context.generation)
                    {
                        continue;
                    }
                    if self.suggestion_dismissed
                        && matches!(scene, crate::relic::Scene::Suggestions(_))
                    {
                        incident::info("overlay.scene_late", "kind=suggestions dismissed=true");
                        continue;
                    }
                    if !matches!(scene, crate::relic::Scene::Suggestions(_)) {
                        self.suggestion_dismissed = false;
                    }
                    let now = Instant::now();
                    if !scene_deadline_is_current(deadline, now) {
                        incident::info(
                            "overlay.scene_late",
                            format!("kind={}", scene_name(&scene)),
                        );
                        continue;
                    }
                    let started = self
                        .relic_scene
                        .as_ref()
                        .filter(|(_, current_deadline, _)| *current_deadline == deadline)
                        .map_or(now, |(_, _, started)| *started);
                    let updates_current_suggestions =
                        matches!(scene, crate::relic::Scene::Suggestions(_))
                            && self.relic_scene.as_ref().is_some_and(
                                |(current, current_deadline, _)| {
                                    *current_deadline == deadline
                                        && matches!(current, crate::relic::Scene::Suggestions(_))
                                },
                            );
                    incident::info(
                        "overlay.scene",
                        deadline.map_or_else(
                            || format!("kind={} persistent=true", scene_name(&scene)),
                            |expires| {
                                format!(
                                    "kind={} remaining_ms={}",
                                    scene_name(&scene),
                                    expires.saturating_duration_since(now).as_millis()
                                )
                            },
                        ),
                    );
                    self.pending_asset_refreshes.apply(&mut scene);
                    self.asset_loader.request(scene.clone());
                    if matches!(scene, crate::relic::Scene::Suggestions(_))
                        && !updates_current_suggestions
                    {
                        self.presentation.reset_suggestions();
                    }
                    self.relic_scene = Some((scene, deadline, started));
                    self.request_full_redraw();
                }
                UiEvent::RelicStart(context) => {
                    if !context.is_current() {
                        continue;
                    }
                    self.relic_context = Some(context);
                    self.relic_scene = None;
                    self.asset_loader.request(crate::relic::Scene::Reading);
                    self.suggestion_dismissed = false;
                    self.request_full_redraw();
                }
                UiEvent::RelicDismiss => {
                    if self
                        .relic_context
                        .as_ref()
                        .is_none_or(|context| !context.is_current())
                    {
                        incident::info("overlay.scene", "kind=dismissed");
                        self.relic_scene = None;
                        self.relic_context = None;
                        self.asset_loader.request(crate::relic::Scene::Reading);
                        self.request_full_redraw();
                    }
                }
                UiEvent::InteractionToggle => {
                    self.set_interaction(!self.interaction_active);
                }
            }
        }
    }

    fn expire_scene(&mut self) {
        if self
            .relic_scene
            .as_ref()
            .is_some_and(|(_, expires, _)| expires.is_some_and(|expires| Instant::now() >= expires))
            || (self.relic_scene.is_some()
                && self
                    .relic_context
                    .as_ref()
                    .is_some_and(|context| !context.is_current()))
        {
            incident::info("overlay.scene_expired", "relic");
            self.relic_scene = None;
            self.asset_loader.request(crate::relic::Scene::Reading);
            self.request_full_redraw();
        }
    }

    fn animate_loading(&mut self) {
        if !self.loading_active() {
            self.frame_pending = false;
            return;
        }
        if !self.frame_pending && self.last_loading_frame.elapsed() >= LOADING_FRAME_INTERVAL {
            self.last_loading_frame = Instant::now();
            self.request_loading_redraw();
        }
    }

    fn update_focus(&mut self) {
        if self.last_focus_check.elapsed() < FOCUS_INTERVAL {
            return;
        }
        self.last_focus_check = Instant::now();
        let active = self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_active(self.warframe_pid));
        if active != self.warframe_active {
            self.warframe_active = active;
            incident::info("overlay.focus", format!("warframe_active={active}"));
            self.request_full_redraw();
        }
    }

    fn interaction_allowed(&self) -> bool {
        self.overlay_enabled
            && self.warframe_active
            && self.relic_scene.as_ref().is_some_and(|(scene, _, _)| {
                matches!(
                    scene,
                    crate::relic::Scene::Rewards(_) | crate::relic::Scene::Suggestions(_)
                )
            })
    }

    fn sync_shortcut_scope(&mut self) {
        let enabled = self.interaction_allowed();
        if enabled != self.shortcut_scope {
            self.shortcut_scope = enabled;
            self.shortcut.set_enabled(enabled);
        }
        if !enabled {
            self.set_interaction(false);
        }
    }

    fn set_interaction(&mut self, active: bool) {
        let active = active && self.interaction_allowed();
        if active == self.interaction_active {
            return;
        }
        self.interaction_active = active;
        self.reload.interaction(active);
        self.presentation.interaction_changed();
        self.layer.wl_surface().set_input_region(Some(if active {
            self.interactive_input_region.wl_region()
        } else {
            self.empty_input_region.wl_region()
        }));
        self.layer.commit();
        incident::info("overlay.interaction", format!("active={active}"));
        self.request_full_redraw();
    }

    fn update_pointer(&mut self, connection: &Connection, position: (f64, f64)) {
        let hovered = self.hit_target(HitTarget::Close, position);
        if self.presentation.set_close_hovered(hovered) {
            self.request_full_redraw();
        }
        if let Some(pointer) = self.themed_pointer.as_ref() {
            let icon = if hovered {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            };
            let _ = pointer.set_cursor(connection, icon);
        }
    }

    fn hit_target(&self, target: HitTarget, position: (f64, f64)) -> bool {
        self.renderer
            .frame()
            .is_some_and(|frame| frame.contains_surface_point(target, position))
    }

    fn scroll_suggestions(&mut self, delta: f64) {
        let Some(item_count) = self.relic_scene.as_ref().and_then(|(scene, _, _)| {
            let crate::relic::Scene::Suggestions(suggestions) = scene else {
                return None;
            };
            Some(suggestions.items.len())
        }) else {
            return;
        };
        if self.presentation.scroll_suggestions(item_count, delta) {
            self.request_full_redraw();
        }
    }

    fn dismiss_suggestions(&mut self) {
        if self
            .relic_scene
            .as_ref()
            .is_some_and(|(scene, _, _)| matches!(scene, crate::relic::Scene::Suggestions(_)))
        {
            incident::info("overlay.scene", "kind=closed");
            self.suggestion_dismissed = true;
            if let Some(context) = &self.relic_context {
                context.cancel();
                let _ = self.relic.send(crate::relic::Trigger::DismissSuggestions {
                    generation: context.generation,
                });
            }
            self.relic_scene = None;
            self.set_interaction(false);
            self.request_full_redraw();
        }
    }

    fn sync_surface(&mut self, queue_handle: &QueueHandle<Self>) {
        let scene_visible = self.overlay_enabled && self.relic_scene.is_some();
        let status_visible = self.overlay_enabled && self.hud.visible();
        let contextual = contextual_surface_required(
            self.overlay_enabled,
            scene_visible,
            status_visible,
            self.contextual_surface,
        );
        if self.sync_geometry(contextual) {
            return;
        }
        let should_draw = ((status_visible || scene_visible) && self.warframe_active) || contextual;
        if !self.configured {
            if needs_remap_configure(should_draw, self.configure_pending) {
                apply_layer_state(&self.layer, self.contextual_surface);
                self.layer.commit();
                self.configure_pending = true;
                incident::info("overlay.remap_requested", "waiting for configure");
            }
            return;
        }
        if should_draw {
            if self.redraw != Redraw::None || !self.mapped {
                self.draw(queue_handle);
            }
        } else if self.mapped {
            self.unmap();
        }
    }

    fn sync_geometry(&mut self, contextual: bool) -> bool {
        if self.contextual_surface == contextual {
            return false;
        }
        self.contextual_surface = contextual;
        apply_layer_state(&self.layer, contextual);
        self.layer.commit();
        self.configured = false;
        self.configure_pending = true;
        self.buffers.clear();
        true
    }

    fn unmap(&mut self) {
        if self.mapped {
            self.layer
                .wl_surface()
                .set_input_region(Some(self.empty_input_region.wl_region()));
            self.layer.wl_surface().attach(None, 0, 0);
            self.layer.commit();
            self.buffers.clear();
            self.configured = false;
            self.configure_pending = false;
            self.mapped = false;
            self.frame_pending = false;
            self.request_full_redraw();
            incident::info("overlay.unmapped", "surface detached");
        }
    }

    fn acquire_surface_buffer(&mut self, width: u32, height: u32, stride: i32) -> Option<usize> {
        for index in 0..self.buffers.len() {
            if self.buffers[index].buffer.canvas(&mut self.pool).is_some() {
                return Some(index);
            }
        }
        if self.buffers.len() >= MAX_SURFACE_BUFFERS {
            return None;
        }

        let (buffer, _) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("create overlay buffer");
        self.buffers.push(SurfaceBuffer {
            buffer,
            frame_key: None,
        });
        Some(self.buffers.len() - 1)
    }

    fn draw(&mut self, queue_handle: &QueueHandle<Self>) {
        let was_mapped = self.mapped;
        let requested_redraw = if was_mapped {
            self.redraw
        } else {
            Redraw::Full
        };
        let scale = self.scale.max(1) as u32;
        let width = self.width * scale;
        let height = self.height * scale;
        let stride = width as i32 * 4;
        let scene_visible =
            self.overlay_enabled && self.relic_scene.is_some() && self.warframe_active;
        let status_visible = self.overlay_enabled
            && self.hud.visible()
            && self.warframe_active
            && (DEBUG_HUD || !scene_visible);
        let status = status_visible.then(|| {
            let origin = STATUS_INSET;
            let daemon = if DEBUG_HUD {
                format!("DEV | {}", self.daemon_summary())
            } else {
                self.daemon_summary()
            };
            let detail = if DEBUG_HUD {
                self.debug_summary()
            } else {
                "wfcli companion hud hide".to_owned()
            };
            (origin, daemon, self.player_summary(), detail)
        });
        let loading_elapsed = self
            .relic_scene
            .as_ref()
            .filter(|_| scene_visible)
            .map(|(_, _, started)| started.elapsed());
        if let Some((scene, _, _)) = self.relic_scene.as_ref().filter(|_| scene_visible) {
            let key = FrameKey {
                width,
                height,
                scale,
                scene: self
                    .presentation
                    .scene(scene.clone(), self.interaction_active),
            };
            if let Err(error) = self.renderer.prepare_frame(key) {
                incident::error("overlay.render_failed", error);
                return;
            }
        }
        let Some(buffer_index) = self.acquire_surface_buffer(width, height, stride) else {
            return;
        };
        let buffer = &mut self.buffers[buffer_index];
        let canvas = buffer
            .buffer
            .canvas(&mut self.pool)
            .expect("selected overlay buffer is writable");

        let animation_bounds = if scene_visible {
            self.renderer
                .frame()
                .as_ref()
                .and_then(|cache| cache.output.animation_bounds)
        } else {
            None
        };
        let partial_animation = scene_visible
            && requested_redraw == Redraw::Loading
            && animation_bounds.is_some()
            && buffer.frame_key.as_ref() == self.renderer.frame().map(|cache| &cache.key);
        let damage = if scene_visible {
            let cache = self
                .renderer
                .frame()
                .expect("visible relic scene has cached frame");
            if partial_animation {
                let bounds = animation_bounds.expect("loading scene has animation bounds");
                copy_frame_rect(&cache.pixels, canvas, width, bounds);
                bounds
            } else {
                canvas.copy_from_slice(&cache.pixels);
                buffer.frame_key = Some(cache.key.clone());
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }
            }
        } else {
            buffer.frame_key = None;
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            }
        };

        let mut painter = match Painter::new(canvas, width, height) {
            Ok(painter) => painter,
            Err(error) => {
                incident::error("overlay.render_failed", error.to_string());
                return;
            }
        };
        if !scene_visible {
            painter.clear();
        }
        if animation_bounds.is_some()
            && let Some(elapsed) = loading_elapsed
        {
            self.renderer.draw_animation(&mut painter, elapsed);
        }
        if !partial_animation && let Some((origin, daemon, player, detail)) = status.as_ref() {
            self.renderer.draw_status(
                &mut painter,
                StatusView {
                    scale,
                    origin: (*origin, *origin),
                    daemon,
                    player,
                    detail,
                },
            );
        }
        if let Err(error) = painter.finish() {
            incident::error("overlay.render_failed", error.to_string());
            return;
        }

        self.layer.wl_surface().damage_buffer(
            damage.x as i32,
            damage.y as i32,
            damage.width as i32,
            damage.height as i32,
        );
        if animation_bounds.is_some() && !self.frame_pending {
            self.layer
                .wl_surface()
                .frame(queue_handle, self.layer.wl_surface().clone());
            self.frame_pending = true;
        }
        self.layer
            .wl_surface()
            .set_input_region(Some(if self.interaction_active {
                self.interactive_input_region.wl_region()
            } else {
                self.empty_input_region.wl_region()
            }));
        buffer
            .buffer
            .attach_to(self.layer.wl_surface())
            .expect("attach overlay buffer");
        self.layer.commit();
        self.mapped = true;
        self.redraw = Redraw::None;
        if !was_mapped {
            incident::info(
                "overlay.mapped",
                format!(
                    "contextual={} size={}x{} scale={scale}",
                    self.contextual_surface, self.width, self.height
                ),
            );
        }
    }

    fn daemon_summary(&self) -> String {
        let version = &self.daemon_version;
        let state = if self.connected {
            "connected"
        } else {
            "disconnected"
        };
        format!("retinue-daemon {version} - {state}")
    }

    fn player_summary(&self) -> String {
        let phase = self.player.phase.as_deref();
        let pid = self
            .player
            .pid
            .map(|pid| format!(" (pid {pid})"))
            .unwrap_or_default();
        match phase {
            Some("game") => format!("Warframe running{pid}"),
            Some("launcher") => format!("Warframe launcher running{pid}"),
            Some("stopped") => "Warframe stopped".to_owned(),
            _ => "Warframe state pending".to_owned(),
        }
    }

    fn debug_summary(&self) -> String {
        let bridge = self
            .player
            .debug_output_active
            .map_or("?", |active| if active { "on" } else { "off" });
        let debug_lines = self.player.debug_lines;
        let scene = self
            .relic_scene
            .as_ref()
            .map_or("idle", |(scene, _, _)| scene_name(scene));
        format!("DBWIN {bridge} | dbg {debug_lines} | relic {scene}")
    }
}

fn scene_name(scene: &crate::relic::Scene) -> &'static str {
    match scene {
        crate::relic::Scene::Reading => "reading",
        crate::relic::Scene::Rewards(_) => "rewards",
        crate::relic::Scene::Suggestions(_) => "suggestions",
        crate::relic::Scene::Error(_) => "error",
    }
}

fn contextual_surface_required(
    overlay_enabled: bool,
    scene_visible: bool,
    status_visible: bool,
    currently_contextual: bool,
) -> bool {
    overlay_enabled && (scene_visible || (currently_contextual && !status_visible))
}

fn scene_deadline_is_current(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_none_or(|deadline| deadline > now)
}

fn needs_remap_configure(should_draw: bool, configure_pending: bool) -> bool {
    should_draw && !configure_pending
}

fn apply_layer_state(layer: &LayerSurface, contextual: bool) {
    let state = if contextual {
        OverlayLayerState::full_output(KeyboardInteractivity::None)
    } else {
        OverlayLayerState::top_left((STATUS_SURFACE_WIDTH, STATUS_SURFACE_HEIGHT))
    };
    state.apply(layer);
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if self.layer.wl_surface() == surface && self.scale != scale {
            self.scale = scale.max(1);
            self.layer.wl_surface().set_buffer_scale(self.scale);
            self.buffers.clear();
            self.request_full_redraw();
        }
    }

    fn transform_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if self.layer.wl_surface() == surface {
            self.frame_pending = false;
        }
    }

    fn surface_enter(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Overlay {
    fn closed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
    ) {
        incident::warn("overlay.closed", "compositor closed layer surface");
        self.closed = true;
        self.loop_signal.stop();
    }

    fn configure(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = if configure.new_size.0 == 0 {
            if self.contextual_surface {
                self.width
            } else {
                STATUS_SURFACE_WIDTH
            }
        } else {
            configure.new_size.0
        };
        self.height = if configure.new_size.1 == 0 {
            if self.contextual_surface {
                self.height
            } else {
                STATUS_SURFACE_HEIGHT
            }
        } else {
            configure.new_size.1
        };
        self.buffers.clear();
        self.configured = true;
        self.configure_pending = false;
        self.request_full_redraw();
    }
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
    ) {
    }

    fn new_capability(
        &mut self,
        _connection: &Connection,
        queue_handle: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.themed_pointer.is_none() {
            let surface = self.compositor.create_surface(queue_handle);
            match self.seat_state.get_pointer_with_theme(
                queue_handle,
                &seat,
                self.shm.wl_shm(),
                surface,
                ThemeSpec::default(),
            ) {
                Ok(pointer) => self.themed_pointer = Some(pointer),
                Err(error) => incident::warn("overlay.pointer_failed", error.to_string()),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.themed_pointer.take()
        {
            pointer.pointer().release();
        }
    }

    fn remove_seat(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
    ) {
    }
}

impl PointerHandler for Overlay {
    fn pointer_frame(
        &mut self,
        connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() || !self.interaction_active {
                continue;
            }
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.update_pointer(connection, event.position);
                }
                PointerEventKind::Leave { .. } => {
                    if self.presentation.close_hovered()
                        && self.presentation.set_close_hovered(false)
                    {
                        self.request_full_redraw();
                    }
                    incident::info("overlay.pointer", "left_surface");
                    self.set_interaction(false);
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    self.update_pointer(connection, event.position);
                    if self.hit_target(HitTarget::Close, event.position) {
                        self.dismiss_suggestions();
                    } else if !self.hit_target(HitTarget::Content, event.position) {
                        self.set_interaction(false);
                    }
                }
                PointerEventKind::Axis { vertical, .. } => {
                    self.update_pointer(connection, event.position);
                    if self.hit_target(HitTarget::Scroll, event.position) {
                        let delta = if vertical.value120 != 0 {
                            f64::from(vertical.value120) / 120.0
                        } else if vertical.discrete != 0 {
                            f64::from(vertical.discrete)
                        } else {
                            vertical.absolute / 15.0
                        };
                        self.scroll_suggestions(delta);
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Overlay);
delegate_output!(Overlay);
delegate_pointer!(Overlay);
delegate_seat!(Overlay);
delegate_shm!(Overlay);
delegate_layer!(Overlay);
delegate_registry!(Overlay);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hud_hides_once_when_game_starts() {
        let mut hud = HudVisibility::for_build(true);
        for phase in [None, Some("stopped"), Some("launcher")] {
            assert!(!hud.observe_phase(phase));
            assert!(hud.visible());
        }
        assert!(hud.observe_phase(Some("game")));
        assert!(!hud.visible());
        for phase in [Some("game"), None, Some("stopped"), Some("launcher")] {
            assert!(!hud.observe_phase(phase));
            assert!(!hud.visible());
        }
    }

    #[test]
    fn explicit_hud_controls_override_startup_policy() {
        for debug in [false, true] {
            let mut hud = HudVisibility::for_build(debug);
            assert_eq!(hud.visible(), debug);
            hud.set_visible(true);
            assert!(!hud.observe_phase(Some("game")));
            assert!(hud.visible());
            hud.set_visible(false);
            assert!(!hud.observe_phase(Some("game")));
            assert!(!hud.visible());
        }
    }

    #[test]
    fn production_hud_stays_hidden_by_default() {
        let mut hud = HudVisibility::for_build(false);
        for phase in [None, Some("launcher"), Some("game")] {
            assert!(!hud.observe_phase(phase));
            assert!(!hud.visible());
        }
    }

    #[test]
    fn frame_rect_copy_leaves_other_pixels_untouched() {
        let source = vec![7; 8 * 6 * 4];
        let mut target = vec![3; source.len()];
        copy_frame_rect(
            &source,
            &mut target,
            8,
            Rect {
                x: 2,
                y: 1,
                width: 3,
                height: 2,
            },
        );

        assert_eq!(&target[(8 + 2) * 4..(8 + 5) * 4], &[7; 12]);
        assert_eq!(&target[(2 * 8 + 2) * 4..(2 * 8 + 5) * 4], &[7; 12]);
        assert_eq!(target[0], 3);
        assert_eq!(target[(3 * 8 + 2) * 4], 3);
    }

    #[test]
    fn persistent_and_current_scene_updates_are_accepted() {
        let now = Instant::now();
        assert!(scene_deadline_is_current(None, now));
        assert!(scene_deadline_is_current(
            Some(now + Duration::from_secs(1)),
            now
        ));
        assert!(!scene_deadline_is_current(Some(now), now));
    }

    #[test]
    fn overlay_visibility_controls_contextual_surface() {
        assert!(contextual_surface_required(true, true, false, false));
        assert!(contextual_surface_required(true, false, false, true));
        assert!(!contextual_surface_required(true, false, true, true));
        assert!(!contextual_surface_required(true, false, false, false));
        assert!(!contextual_surface_required(false, true, false, true));
    }

    #[test]
    fn unmapped_surface_requests_one_configure_before_drawing() {
        assert!(needs_remap_configure(true, false));
        assert!(!needs_remap_configure(true, true));
        assert!(!needs_remap_configure(false, false));
    }
}
