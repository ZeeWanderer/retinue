use retinue_overlay::layer::OverlayLayerState;
use smithay_client_toolkit::{
    delegate_layer,
    shell::{
        WaylandSurface,
        wlr_layer::{
            KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::{
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol as client,
};
use wayland_protocols_wlr::layer_shell::v1::server::{
    zwlr_layer_shell_v1 as shell, zwlr_layer_surface_v1 as layer,
};
use wayland_server::{
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
    protocol as server,
};

struct ClientState;
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
delegate_layer!(ClientState);
impl LayerShellHandler for ClientState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
    }
}

struct Server(Arc<Mutex<Vec<layer::Request>>>);
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
        if let server::wl_surface::Request::Attach { buffer: None, .. } = request {
            state.0.lock().unwrap().clear();
        }
    }
}
impl Dispatch<shell::ZwlrLayerShellV1, ()> for Server {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &shell::ZwlrLayerShellV1,
        request: shell::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let shell::Request::GetLayerSurface { id, .. } = request {
            init.init(id, ());
        }
    }
}
impl Dispatch<layer::ZwlrLayerSurfaceV1, ()> for Server {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &layer::ZwlrLayerSurfaceV1,
        request: layer::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        state.0.lock().unwrap().push(request);
    }
}

#[test]
fn every_remap_restores_all_layer_role_state() {
    let mut display = Display::<Server>::new().unwrap();
    let mut handle = display.handle();
    handle.create_global::<Server, server::wl_compositor::WlCompositor, _>(4, ());
    handle.create_global::<Server, shell::ZwlrLayerShellV1, _>(4, ());
    let (client, server) = UnixStream::pair().unwrap();
    handle.insert_client(server, Arc::new(())).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let (stop, stopping) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut state = Server(recorded);
        while matches!(
            stopping.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ) {
            display.dispatch_clients(&mut state).unwrap();
            display.flush_clients().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    });
    let connection = Connection::from_socket(client).unwrap();
    let (globals, mut queue) = registry_queue_init::<ClientState>(&connection).unwrap();
    let qh = queue.handle();
    let compositor: client::wl_compositor::WlCompositor = globals.bind(&qh, 4..=4, ()).unwrap();
    let shell = LayerShell::bind(&globals, &qh).unwrap();
    let layer = shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh, ()),
        Layer::Overlay,
        Some("test"),
        None,
    );
    for state in [
        OverlayLayerState::full_output(KeyboardInteractivity::Exclusive),
        OverlayLayerState::full_output(KeyboardInteractivity::None),
        OverlayLayerState::top_left((340, 110)),
        OverlayLayerState::full_output(KeyboardInteractivity::Exclusive),
    ] {
        layer.wl_surface().attach(None, 0, 0);
        layer.commit();
        state.apply(&layer);
        layer.commit();
        queue.roundtrip(&mut ClientState).unwrap();
        let actual = requests.lock().unwrap();
        assert_eq!(
            actual.len(),
            6,
            "remap must restore the complete layer role"
        );
        assert!(matches!(
            actual[0],
            layer::Request::SetLayer {
                layer: WEnum::Value(shell::Layer::Overlay)
            }
        ));
        assert!(matches!(actual[1], layer::Request::SetAnchor { .. }));
        assert!(matches!(actual[2], layer::Request::SetSize { .. }));
        assert!(matches!(actual[3], layer::Request::SetExclusiveZone { .. }));
        assert!(matches!(
            actual[4],
            layer::Request::SetMargin {
                top: 0,
                right: 0,
                bottom: 0,
                left: 0
            }
        ));
        let keyboard = match state.keyboard {
            KeyboardInteractivity::Exclusive => layer::KeyboardInteractivity::Exclusive,
            _ => layer::KeyboardInteractivity::None,
        };
        assert!(
            matches!(actual[5], layer::Request::SetKeyboardInteractivity { keyboard_interactivity: WEnum::Value(value) } if value == keyboard)
        );
    }
    stop.send(()).unwrap();
    server.join().unwrap();
}
