pub use accesskit;
pub use winit;
pub mod clipboard;

mod frame_clock;
use frame_clock::FrameClock;

use crate::{
    damage::{AgeRepaintTracker, Damage},
    painter::{Painter, render_thread_budget},
};
use accesskit::{ActionRequest, TreeUpdate};
use accesskit_winit::{Adapter, WindowEvent as AccessEvent};
use softbuffer::{Context, Surface};
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
    surface: Surface<Arc<Window>, Arc<Window>>,
    window: Arc<Window>,
    size: winit::dpi::PhysicalSize<u32>,
}

struct Host<A: Application> {
    app: A,
    proxy: EventLoopProxy<Event<A::Message>>,
    native: Option<NativeWindow>,
    damage: AgeRepaintTracker,
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
        self.damage.invalidate(damage);
        if damage != Damage::None {
            if let Some(native) = &mut self.native {
                native.adapter.update_if_active(|| self.app.accessibility());
            }
        }
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
        let mut buffer = native.surface.buffer_mut().map_err(|e| e.to_string())?;
        let Some(plan) = self.damage.plan(buffer.age(), (size.width, size.height)) else {
            return Ok(());
        };
        {
            let pixels = bytemuck::cast_slice_mut(&mut buffer);
            let mut painter = Painter::with_threads(pixels, size.width, size.height, self.threads)
                .map_err(|e| e.to_string())?;
            painter.clip_to_rect(plan.repair);
            let background = self.app.background();
            if background[3] != 255 {
                return Err("desktop background must be opaque".into());
            }
            painter.fill_rounded_rect(
                plan.repair.x,
                plan.repair.y,
                plan.repair.width,
                plan.repair.height,
                0,
                background,
            );
            self.app.paint(&mut painter)?;
            painter.finish().map_err(|e| e.to_string())?;
        }
        // Wayland/X11 present XRGB; keeping FF also preserves valid opaque PRGB32 for reuse.
        let capture = self
            .options
            .capture
            .as_ref()
            .filter(|_| self.app.ready())
            .cloned();
        if let Some(path) = &capture {
            let image = image::RgbImage::from_fn(size.width, size.height, |x, y| {
                let pixel = buffer[(y * size.width + x) as usize];
                image::Rgb([(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])
            });
            image.save(path).map_err(|e| e.to_string())?;
        }
        native.window.pre_present_notify();
        buffer
            .present_with_damage(&[softbuffer::Rect {
                x: plan.damage.x,
                y: plan.damage.y,
                width: NonZeroU32::new(plan.damage.width).unwrap(),
                height: NonZeroU32::new(plan.damage.height).unwrap(),
            }])
            .map_err(|e| e.to_string())?;
        self.damage.submitted();
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
            let context = Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface = Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
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
        if self.occluded {
            events.set_control_flow(ControlFlow::Wait);
            return;
        }
        if self.options.capture.is_some() && self.app.ready() {
            self.damage.invalidate(Damage::Full);
        }
        if self
            .frames
            .request(self.damage.is_dirty(), self.app.deadline(), Instant::now())
        {
            if let Some(native) = &self.native {
                native.window.request_redraw();
            }
        }
        events.set_control_flow(
            self.frames
                .deadline()
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}
