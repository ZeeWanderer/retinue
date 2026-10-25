use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

pub struct FocusDetector {
    connection: RustConnection,
    root: Window,
    active_window_atom: Atom,
    pid_atom: Atom,
    class: String,
}

impl FocusDetector {
    pub fn connect(class: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::connect_display(class, None)
    }

    pub fn connect_display(
        class: &str,
        display: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (connection, screen) = RustConnection::connect(display)?;
        let root = connection.setup().roots[screen].root;
        let active_window_atom = intern_atom(&connection, b"_NET_ACTIVE_WINDOW")?;
        let pid_atom = intern_atom(&connection, b"_NET_WM_PID")?;
        Ok(Self {
            connection,
            root,
            active_window_atom,
            pid_atom,
            class: class.to_ascii_lowercase(),
        })
    }

    pub fn is_active(&self, expected_pid: Option<u32>) -> bool {
        self.active_game_window(expected_pid).is_some()
    }

    pub fn active_game_window(&self, expected_pid: Option<u32>) -> Option<Window> {
        self.active_window()
            .filter(|&window| self.is_game_window(window, expected_pid))
    }

    pub fn request_game_focus_at_pointer(
        &self,
        expected_pid: Option<u32>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut parent = self.root;
        for _ in 0..8 {
            let pointer = self.connection.query_pointer(parent)?.reply()?;
            if !pointer.same_screen || pointer.child == x11rb::NONE {
                return Ok(false);
            }
            let window = pointer.child;
            if self.is_game_window(window, expected_pid) {
                let request = ClientMessageEvent::new(
                    32,
                    window,
                    self.active_window_atom,
                    [1, x11rb::CURRENT_TIME, 0, 0, 0],
                );
                self.connection
                    .send_event(
                        false,
                        self.root,
                        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                        request,
                    )?
                    .check()?;
                self.connection.flush()?;
                return Ok(true);
            }
            parent = window;
        }
        Ok(false)
    }

    fn active_window(&self) -> Option<Window> {
        self.connection
            .get_property(
                false,
                self.root,
                self.active_window_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
            .filter(|window| *window != x11rb::NONE)
    }

    fn is_game_window(&self, window: Window, expected_pid: Option<u32>) -> bool {
        let pid_matches =
            expected_pid.is_some_and(|expected| self.window_pid(window) == Some(expected));
        pid_matches
            || self
                .window_classes(window)
                .any(|class| class.eq_ignore_ascii_case(&self.class))
    }

    fn window_pid(&self, window: Window) -> Option<u32> {
        self.connection
            .get_property(false, window, self.pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
    }

    fn window_classes(&self, window: Window) -> impl Iterator<Item = String> {
        let value = self
            .connection
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.value)
            .unwrap_or_default();
        value
            .split(|byte| *byte == 0)
            .filter(|class| !class.is_empty())
            .map(|class| String::from_utf8_lossy(class).to_ascii_lowercase())
            .collect::<Vec<_>>()
            .into_iter()
    }
}

fn intern_atom(
    connection: &RustConnection,
    name: &[u8],
) -> Result<Atom, Box<dyn std::error::Error>> {
    Ok(connection.intern_atom(false, name)?.reply()?.atom)
}
