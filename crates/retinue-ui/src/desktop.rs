pub use accesskit;
pub use winit;
pub mod clipboard;

mod frame_clock;
mod presentation;
use frame_clock::FrameClock;

use accesskit::{ActionRequest, TreeUpdate};
use accesskit_winit::{Adapter, WindowEvent as AccessEvent};
use presentation::Surface;
use retinue_render::{
    damage::Damage,
    painter::{Painter, render_thread_budget},
    surface::{Aged, Snapshot},
};
use std::{num::NonZeroU32, path::PathBuf, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::{Window, WindowAttributes, WindowId},
};

pub trait Application {
    type Message: Send + 'static;
    fn attributes(&self) -> WindowAttributes;
    fn resize(&mut self, size: (u32, u32), display_scale: f64);
    fn event(&mut self, event: &WindowEvent, window: &Window) -> Damage;
    fn message(&mut self, message: Self::Message) -> Damage;
    fn prepare(&mut self) -> Result<Option<Snapshot>, String> {
        Ok(None)
    }
    fn paint(&mut self, painter: &mut Painter<'_>) -> Result<(), String>;
    fn background(&self) -> [u8; 4];
    fn accessibility(&self) -> TreeUpdate;
    fn access(&mut self, request: ActionRequest, window: &Window) -> Damage;
    fn tick(&mut self, _now: Instant) -> Damage {
        Damage::None
    }
    fn deadline(&self) -> Option<Instant> {
        None
    }
    /// One-shot nonvisual deadline; `maintain` must advance or clear it.
    fn maintenance_deadline(&self) -> Option<Instant> {
        None
    }
    /// Nonvisual work that must also run while the window is occluded.
    fn maintain(&mut self, _now: Instant) {}
    fn render_host(&self) -> Option<retinue_render::RenderHost> {
        None
    }
    fn relocation(&self) -> retinue_render::surface::Relocation {
        retinue_render::surface::Relocation::Auto
    }
    fn ready(&self) -> bool {
        true
    }
    fn close_requested(&self) -> bool {
        false
    }
}

pub enum Event<M> {
    Message(M),
    Accessibility(accesskit_winit::Event),
}

impl<M> From<accesskit_winit::Event> for Event<M> {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

pub struct Sender<M: Send + 'static>(EventLoopProxy<Event<M>>);

impl<M: Send + 'static> Clone for Sender<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<M: Send + 'static> Sender<M> {
    pub fn send(&self, message: M) -> bool {
        self.0.send_event(Event::Message(message)).is_ok()
    }
}

#[derive(Default)]
pub struct Options {
    pub capture: Option<PathBuf>,
    pub exit_after_capture: bool,
}

pub fn render_threads() -> NonZeroU32 {
    render_thread_budget(6)
}

pub fn run<A: Application>(
    create: impl FnOnce(Sender<A::Message>) -> Result<A, String>,
    options: Options,
) -> Result<(), String> {
    let events = EventLoop::<Event<A::Message>>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = events.create_proxy();
    let app = create(Sender(proxy.clone()))?;
    let mut host = Host {
        app,
        proxy,
        native: None,
        damage: Default::default(),
        options,
        failure: None,
        occluded: false,
        threads: render_threads(),
        frames: FrameClock::default(),
    };
    events.run_app(&mut host).map_err(|e| e.to_string())?;
    host.failure.map_or(Ok(()), Err)
}

struct NativeWindow {
    adapter: Adapter,
    surface: Surface,
    window: Arc<Window>,
    size: winit::dpi::PhysicalSize<u32>,
}

struct Host<A: Application> {
    app: A,
    proxy: EventLoopProxy<Event<A::Message>>,
    native: Option<NativeWindow>,
    damage: Aged,
    options: Options,
    failure: Option<String>,
    occluded: bool,
    threads: NonZeroU32,
    frames: FrameClock,
}

impl<A: Application> Host<A> {
    fn fail(&mut self, events: &ActiveEventLoop, error: impl ToString) {
        self.failure = Some(error.to_string());
        events.exit();
    }

    fn invalidate(&mut self, damage: Damage) {
        if damage != Damage::None {
            if let Some(native) = &mut self.native {
                native.adapter.update_if_active(|| self.app.accessibility());
            }
        }
        self.damage.invalidate(damage);
    }

    fn resize(&mut self) -> Result<(), String> {
        let Some(native) = &mut self.native else {
            return Ok(());
        };
        let size = native.window.inner_size();
        if let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        {
            native
                .surface
                .resize(width, height)
                .map_err(|e| e.to_string())?;
        }
        native.size = size;
        self.app
            .resize((size.width, size.height), native.window.scale_factor());
        native.adapter.update_if_active(|| self.app.accessibility());
        self.damage.reset();
        Ok(())
    }

