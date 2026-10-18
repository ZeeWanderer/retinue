use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{ConnectionExt as _, GrabMode, Keycode, Window},
    },
    rust_connection::RustConnection,
};

pub use x11rb::protocol::xproto::ModMask;

pub struct Binding {
    pub keysym: u32,
    pub modifiers: ModMask,
}

pub struct WindowInput {
    connection: RustConnection,
    keys: Vec<(Keycode, ModMask)>,
    window: Option<Window>,
    enabled: Vec<usize>,
}

impl WindowInput {
    pub fn connect(bindings: &[Binding]) -> Result<Self, String> {
        Self::connect_display(None, bindings)
    }

    pub fn connect_display(display: Option<&str>, bindings: &[Binding]) -> Result<Self, String> {
        let (connection, _) = RustConnection::connect(display).map_err(|e| e.to_string())?;
        let first = connection.setup().min_keycode;
        let mapping = connection
            .get_keyboard_mapping(first, connection.setup().max_keycode - first + 1)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let keys = bindings
            .iter()
            .map(|binding| {
                mapping
                    .keysyms
                    .chunks(mapping.keysyms_per_keycode as usize)
                    .position(|symbols| symbols.contains(&binding.keysym))
                    .map(|index| (first + index as u8, binding.modifiers))
                    .ok_or_else(|| format!("keysym {:#x} is unavailable", binding.keysym))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            connection,
            keys,
            window: None,
            enabled: Vec::new(),
        })
    }

    pub fn bind(&mut self, window: Option<Window>, enabled: &[usize]) -> Result<(), String> {
        let enabled = if window.is_some() { enabled } else { &[] };
        if self.window == window && self.enabled == enabled {
            return Ok(());
        }
        if enabled.iter().any(|index| *index >= self.keys.len()) {
            return Err("invalid window input binding".into());
        }
        self.unbind();
        while self
            .connection
            .poll_for_event()
            .map_err(|e| e.to_string())?
            .is_some()
        {}
        if let Some(window) = window {
            if self
                .connection
                .setup()
                .roots
                .iter()
                .any(|screen| screen.root == window)
            {
                return Err("window input cannot bind desktop-wide shortcuts".into());
            }
            self.window = Some(window);
            for &index in enabled {
                let (key, modifiers) = self.keys[index];
                self.enabled.push(index);
                for locks in lock_masks() {
                    let result = self
                        .connection
                        .grab_key(
                            false,
                            window,
                            modifiers | locks,
                            key,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                        )
                        .map_err(|e| e.to_string())
                        .and_then(|cookie| cookie.check().map_err(|e| e.to_string()));
                    if let Err(error) = result {
                        self.unbind();
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Vec<usize>, String> {
        let mut pressed = Vec::new();
        while let Some(event) = self
            .connection
            .poll_for_event()
            .map_err(|e| e.to_string())?
        {
            if let Event::KeyPress(event) = event {
                if Some(event.event) != self.window {
                    continue;
                }
                let modifiers = u16::from(event.state) & !(u16::from(ModMask::LOCK | ModMask::M2));
                if let Some(&index) = self
                    .enabled
                    .iter()
                    .find(|&&index| self.keys[index] == (event.detail, ModMask::from(modifiers)))
                {
                    pressed.push(index);
                }
                // A passive grab becomes active until release; don't hold other game input.
                self.connection
                    .ungrab_keyboard(event.time)
                    .map_err(|e| e.to_string())?;
                self.connection.flush().map_err(|e| e.to_string())?;
            }
        }
        Ok(pressed)
    }

    fn unbind(&mut self) {
        if let Some(window) = self.window.take() {
            for index in self.enabled.drain(..) {
                let (key, modifiers) = self.keys[index];
                for locks in lock_masks() {
                    if let Ok(cookie) = self.connection.ungrab_key(key, window, modifiers | locks) {
                        let _ = cookie.check();
                    }
                }
            }
        }
    }
}

impl Drop for WindowInput {
    fn drop(&mut self) {
        self.unbind();
    }
}

fn lock_masks() -> [ModMask; 4] {
    [
        ModMask::default(),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ]
}
