use crate::Rect;
mod clipboard;
use crate::layer::OverlayLayerState;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{EventLoop, LoopHandle},
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
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
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
    /// Interactive rectangles in buffer pixels, after painting.
    fn pointer_regions(&self) -> Vec<Rect>;
    fn tick(&mut self) -> bool;
    fn input(&mut self, event: Input) -> bool;
    fn paint(&mut self, pixels: &mut [u8], size: (u32, u32), scale: u32) -> Result<(), String>;
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

pub fn run(name: &str, app: Box<dyn Application>, stopping: Arc<AtomicBool>) -> Result<(), String> {
    run_inner(name, app, stopping).map_err(|e| e.to_string())
}

fn run_inner(
    name: &str,
    app: Box<dyn Application>,
    stopping: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&connection)?;
    let qh = queue.handle();
    let mut events: EventLoop<Surface> = EventLoop::try_new()?;
    WaylandSource::new(connection.clone(), queue).insert(events.handle())?;
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
    let pool = SlotPool::new(4, &shm)?;
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
        pool,
        buffers: Vec::new(),
        size: (0, 0),
        scale: 1,
        configured: false,
        mapped: false,
        keyboard_interactive: false,
        pointer_regions: Vec::new(),
        frame_pending: false,
        dirty: true,
        closed: false,
        app,
        clipboard: clipboard::Clipboard::new(&globals, &qh),
        loop_handle: events.handle(),
    };
    while !stopping.load(Ordering::Acquire) && !state.closed {
        events.dispatch(
            Some(Duration::from_millis(if state.mapped { 16 } else { 100 })),
            &mut state,
        )?;
        state.dirty |= state.app.tick();
        if !state.app.keyboard_interactive() || state.clipboard.expired() {
            state.cancel_paste();
        }
        if state.app.visible() != state.mapped {
            state.mapped = state.app.visible();
            if cfg!(debug_assertions) {
                eprintln!("retinue-overlay: mapped={}", state.mapped);
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
                state.pool = SlotPool::new(4, &state.shm)?;
            }
            state.dirty = true;
        }
        if state.mapped && state.keyboard_interactive != state.app.keyboard_interactive() {
            state.keyboard_interactive = state.app.keyboard_interactive();
            state
                .layer
                .set_keyboard_interactivity(state.keyboard_mode());
            state.layer.commit();
        }
        if state.mapped && state.configured && state.dirty && !state.frame_pending {
            state.draw(&qh)?;
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
    pool: SlotPool,
    buffers: Vec<Buffer>,
    size: (u32, u32),
    scale: u32,
    configured: bool,
    mapped: bool,
    keyboard_interactive: bool,
    pointer_regions: Vec<Rect>,
    frame_pending: bool,
    dirty: bool,
    closed: bool,
    app: Box<dyn Application>,
    clipboard: clipboard::Clipboard,
    loop_handle: LoopHandle<'static, Self>,
}

impl Surface {
    fn keyboard_mode(&self) -> KeyboardInteractivity {
        if self.keyboard_interactive {
            KeyboardInteractivity::Exclusive
        } else {
            KeyboardInteractivity::None
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height) = (self.size.0 * self.scale, self.size.1 * self.scale);
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
            return Err("unsupported overlay output size".into());
        }
        let index = match self
            .buffers
            .iter()
            .position(|b| b.canvas(&mut self.pool).is_some())
        {
            Some(index) => index,
            None if self.buffers.len() < 3 => {
                let (buffer, _) = self.pool.create_buffer(
                    width as i32,
                    height as i32,
                    width as i32 * 4,
                    wl_shm::Format::Argb8888,
                )?;
                self.buffers.push(buffer);
                self.buffers.len() - 1
            }
            None => return Ok(()),
        };
        let buffer = &self.buffers[index];
        let pixels = buffer
            .canvas(&mut self.pool)
            .ok_or("overlay buffer unavailable")?;
        self.app.paint(pixels, (width, height), self.scale)?;
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
        buffer.attach_to(self.layer.wl_surface())?;
        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        self.layer
            .wl_surface()
            .frame(qh, self.layer.wl_surface().clone());
        self.layer.commit();
        self.frame_pending = true;
        self.dirty = false;
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
        self.dirty = true;
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
            eprintln!("retinue-overlay: configured={:?}", configure.new_size);
        }
        if self.size != configure.new_size {
            self.size = configure.new_size;
            self.buffers.clear();
        }
        self.configured = true;
        self.dirty = true;
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
            self.dirty |= self.app.input(input);
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
            self.app.input(Input::FocusLost);
            self.dirty = true;
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