    fn draw(&mut self, events: &ActiveEventLoop) -> Result<(), String> {
        let Some(native) = &mut self.native else {
            return Ok(());
        };
        // A requested window resize can precede its event and the matching buffer resize.
        let size = native.size;
        if size.width == 0 || size.height == 0 || !self.damage.is_dirty() {
            return Ok(());
        }
        let scene = self.app.prepare()?;
        self.damage.set_relocation(self.app.relocation());
        let frame = self
            .app
            .render_host()
            .map(|host| host.begin_frame(self.threads));
        let threads = frame
            .as_ref()
            .map_or(self.threads, retinue_render::RenderPass::threads);
        let capture = self
            .options
            .capture
            .as_ref()
            .filter(|_| self.app.ready())
            .cloned();
        let Some(plan) = native.surface.draw(
            self.app.relocation() != retinue_render::surface::Relocation::Disabled,
            |age, pixels, source| {
                let Some(plan) = self.damage.render_queued(
                    age,
                    (size.width, size.height),
                    scene,
                    pixels,
                    source,
                    threads,
                    |painter, repair| {
                        let background = self.app.background();
                        if background[3] != 255 {
                            return Err("desktop background must be opaque".into());
                        }
                        let repair = repair.bounds();
                        painter.fill_rounded_rect(
                            repair.x,
                            repair.y,
                            repair.width,
                            repair.height,
                            0,
                            background,
                        );
                        self.app.paint(painter)
                    },
                )?
                else {
                    return Ok(None);
                };
                if let Some(path) = &capture {
                    let image = image::RgbImage::from_fn(size.width, size.height, |x, y| {
                        let n = ((y * size.width + x) * 4) as usize;
                        image::Rgb([pixels[n + 2], pixels[n + 1], pixels[n]])
                    });
                    image.save(path).map_err(|e| e.to_string())?;
                }
                native.window.pre_present_notify();
                Ok(Some(plan))
            },
        )?
        else {
            return Ok(());
        };
        self.damage.submitted(plan);
        if capture.is_some() {
            self.options.capture = None;
            if self.options.exit_after_capture {
                events.exit();
            }
        }
        Ok(())
    }
}

impl<A: Application> ApplicationHandler<Event<A::Message>> for Host<A> {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        let result = (|| {
            let window = Arc::new(
                events
                    .create_window(self.app.attributes().with_visible(false))
                    .map_err(|e| e.to_string())?,
            );
            let adapter = Adapter::with_event_loop_proxy(events, &window, self.proxy.clone());
            match window.display_handle().map_err(|e| e.to_string())?.as_raw() {
                RawDisplayHandle::Wayland(_)
                | RawDisplayHandle::Xlib(_)
                | RawDisplayHandle::Xcb(_) => {}
                _ => return Err("desktop presentation requires Wayland or X11".into()),
            }
            let surface = Surface::new(window.clone())?;
            self.native = Some(NativeWindow {
                adapter,
                surface,
                window: window.clone(),
                size: winit::dpi::PhysicalSize::new(0, 0),
            });
            self.resize()?;
            window.set_visible(true);
            window.request_redraw();
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            self.fail(events, error);
        }
    }

    fn suspended(&mut self, _events: &ActiveEventLoop) {
        self.native = None;
        self.damage.reset();
    }

    fn user_event(&mut self, _events: &ActiveEventLoop, event: Event<A::Message>) {
        let damage = match event {
            Event::Message(message) => self.app.message(message),
            Event::Accessibility(event) => {
                let Some(native) = &mut self.native else {
                    return;
                };
                if event.window_id != native.window.id() {
                    return;
                }
                match event.window_event {
                    AccessEvent::InitialTreeRequested => {
                        native.adapter.update_if_active(|| self.app.accessibility());
                        Damage::None
                    }
                    AccessEvent::ActionRequested(request) => {
                        self.app.access(request, &native.window)
                    }
                    AccessEvent::AccessibilityDeactivated => Damage::None,
                }
            }
        };
        self.invalidate(damage);
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = &mut self.native else {
            return;
        };
        if native.window.id() != id {
            return;
        }
        native.adapter.process_event(&native.window, &event);
        match &event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Err(error) = self.resize() {
                    self.fail(events, error);
                } else if let Some(native) = &self.native {
                    let damage = self.app.event(&event, &native.window);
                    self.invalidate(damage);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frames.frame_started();
                let damage = self.app.tick(Instant::now());
                self.invalidate(damage);
                self.frames.schedule(self.app.deadline());
                if let Err(error) = self.draw(events) {
                    self.fail(events, error);
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = *occluded;
                if !occluded {
                    self.damage.reset();
                    self.frames.frame_started();
                }
            }
            _ => {
                let damage = self.app.event(&event, &native.window);
                self.invalidate(damage);
            }
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        if self.app.close_requested() {
            events.exit();
            return;
        }
        let now = Instant::now();
        if self
            .app
            .maintenance_deadline()
            .is_some_and(|due| due <= now)
        {
            self.app.maintain(now);
        }
        let maintenance = self.app.maintenance_deadline();
        if self.occluded {
            events.set_control_flow(
                self.frames
                    .wake_deadline(maintenance, false)
                    .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
            );
            return;
        }
        if self.options.capture.is_some() && self.app.ready() {
            self.damage.invalidate(Damage::Full);
        }
        if self
            .frames
            .request(self.damage.is_dirty(), self.app.deadline(), now)
        {
            if let Some(native) = &self.native {
                native.window.request_redraw();
            }
        }
        events.set_control_flow(
            self.frames
                .wake_deadline(maintenance, true)
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}
