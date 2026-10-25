use crate::focus::FocusDetector;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xfixes::{ConnectionExt as _, SelectionEventMask},
        xproto::{self, AtomEnum, ConnectionExt as _, CreateWindowAux, WindowClass},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

const MAX_TEXT: u32 = 32 * 1024;

pub fn copy_from_game(class: &str, pid: Option<u32>, stop: &AtomicBool) -> Result<String, String> {
    copy(class, pid, stop).map_err(|e| e.to_string())
}

fn copy(
    class: &str,
    pid: Option<u32>,
    stop: &AtomicBool,
) -> Result<String, Box<dyn std::error::Error>> {
    cancelled(stop)?;
    let focus = FocusDetector::connect(class)?;
    if !focus.is_active(pid) {
        return Err("game must be focused to copy text".into());
    }
    let (conn, screen) = RustConnection::connect(None)?;
    conn.xfixes_query_version(5, 0)?.reply()?;
    let root = conn.setup().roots[screen].root;
    let window = conn.generate_id()?;
    conn.create_window(
        0,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )?
    .check()?;
    let clipboard = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
    let utf8 = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
    let property = conn.intern_atom(false, b"RETINUE_CLIPBOARD")?.reply()?.atom;
    conn.xfixes_select_selection_input(window, clipboard, SelectionEventMask::SET_SELECTION_OWNER)?
        .check()?;
    let first = conn.setup().min_keycode;
    let keys = conn
        .get_keyboard_mapping(first, conn.setup().max_keycode - first + 1)?
        .reply()?;
    let key = |sym| {
        keys.keysyms
            .chunks(keys.keysyms_per_keycode as usize)
            .position(|syms| syms.contains(&sym))
            .map(|i| first + i as u8)
    };
    let control = key(0xffe3).ok_or("Control key unavailable")?;
    let c = key(b'c' as u32).ok_or("C key unavailable")?;
    let modifiers: Vec<u8> = [
        0xffe1, 0xffe2, 0xffe9, 0xffea, 0xffe3, 0xffe4, 0xffeb, 0xffec, 0xffe7, 0xffe8,
    ]
    .into_iter()
    .filter_map(key)
    .collect();
    let started = Instant::now();
    loop {
        cancelled(stop)?;
        let held = conn.query_keymap()?.reply()?.keys;
        if !modifiers
            .iter()
            .any(|k| held[usize::from(*k / 8)] & (1 << (k % 8)) != 0)
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(2) {
            return Err("release the shortcut keys, then try again".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    while conn.poll_for_event()?.is_some() {}
    if !focus.is_active(pid) {
        return Err("game focus changed before item copy".into());
    }
    cancelled(stop)?;
    for (kind, key) in [
        (xproto::KEY_PRESS_EVENT, control),
        (xproto::KEY_PRESS_EVENT, c),
        (xproto::KEY_RELEASE_EVENT, c),
        (xproto::KEY_RELEASE_EVENT, control),
    ] {
        conn.xtest_fake_input(kind, key, 0, root, 0, 0, 0)?
            .check()?;
    }
    conn.flush()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut requested = false;
    while Instant::now() < deadline {
        cancelled(stop)?;
        if !focus.is_active(pid) {
            return Err("game focus changed during item copy".into());
        }
        match conn.poll_for_event()? {
            Some(Event::XfixesSelectionNotify(event))
                if event.selection == clipboard && !requested =>
            {
                conn.convert_selection(window, clipboard, utf8, property, event.timestamp)?
                    .check()?;
                conn.flush()?;
                requested = true;
            }
            Some(Event::SelectionNotify(event)) if event.requestor == window && requested => {
                if event.property == x11rb::NONE {
                    return Err("game did not supply item text".into());
                }
                let data = conn
                    .get_property(true, window, property, AtomEnum::ANY, 0, MAX_TEXT / 4)?
                    .reply()?;
                if data.type_ != utf8 || data.format != 8 || data.bytes_after != 0 {
                    return Err("copied item text exceeds the limit or is not UTF-8".into());
                }
                return Ok(String::from_utf8(data.value)?);
            }
            _ => thread::sleep(Duration::from_millis(8)),
        }
    }
    Err("no fresh item copied; hover an item and try again".into())
}

fn cancelled(stop: &AtomicBool) -> Result<(), std::io::Error> {
    if stop.load(Ordering::Acquire) {
        Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "item copy cancelled",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_precedes_x11_and_input() {
        assert_eq!(
            copy_from_game("unused", None, &AtomicBool::new(true)),
            Err("item copy cancelled".into())
        );
    }
}
