use retinue_render::surface::Submission;
use smithay_client_toolkit::{
    delegate_shm,
    shm::{Shm, ShmHandler},
};
use std::{num::NonZeroU32, sync::Arc};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{Backend, ObjectId},
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_shm, wl_surface},
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::Window,
};

pub(super) enum Surface {
    Wayland(Wayland),
    Other(softbuffer::Surface<Arc<Window>, Arc<Window>>),
}

impl Surface {
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        if matches!(
            window.display_handle().map_err(|e| e.to_string())?.as_raw(),
            RawDisplayHandle::Wayland(_)
        ) {
            return Wayland::new(window).map(Self::Wayland);
        }
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        softbuffer::Surface::new(&context, window)
            .map(Self::Other)
            .map_err(|e| e.to_string())
    }
    pub fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), String> {
        match self {
            Self::Wayland(surface) => {
                if surface.size != (width.get(), height.get()) {
                    surface.buffers.clear();
                    surface.size = (width.get(), height.get());
                }
                Ok(())
            }
            Self::Other(surface) => surface.resize(width, height).map_err(|e| e.to_string()),
        }
    }
    pub fn draw(
        &mut self,
        preserve_source: bool,
        paint: impl FnOnce(u8, &mut [u8], Option<&[u8]>) -> Result<Option<Submission>, String>,
    ) -> Result<Option<Submission>, String> {
        match self {
            Self::Wayland(surface) => surface.draw(preserve_source, paint),
            Self::Other(surface) => {
                let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
                let age = buffer.age();
                let Some(plan) = paint(age, bytemuck::cast_slice_mut(&mut buffer), None)? else {
                    return Ok(None);
                };
                let damage = plan
                    .damage
                    .rects()
                    .iter()
                    .filter_map(|r| {
                        Some(softbuffer::Rect {
                            x: r.x,
                            y: r.y,
                            width: NonZeroU32::new(r.width)?,
                            height: NonZeroU32::new(r.height)?,
                        })
                    })
                    .collect::<Vec<_>>();
                buffer
                    .present_with_damage(&damage)
                    .map_err(|e| e.to_string())?;
                Ok(Some(plan))
            }
        }
    }
}

struct State {
    shm: Shm,
}
impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
delegate_shm!(State);
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

pub(super) struct Wayland {
    buffers: crate::presentation_buffers::Buffers,
    surface: wl_surface::WlSurface,
    events: EventQueue<State>,
    state: State,
    connection: Connection,
    size: (u32, u32),
    _window: Arc<Window>,
}

impl Wayland {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        let RawDisplayHandle::Wayland(display) =
            window.display_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("not a Wayland display".into());
        };
        let RawWindowHandle::Wayland(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("not a Wayland window".into());
        };
        // Keep the owning window alive until all foreign proxies and mappings are dropped.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, events) =
            registry_queue_init::<State>(&connection).map_err(|e| e.to_string())?;
        let shm = Shm::bind(&globals, &events.handle()).map_err(|e| e.to_string())?;
        let id = unsafe {
            ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                handle.surface.as_ptr().cast(),
            )
        }
        .map_err(|e| e.to_string())?;
        let surface = wl_surface::WlSurface::from_id(&connection, id).map_err(|e| e.to_string())?;
        Ok(Self {
            buffers: Default::default(),
            surface,
            events,
            state: State { shm },
            connection,
            size: (0, 0),
            _window: window,
        })
    }
    fn draw(
        &mut self,
        preserve_source: bool,
        paint: impl FnOnce(u8, &mut [u8], Option<&[u8]>) -> Result<Option<Submission>, String>,
    ) -> Result<Option<Submission>, String> {
        self.events
            .dispatch_pending(&mut self.state)
            .map_err(|e| e.to_string())?;
        let index = loop {
            if let Some(index) = self.buffers.acquire(
                &self.state.shm,
                self.size,
                wl_shm::Format::Xrgb8888,
                preserve_source,
            )? {
                break index;
            }
            self.events
                .blocking_dispatch(&mut self.state)
                .map_err(|e| e.to_string())?;
        };
        let pixels = self
            .buffers
            .pixels(index)
            .ok_or("presentation target busy")?;
        let Some(plan) = paint(
            pixels.age,
            pixels.target,
            pixels.source.map(|(_, source)| source),
        )?
        else {
            return Ok(None);
        };
        self.buffers
            .present(index, &self.surface, &plan.damage, || {})?;
        match self.connection.flush() {
            Ok(()) => {}
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
        Ok(Some(plan))
    }
}
