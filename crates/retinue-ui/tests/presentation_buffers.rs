#![cfg(any(feature = "overlay", feature = "desktop"))]

use retinue_render::{Rect, damage::Region};
use retinue_ui::presentation_buffers::Buffers;
use smithay_client_toolkit::{
    delegate_shm,
    shm::{Shm, ShmHandler},
};
use std::{
    os::unix::net::UnixStream,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol as client,
};
use wayland_server::{
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New, Resource,
    protocol as server,
};

struct ClientState(Shm);
impl ShmHandler for ClientState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.0
    }
}
delegate_shm!(ClientState);
impl wayland_client::Dispatch<client::wl_registry::WlRegistry, GlobalListContents> for ClientState {
    fn event(
        _: &mut Self,
        _: &client::wl_registry::WlRegistry,
        _: client::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
wayland_client::delegate_noop!(ClientState: ignore client::wl_compositor::WlCompositor);
wayland_client::delegate_noop!(ClientState: ignore client::wl_surface::WlSurface);

#[derive(Debug, PartialEq)]
enum SurfaceEvent {
    Attach(bool),
    Damage((i32, i32, i32, i32)),
    DamageBuffer((i32, i32, i32, i32)),
    InputRegion,
    Commit,
}

struct Server {
    buffers: Arc<Mutex<Vec<server::wl_buffer::WlBuffer>>>,
    requests: Arc<Mutex<Vec<SurfaceEvent>>>,
}
impl<I: Resource + 'static> GlobalDispatch<I, ()> for Server
where
    Self: Dispatch<I, ()>,
{
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        new: New<I>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(new, ());
    }
}
impl Dispatch<server::wl_compositor::WlCompositor, ()> for Server {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &server::wl_compositor::WlCompositor,
        request: server::wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let server::wl_compositor::Request::CreateSurface { id } = request {
            init.init(id, ());
        }
    }
}
impl Dispatch<server::wl_surface::WlSurface, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &server::wl_surface::WlSurface,
        request: server::wl_surface::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        use server::wl_surface::Request;
        let event = match request {
            Request::Attach { buffer, .. } => SurfaceEvent::Attach(buffer.is_some()),
            Request::Damage {
                x,
                y,
                width,
                height,
            } => SurfaceEvent::Damage((x, y, width, height)),
            Request::DamageBuffer {
                x,
                y,
                width,
                height,
            } => SurfaceEvent::DamageBuffer((x, y, width, height)),
            Request::SetInputRegion { .. } => SurfaceEvent::InputRegion,
            Request::Commit => SurfaceEvent::Commit,
            _ => return,
        };
        state.requests.lock().unwrap().push(event);
    }
}
impl Dispatch<server::wl_shm::WlShm, ()> for Server {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &server::wl_shm::WlShm,
        request: server::wl_shm::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let server::wl_shm::Request::CreatePool { id, .. } = request {
            init.init(id, ());
        }
    }
}
impl Dispatch<server::wl_shm_pool::WlShmPool, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &server::wl_shm_pool::WlShmPool,
        request: server::wl_shm_pool::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let server::wl_shm_pool::Request::CreateBuffer { id, .. } = request {
            state.buffers.lock().unwrap().push(init.init(id, ()));
        }
    }
}
impl Dispatch<server::wl_buffer::WlBuffer, ()> for Server {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &server::wl_buffer::WlBuffer,
        _: server::wl_buffer::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

struct ServerThread(mpsc::Sender<()>, Option<thread::JoinHandle<()>>);
impl Drop for ServerThread {
    fn drop(&mut self) {
        let _ = self.0.send(());
        self.1.take().unwrap().join().unwrap();
    }
}

#[test]
fn released_targets_borrow_busy_sources_without_copying_or_overwriting_them() {
    exercise_buffers(4, false);
}

#[test]
fn legacy_surfaces_use_surface_damage_with_the_same_submission_lifecycle() {
    exercise_buffers(3, false);
}

#[cfg(feature = "desktop")]
#[test]
fn release_wakes_a_blocked_host_without_event_thread_dispatch() {
    exercise_buffers(4, true);
}

fn exercise_buffers(version: u32, _async_release: bool) {
    let mut display = Display::<Server>::new().unwrap();
    let mut handle = display.handle();
    handle.create_global::<Server, server::wl_compositor::WlCompositor, _>(version, ());
    handle.create_global::<Server, server::wl_shm::WlShm, _>(1, ());
    let (client, server) = UnixStream::pair().unwrap();
    handle.insert_client(server, Arc::new(())).unwrap();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut state = Server {
        buffers: recorded.clone(),
        requests: requests.clone(),
    };
    let (stop, stopping) = mpsc::channel();
    let _server = ServerThread(
        stop,
        Some(thread::spawn(move || {
            while matches!(stopping.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                display.dispatch_clients(&mut state).unwrap();
                display.flush_clients().unwrap();
                thread::sleep(Duration::from_millis(1));
            }
        })),
    );
    let display_owner = Connection::from_socket(client).unwrap();
    #[cfg(feature = "desktop")]
    let connection = if _async_release {
        // Match Winit interop: a separate native queue on an externally owned display.
        Connection::from_backend(unsafe {
            wayland_client::backend::Backend::from_foreign_display(
                display_owner.backend().display_ptr(),
            )
        })
    } else {
        display_owner.clone()
    };
    #[cfg(not(feature = "desktop"))]
    let connection = display_owner.clone();
    let (globals, mut queue) = registry_queue_init::<ClientState>(&connection).unwrap();
    let qh = queue.handle();
    let compositor: client::wl_compositor::WlCompositor =
        globals.bind(&qh, version..=version, ()).unwrap();
    let surface = compositor.create_surface(&qh, ());
    let mut state = ClientState(Shm::bind(&globals, &qh).unwrap());
    #[cfg(feature = "desktop")]
    let wake = _async_release.then(|| {
        let (tx, rx) = mpsc::channel();
        let wake = retinue_ui::presentation_buffers::readiness::QueueWake::new(
            connection.backend(),
            move || {
                tx.send(()).unwrap();
            },
        )
        .unwrap();
        (wake, rx)
    });
    let mut buffers = Buffers::default();
    let size = (83, 59);
    let format = client::wl_shm::Format::Argb8888;
    let damage = Region::new([
        Rect {
            x: 1,
            y: 2,
            width: 10,
            height: 5,
        },
        Rect {
            x: 60,
            y: 42,
            width: 8,
            height: 6,
        },
    ]);
    let mut previous_pointer = None;
    for index in 0..3 {
        assert_eq!(
            buffers.acquire(&state.0, size, format, true).unwrap(),
            Some(index)
        );
        assert_eq!(
            buffers.allocated_bytes(),
            (index + 1) * (size.0 as usize * size.1 as usize * 4).next_multiple_of(64)
        );
        let pixels = buffers.pixels(index).unwrap();
        assert_eq!(pixels.age, 0);
        assert_eq!(pixels.target.len(), size.0 as usize * size.1 as usize * 4);
        if let Some((from, source)) = pixels.source {
            assert_eq!(from, index - 1);
            assert_eq!(Some(source.as_ptr()), previous_pointer);
            assert!(source.iter().all(|v| *v == index as u8));
        } else {
            assert_eq!(index, 0);
        }
        pixels.target.fill(index as u8 + 1);
        previous_pointer = Some(pixels.target.as_ptr());
        buffers
            .present(index, &surface, &damage, || surface.set_input_region(None))
            .unwrap();
        queue.roundtrip(&mut state).unwrap();
        let mut expected = vec![SurfaceEvent::Attach(true)];
        if version >= 4 {
            expected.extend(damage.rects().iter().map(|r| {
                SurfaceEvent::DamageBuffer((
                    r.x as i32,
                    r.y as i32,
                    r.width as i32,
                    r.height as i32,
                ))
            }));
        } else {
            expected.push(SurfaceEvent::Damage((0, 0, i32::MAX, i32::MAX)));
        }
        expected.extend([SurfaceEvent::InputRegion, SurfaceEvent::Commit]);
        assert_eq!(*requests.lock().unwrap(), expected);
        requests.lock().unwrap().clear();
        assert!(
            buffers.pixels(index).is_none(),
            "compositor-owned target must not be writable"
        );
        let mut called = false;
        assert!(
            buffers
                .present(index, &surface, &damage, || called = true)
                .is_err()
        );
        assert!(!called);
        queue.roundtrip(&mut state).unwrap();
        assert!(requests.lock().unwrap().is_empty());
    }
    assert!(!buffers.drawable());
    assert!(
        buffers
            .acquire(&state.0, size, format, true)
            .unwrap()
            .is_none()
    );
    recorded.lock().unwrap()[0].release();
    #[cfg(feature = "desktop")]
    if let Some((wake, rx)) = &wake {
        while !buffers.drawable() {
            wake.arm().unwrap();
            rx.recv_timeout(Duration::from_secs(3))
                .expect("release must wake the host");
        }
        assert!(!wake.waiting());
        assert!(
            rx.recv_timeout(Duration::from_millis(20)).is_err(),
            "idle waiter must not poll"
        );
        wake.arm().unwrap();
        wake.cancel();
    } else {
        queue.roundtrip(&mut state).unwrap();
    }
    #[cfg(not(feature = "desktop"))]
    queue.roundtrip(&mut state).unwrap();
    assert!(buffers.drawable());
    assert_eq!(
        buffers.acquire(&state.0, size, format, true).unwrap(),
        Some(0)
    );
    let pixels = buffers.pixels(0).unwrap();
    assert_eq!(pixels.age, 3);
    let (from, source) = pixels.source.unwrap();
    assert_eq!(from, 2);
    assert_eq!(Some(source.as_ptr()), previous_pointer);
    let target_address = pixels.target.as_ptr() as usize;
    let source_address = source.as_ptr() as usize;
    let mut lease = buffers.detach(0).unwrap();
    assert!(buffers.pixels(0).is_none());
    assert!(
        buffers
            .acquire(&state.0, size, format, true)
            .unwrap()
            .is_none()
    );
    lease = thread::spawn(move || {
        let pixels = lease.pixels().unwrap();
        assert_eq!(pixels.target.as_ptr() as usize, target_address);
        assert_eq!(pixels.source.unwrap().1.as_ptr() as usize, source_address);
        pixels.target.fill(99);
        lease
    })
    .join()
    .unwrap();
    assert_eq!(buffers.restore(lease), Some(0));
    let pixels = buffers.pixels(0).unwrap();
    let (_, source) = pixels.source.unwrap();
    pixels.target.fill(99);
    assert!(source.iter().all(|v| *v == 3));
    // An abandoned target is not promoted to source.
    assert_eq!(buffers.pixels(0).unwrap().source.unwrap().0, 2);
    assert!(buffers.acquire(&state.0, (84, 59), format, true).is_err());
    drop(buffers.detach(0).unwrap());
    assert_eq!(
        buffers.acquire(&state.0, size, format, true).unwrap(),
        Some(0)
    );
    buffers.buffer(0).attach_to(&surface).unwrap();
    surface.commit();
    queue.roundtrip(&mut state).unwrap();
    recorded.lock().unwrap()[2].release();
    queue.roundtrip(&mut state).unwrap();
    assert_eq!(
        buffers.acquire(&state.0, size, format, true).unwrap(),
        Some(2)
    );
    let pixels = buffers.pixels(2).unwrap();
    assert_eq!(pixels.age, 1);
    assert!(
        pixels.source.is_none(),
        "same-target fallback must not alias"
    );

    let stale = buffers.detach(2).unwrap();
    buffers.clear();
    assert_eq!(buffers.restore(stale), None);
    assert_eq!(
        buffers.acquire(&state.0, (40, 30), format, true).unwrap(),
        Some(0)
    );
    assert_eq!(buffers.pixels(0).unwrap().age, 0);
    assert!(buffers.pixels(0).unwrap().source.is_none());
    buffers.submitted(0);
    assert_eq!(
        buffers.acquire(&state.0, (40, 30), format, false).unwrap(),
        Some(0)
    );
    assert_eq!(
        buffers.len(),
        1,
        "disabled relocation must not allocate a source slot"
    );
    exercise_render_job(&state.0);
    #[cfg(feature = "desktop")]
    if let Some((wake, _rx)) = wake {
        // Shutdown must interrupt a waiter even if the compositor sends no more events.
        wake.arm().unwrap();
        drop(wake);
    }
}

fn exercise_render_job(shm: &Shm) {
    use retinue_render::{
        Color, DrawPacket, HostPolicy, PacketJob, RenderHost, Wake,
        damage::Damage,
        painter::RasterImage,
        surface::Buffered,
        work::{Graph, Priority},
    };
    use retinue_ui::presentation_buffers::render::RenderJob;
    use std::num::{NonZeroU32, NonZeroUsize};
    let host = RenderHost::new(HostPolicy {
        threads: 2,
        ..HostPolicy::overlay()
    });
    let threads = host.task_threads(NonZeroU32::new(2).unwrap());
    for (reset, raster) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut buffers = Buffers::default();
        let index = buffers
            .acquire(shm, (40, 30), client::wl_shm::Format::Argb8888, true)
            .unwrap()
            .unwrap();
        let mut repair = Buffered::default();
        repair.add_buffer();
        repair.invalidate(Damage::Full);
        let pixels = buffers.pixels(index).unwrap();
        let address = pixels.target.as_ptr() as usize;
        let work = repair
            .plan_queued(index, (40, 30), None, pixels.target, pixels.source, threads)
            .unwrap()
            .unwrap()
            .into_work();
        let rect = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 30,
        };
        let image = Arc::new(
            RasterImage::render(40, 30, |p| {
                p.fill_rounded_rect(0, 0, 40, 30, 0, Color::rgb(0x123456).bgra())
            })
            .unwrap(),
        );
        let packet = DrawPacket::image(image, (0, 0), rect);
        let (release, gate) = mpsc::channel();
        let (entered, running) = mpsc::channel();
        let mut graph = Graph::default();
        graph.add(
            &[],
            NonZeroUsize::new(threads.get() as usize).unwrap(),
            move |_| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5))
                    .expect("test did not release the rendering participant");
                Ok(())
            },
        );
        let hold = host
            .executor()
            .submit(graph, Priority::Foreground, || {})
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let (wake, woke) = mpsc::channel();
        let wake: Wake = Arc::new(move || {
            let _ = wake.send(());
        });
        let mut packet = PacketJob::ready(packet);
        if raster {
            packet = packet.rasterize((40, 30), Arc::default(), rect, &host, threads, wake.clone());
        }
        let mut job = RenderJob::submit(
            &host,
            threads,
            buffers.detach(index).unwrap(),
            work,
            packet,
            [0; 4],
            wake,
        )
        .unwrap();
        assert_eq!(host.executor().stats().queued, 1);
        assert_eq!(woke.try_recv(), Err(mpsc::TryRecvError::Empty));
        let late = Rect {
            x: 3,
            y: 5,
            width: 9,
            height: 7,
        };
        repair.invalidate(Damage::rect(late));
        if reset {
            buffers.clear();
            repair.reset();
        }
        release.send(()).unwrap();
        hold.wait().unwrap();
        woke.recv_timeout(Duration::from_secs(5))
            .expect("worker completion must wake an idle host");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let rendered = loop {
            if let Some(result) = job.poll() {
                break result.unwrap().unwrap();
            }
            woke.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
        };
        assert_eq!(repair.can_submit(&rendered.submission), !reset);
        assert_eq!(buffers.restore(rendered.lease), (!reset).then_some(index));
        if !reset {
            let pixels = buffers.pixels(index).unwrap();
            assert_eq!(pixels.target.as_ptr() as usize, address);
            assert!(
                pixels
                    .target
                    .chunks_exact(4)
                    .all(|pixel| pixel == Color::rgb(0x123456).bgra())
            );
            assert!(repair.submitted(rendered.submission));
            assert!(repair.is_dirty(), "late damage must survive publication");
            let next = repair
                .plan_queued(index, (40, 30), None, pixels.target, pixels.source, threads)
                .unwrap()
                .unwrap();
            assert!(next.repair().intersects(late));
        }
    }
}
