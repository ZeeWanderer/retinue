use crate::Rect;
use crate::schedule::{Deadlines, PingSource};
use retinue_render::{
    damage::Damage,
    surface::{Buffered, Snapshot},
};
#[cfg(feature = "benchmark")]
pub mod benchmark;
mod clipboard;
use crate::layer::OverlayLayerState;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic},
        calloop_wayland_source::WaylandSource,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Modifiers, RawModifiers},
        pointer::{
            CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler},
};
use std::{
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

#[derive(Clone, Debug)]
pub enum Input {
    Motion(f64, f64),
    Press {
        x: f64,
        y: f64,
        shift: bool,
    },
    Release,
    Scroll {
        x: f64,
        y: f64,
        amount: f64,
        shift: bool,
        alt: bool,
    },
    Key {
        symbol: u32,
        text: Option<String>,
        control: bool,
        alt: bool,
        shift: bool,
    },
    Paste(String),
    Cut,
    FocusLost,
    PointerLeave,
}

pub trait Application {
    fn visible(&self) -> bool;
    fn keyboard_interactive(&self) -> bool;
    fn text_cursor(&self) -> bool {
        false
    }
    fn clipboard_text(&self) -> Option<String> {
        None
    }
    /// Interactive rectangles in buffer pixels, published by preparation.
    fn pointer_regions(&self) -> Vec<Rect>;
    /// Level-triggered sources; tick must drain them even while hidden.
    fn event_fds(&self) -> std::io::Result<Vec<OwnedFd>> {
        Ok(Vec::new())
    }
    fn deadlines(&self, now: Instant) -> Deadlines;
    fn tick(&mut self, drawable: bool) -> Damage;
    fn input(&mut self, event: Input) -> Damage;
    /// Publish geometry and include damage discovered by layout or scene diffing.
    fn prepare(&mut self, _size: (u32, u32), _scale: u32) -> Result<Damage, String> {
        Ok(Damage::None)
    }
    fn paint_scene(&self) -> Option<Snapshot> {
        None
    }
    fn relocation(&self) -> retinue_render::surface::Relocation {
        retinue_render::surface::Relocation::Auto
    }
    fn render_frame(&self) -> Option<retinue_render::RenderPass> {
        None
    }
    /// Clear and repaint every layer within repair, preserving pixels outside it.
    fn paint(
        &mut self,
        painter: &mut retinue_render::painter::Painter<'_>,
        size: (u32, u32),
        scale: u32,
        repair: &retinue_render::damage::Region,
    ) -> Result<(), String>;
}

fn surface_rect(rect: Rect, scale: u32) -> Rect {
    Rect {
        x: rect.x / scale,
        y: rect.y / scale,
        width: if rect.width == 0 {
            0
        } else {
            rect.x.saturating_add(rect.width).div_ceil(scale) - rect.x / scale
        },
        height: if rect.height == 0 {
            0
        } else {
            rect.y.saturating_add(rect.height).div_ceil(scale) - rect.y / scale
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_region_preserves_physical_edges_at_output_scale() {
        let rect = Rect {
            x: 21,
            y: 43,
            width: 201,
            height: 101,
        };
        assert_eq!(surface_rect(rect, 1), rect);
        assert_eq!(
            surface_rect(rect, 2),
            Rect {
                x: 10,
                y: 21,
                width: 101,
                height: 51
            }
        );
        assert_eq!(surface_rect(Rect { width: 0, ..rect }, 2).width, 0);
    }
}

pub fn run(
    name: &str,
    app: Box<dyn Application>,
    stopping: Arc<AtomicBool>,
    wake: PingSource,
) -> Result<(), String> {
    run_inner(
        name,
        app,
        stopping,
        wake,
        #[cfg(feature = "benchmark")]
        None,
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "benchmark")]
pub fn run_measured(
    name: &str,
    app: Box<dyn Application>,
    stopping: Arc<AtomicBool>,
    wake: PingSource,
    capture: benchmark::Capture,
    policy: benchmark::Policy,
) -> Result<(), String> {
    run_inner(name, app, stopping, wake, Some((capture, policy))).map_err(|e| e.to_string())
}

fn run_inner(
    name: &str,
    app: Box<dyn Application>,
    stopping: Arc<AtomicBool>,
    wake: PingSource,
    #[cfg(feature = "benchmark")] measurement: Option<(benchmark::Capture, benchmark::Policy)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&connection)?;
    let qh = queue.handle();
    let mut events: EventLoop<Surface> = EventLoop::try_new()?;
    WaylandSource::new(connection.clone(), queue).insert(events.handle())?;
    #[cfg(feature = "benchmark")]
    let event_driven = measurement
        .as_ref()
        .is_none_or(|(_, policy)| policy.wakes());
    #[cfg(not(feature = "benchmark"))]
    let event_driven = true;
    if event_driven {
        events.handle().insert_source(wake, |(), _, _| {})?;
        for fd in app.event_fds()? {
            events.handle().insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |ready, _, _| {
                    Ok(if ready.error {
                        PostAction::Remove
                    } else {
                        PostAction::Continue
                    })
                },
            )?;
        }
    }
    let compositor = CompositorState::bind(&globals, &qh)?;
    let shell = LayerShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let layer = shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Overlay,
        Some(name),
        None,
    );
    OverlayLayerState::full_output(KeyboardInteractivity::None).apply(&layer);
    let empty_region = Region::new(&compositor)?;
    layer
        .wl_surface()
        .set_input_region(Some(empty_region.wl_region()));
    let mut state = Surface {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        shm,
        layer,
        pointer: None,
        keyboard: None,
        modifiers: Modifiers::default(),
        buffers: crate::presentation_buffers::Buffers::default(),
        size: (0, 0),
        scale: 1,
        configured: false,
        mapped: false,
        keyboard_interactive: false,
        pointer_regions: Vec::new(),
        frame_pending: false,
        repaint: Buffered::default(),
        closed: false,
        app,
        clipboard: clipboard::Clipboard::new(&globals, &qh),
        loop_handle: events.handle(),
        #[cfg(feature = "benchmark")]
        measurement: measurement
            .map(|(capture, policy)| benchmark::Measurement::bind(&globals, &qh, capture, policy))
            .transpose()?,
    };
    while !stopping.load(Ordering::Acquire) && !state.closed {
        let drawable = !state.mapped || state.drawable();
        state.repaint.invalidate(state.app.tick(drawable));
        if !state.app.keyboard_interactive() || state.clipboard.expired() {
            state.cancel_paste();
        }
        if state.app.visible() != state.mapped {
            state.mapped = state.app.visible();
            if cfg!(debug_assertions) {
                eprintln!("retinue-ui: mapped={}", state.mapped);
            }
            state.configured = false;
            state.frame_pending = false;
            if state.mapped {
                state.keyboard_interactive = state.app.keyboard_interactive();
                OverlayLayerState::full_output(state.keyboard_mode()).apply(&state.layer);
                state
                    .layer
                    .wl_surface()
                    .set_buffer_scale(state.scale as i32);
                state.layer.commit();
            } else {
                state
                    .layer
                    .set_keyboard_interactivity(KeyboardInteractivity::None);
                state.layer.wl_surface().attach(None, 0, 0);
                state.layer.commit();
                state.buffers.clear();
                state.repaint.reset();
            }
            state.repaint.invalidate(Damage::Full);
        }
        if state.mapped && state.keyboard_interactive != state.app.keyboard_interactive() {
            state.keyboard_interactive = state.app.keyboard_interactive();
            state
                .layer
                .set_keyboard_interactivity(state.keyboard_mode());
            state.layer.commit();
        }
        if state.mapped && state.configured && state.repaint.is_dirty() && !state.frame_pending {
            state.draw(&qh)?;
        }
        if stopping.load(Ordering::Acquire) || state.closed {
            break;
        }
        let now = Instant::now();
        let mut deadlines = state.app.deadlines(now);
        deadlines.control(state.clipboard.deadline());
        let timeout = deadlines.timeout(now, state.drawable());
        #[cfg(feature = "benchmark")]
        let timeout = state.measurement.as_ref().map_or(timeout, |measurement| {
            measurement.policy.timeout(state.mapped, timeout)
        });
        events.dispatch(timeout, &mut state)?;
        #[cfg(feature = "benchmark")]
        if let Some(measurement) = &state.measurement {
            measurement.dispatched();
        }
    }
    #[cfg(feature = "benchmark")]
    {
        let until = Instant::now() + std::time::Duration::from_secs(1);
        while state.measurement.as_ref().is_some_and(|m| m.pending()) && Instant::now() < until {
            events.dispatch(
                Some(until.saturating_duration_since(Instant::now())),
                &mut state,
            )?;
        }
    }
    Ok(())
}

struct Surface {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    layer: LayerSurface,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    modifiers: Modifiers,
    buffers: crate::presentation_buffers::Buffers,
    size: (u32, u32),
    scale: u32,
    configured: bool,
    mapped: bool,
    keyboard_interactive: bool,
    pointer_regions: Vec<Rect>,
    frame_pending: bool,
    repaint: Buffered,
    closed: bool,
    app: Box<dyn Application>,
    clipboard: clipboard::Clipboard,
    loop_handle: LoopHandle<'static, Self>,
    #[cfg(feature = "benchmark")]
    measurement: Option<benchmark::Measurement>,
}

impl Surface {
    fn drawable(&mut self) -> bool {
        self.mapped && self.configured && !self.frame_pending && self.buffers.drawable()
    }

    fn keyboard_mode(&self) -> KeyboardInteractivity {
        if self.keyboard_interactive {
            KeyboardInteractivity::Exclusive
        } else {
            KeyboardInteractivity::None
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height) = crate::presentation_buffers::buffer_size(self.size, self.scale)?;
        let count = self.buffers.len();
        let Some(index) = self.buffers.acquire(
            &self.shm,
            (width, height),
            wl_shm::Format::Argb8888,
            self.app.relocation() != retinue_render::surface::Relocation::Disabled,
        )?
        else {
            return Ok(());
        };
        if self.buffers.len() > count {
            self.repaint.add_buffer();
        }
        #[cfg(feature = "benchmark")]
        let paint_start = Instant::now();
        // Keep actionable geometry on the displayed frame while buffers are busy.
        self.repaint
            .invalidate(self.app.prepare((width, height), self.scale)?);
        let scene = self.app.paint_scene();
        self.repaint.set_relocation(self.app.relocation());
        let frame = self.app.render_frame();
        let threads = frame.as_ref().map_or(
            std::num::NonZeroU32::MIN,
            retinue_render::RenderPass::threads,
        );
        let pixels = self
            .buffers
            .pixels(index)
            .ok_or("overlay buffer unavailable")?;
        let Some(plan) = self.repaint.render_queued(
            index,
            (width, height),
            scene,
            pixels.target,
            pixels.source,
            threads,
            |painter, repair| self.app.paint(painter, (width, height), self.scale, repair),
        )?
        else {
            return Ok(());
        };
        #[cfg(feature = "benchmark")]
        let paint_end = Instant::now();
        let regions: Vec<_> = self
            .app
            .pointer_regions()
            .into_iter()
            .map(|rect| surface_rect(rect, self.scale))
            .collect();
        if regions != self.pointer_regions {
            let region = Region::new(&self.compositor)?;
            for rect in &regions {
                region.add(
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                );
            }
            self.layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
            self.pointer_regions = regions;
        }
        self.buffers
            .present(index, self.layer.wl_surface(), &plan.damage, || {
                self.layer
                    .wl_surface()
                    .frame(qh, self.layer.wl_surface().clone());
                #[cfg(feature = "benchmark")]
                if let Some(measurement) = &mut self.measurement {
                    measurement.submit(self.layer.wl_surface(), qh, paint_start, paint_end);
                }
            })?;
        self.frame_pending = true;
        self.repaint.submitted(plan);
        Ok(())
    }
}

impl CompositorHandler for Surface {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        scale: i32,
    ) {
        self.scale = scale.max(1) as u32;
        self.layer.wl_surface().set_buffer_scale(self.scale as i32);
        self.buffers.clear();
        self.repaint.reset();
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_pending = false;
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl LayerShellHandler for Surface {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if cfg!(debug_assertions) {
            eprintln!("retinue-ui: configured={:?}", configure.new_size);
        }
        if self.size != configure.new_size {
            self.size = configure.new_size;
            self.buffers.clear();
            self.repaint.reset();
        }
        self.configured = true;
        self.repaint.invalidate(Damage::Full);
    }
}
impl OutputHandler for Surface {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl SeatHandler for Surface {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let connection = qh.clone();
            self.keyboard = self
                .seats
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(move |state, keyboard, event| {
                        state.key(&connection, keyboard, None, event);
                    }),
                )
                .ok();
            self.clipboard.attach(qh, &seat);
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self
                .seats
                .get_pointer_with_theme(
                    qh,
                    &seat,
                    self.shm.wl_shm(),
                    self.compositor.create_surface(qh),
                    ThemeSpec::default(),
                )
                .ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            self.cancel_paste();
            self.clipboard.detach();
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.pointer().release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl PointerHandler for Surface {
    fn pointer_frame(
        &mut self,
        connection: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            let (x, y) = event.position;
            let input = match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    Input::Motion(x, y)
                }
                PointerEventKind::Press { button: 0x110, .. } => Input::Press {
                    x,
                    y,
                    shift: self.modifiers.shift,
                },
                PointerEventKind::Leave { .. } => Input::PointerLeave,
                PointerEventKind::Release { button: 0x110, .. } => Input::Release,
                PointerEventKind::Axis { vertical, .. } => Input::Scroll {
                    x,
                    y,
                    amount: if vertical.value120 != 0 {
                        vertical.value120 as f64 / 120.0
                    } else {
                        vertical.absolute / 15.0
                    },
                    shift: self.modifiers.shift,
                    alt: self.modifiers.alt,
                },
                _ => continue,
            };
            if matches!(input, Input::Press { .. } | Input::PointerLeave) {
                self.cancel_paste();
            }
            self.repaint.invalidate(self.app.input(input));
            if !matches!(event.kind, PointerEventKind::Leave { .. })
                && let Some(p) = &self.pointer
            {
                let _ = p.set_cursor(
                    connection,
                    if self.app.text_cursor() {
                        CursorIcon::Text
                    } else {
                        CursorIcon::Default
                    },
                );
            }
        }
    }
}
impl KeyboardHandler for Surface {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.mapped {
            self.cancel_paste();
            self.repaint.invalidate(self.app.input(Input::FocusLost));
        }
    }
    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.key(qh, keyboard, Some(serial), event);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(qh, keyboard, None, event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}
impl ShmHandler for Surface {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl ProvidesRegistryState for Surface {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
delegate_compositor!(Surface);
delegate_layer!(Surface);
delegate_output!(Surface);
delegate_seat!(Surface);
delegate_pointer!(Surface);
delegate_keyboard!(Surface);
delegate_shm!(Surface);
delegate_registry!(Surface);
