use super::{Input, Surface};
use smithay_client_toolkit::{
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{CopyPasteSource, DataSourceHandler},
    },
    delegate_data_device,
    reexports::calloop::{PostAction, RegistrationToken},
    seat::keyboard::KeyEvent,
};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::GlobalList,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_keyboard::WlKeyboard, wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
};

const MIME: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];
const LIMIT: usize = 4096;

pub(super) struct Clipboard {
    manager: Option<DataDeviceManagerState>,
    device: Option<DataDevice>,
    source: Option<(CopyPasteSource, String)>,
    pending: Option<(RegistrationToken, Instant)>,
}

impl Clipboard {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<Surface>) -> Self {
        Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            device: None,
            source: None,
            pending: None,
        }
    }
    pub fn attach(&mut self, qh: &QueueHandle<Surface>, seat: &WlSeat) {
        self.device = self.manager.as_ref().map(|m| m.get_data_device(qh, seat));
    }
    pub fn detach(&mut self) {
        self.device = None;
    }
    pub fn expired(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(2))
    }
    fn copy(&mut self, qh: &QueueHandle<Surface>, serial: u32, text: String) -> bool {
        let (Some(manager), Some(device)) = (&self.manager, &self.device) else {
            return false;
        };
        if text.len() > LIMIT {
            return false;
        }
        let source = manager.create_copy_paste_source(qh, MIME);
        source.set_selection(device, serial);
        self.source = Some((source, text));
        true
    }
}

impl Surface {
    pub(super) fn key(
        &mut self,
        qh: &QueueHandle<Self>,
        _: &WlKeyboard,
        serial: Option<u32>,
        event: KeyEvent,
    ) {
        self.cancel_paste();
        if !self.app.keyboard_interactive() {
            return;
        }
        let symbol = event.keysym.raw();
        if self.modifiers.ctrl && !self.modifiers.alt {
            match symbol {
                0x63 | 0x43 | 0x78 | 0x58 => {
                    if let Some(serial) = serial
                        && let Some(text) = self.app.clipboard_text()
                        && self.clipboard.copy(qh, serial, text)
                        && matches!(symbol, 0x78 | 0x58)
                    {
                        self.dirty |= self.app.input(Input::Cut);
                    }
                    return;
                }
                0x76 | 0x56 => {
                    if serial.is_some() {
                        self.paste();
                    }
                    return;
                }
                _ => {}
            }
        }
        self.dirty |= self.app.input(Input::Key {
            symbol,
            text: event.utf8,
            control: self.modifiers.ctrl,
            alt: self.modifiers.alt,
            shift: self.modifiers.shift,
        });
    }

    pub(super) fn cancel_paste(&mut self) {
        if let Some((token, _)) = self.clipboard.pending.take() {
            self.loop_handle.remove(token);
        }
    }

    fn paste(&mut self) {
        let Some(offer) = self
            .clipboard
            .device
            .as_ref()
            .and_then(|d| d.data().selection_offer())
        else {
            return;
        };
        let mime = offer.with_mime_types(|types| {
            MIME.iter()
                .find(|m| types.iter().any(|t| t == **m))
                .copied()
        });
        let Some(mime) = mime else {
            return;
        };
        let Ok(pipe) = offer.receive(mime.into()) else {
            return;
        };
        let mut bytes = Vec::new();
        let result = self.loop_handle.insert_source(pipe, move |_, file, state| {
            let mut chunk = [0; 1024];
            let mut file: &std::fs::File = file;
            let result = match file.read(&mut chunk) {
                Ok(0) => {
                    if state.app.keyboard_interactive()
                        && let Ok(text) = String::from_utf8(std::mem::take(&mut bytes))
                    {
                        state.dirty |= state.app.input(Input::Paste(text));
                    }
                    PostAction::Remove
                }
                Ok(size) if bytes.len() + size <= LIMIT => {
                    bytes.extend_from_slice(&chunk[..size]);
                    PostAction::Continue
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    PostAction::Continue
                }
                _ => PostAction::Remove,
            };
            if result == PostAction::Remove {
                state.clipboard.pending = None;
            }
            result
        });
        if let Ok(token) = result {
            self.clipboard.pending = Some((token, Instant::now()));
        }
    }
}

impl DataDeviceHandler for Surface {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &WlSurface,
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}
impl DataOfferHandler for Surface {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}
impl DataSourceHandler for Surface {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        mut pipe: WritePipe,
    ) {
        if let Some((owner, text)) = &self.clipboard.source
            && owner.inner() == source
            && MIME.contains(&mime.as_str())
        {
            let _ = pipe.write_all(text.as_bytes());
        }
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self
            .clipboard
            .source
            .as_ref()
            .is_some_and(|(owner, _)| owner.inner() == source)
        {
            self.clipboard.source = None;
        }
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}
delegate_data_device!(Surface);
