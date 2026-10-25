#![cfg(feature = "overlay")]

use retinue_ui::{
    focus::FocusDetector,
    window_input::{Binding, ModMask, WindowInput},
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{self, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, WindowClass},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn shortcuts_are_scoped_to_one_window_and_release_cleanly() {
    let child = match Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "640x480x24",
            "-nolisten",
            "tcp",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP: install Xvfb for isolated window-input tests");
            return;
        }
        Err(error) => panic!("Xvfb: {error}"),
    };
    let mut server = Server(child);
    let mut number = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut number)
        .unwrap();
    let display = format!(":{}", number.trim());
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let window = || {
        let id = conn.generate_id().unwrap();
        conn.create_window(
            0,
            id,
            root,
            0,
            0,
            100,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
        )
        .unwrap()
        .check()
        .unwrap();
        conn.map_window(id).unwrap().check().unwrap();
        id
    };
    let game = window();
    let other = window();
    conn.configure_window(other, &xproto::ConfigureWindowAux::new().x(200))
        .unwrap()
        .check()
        .unwrap();
    conn.change_property8(
        xproto::PropMode::REPLACE,
        game,
        xproto::AtomEnum::WM_CLASS,
        xproto::AtomEnum::STRING,
        b"test\0retinue-test-game\0",
    )
    .unwrap()
    .check()
    .unwrap();
    conn.change_window_attributes(
        root,
        &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
    )
    .unwrap()
    .check()
    .unwrap();
    let detector = FocusDetector::connect_display("retinue-test-game", Some(&display)).unwrap();
    conn.warp_pointer(x11rb::NONE, game, 0, 0, 0, 0, 10, 10)
        .unwrap()
        .check()
        .unwrap();
    assert!(detector.request_game_focus_at_pointer(None).unwrap());
    conn.sync().unwrap();
    let mut activation = None;
    while let Some(event) = conn.poll_for_event().unwrap() {
        if let Event::ClientMessage(message) = event {
            activation = Some(message);
        }
    }
    let activation = activation.expect("focus handoff requests activation, never injects a click");
    assert_eq!(activation.window, game);
    assert_eq!(
        activation.data.as_data32()[0],
        1,
        "use application focus policy, not force activation"
    );
    conn.warp_pointer(x11rb::NONE, other, 0, 0, 0, 0, 10, 10)
        .unwrap()
        .check()
        .unwrap();
    assert!(!detector.request_game_focus_at_pointer(None).unwrap());
    let bindings = [
        Binding {
            keysym: b'e' as u32,
            modifiers: ModMask::M1,
        },
        Binding {
            keysym: 0xff1b,
            modifiers: ModMask::default(),
        },
    ];
    let mut input = WindowInput::connect_display(Some(&display), &bindings).unwrap();
    assert!(input.bind(Some(root), &[0]).is_err());
    assert!(input.bind(Some(game), &[99]).is_err());
    input.bind(Some(game), &[0, 1]).unwrap();
    let first = conn.setup().min_keycode;
    let map = conn
        .get_keyboard_mapping(first, conn.setup().max_keycode - first + 1)
        .unwrap()
        .reply()
        .unwrap();
    let key = |symbol| {
        first
            + map
                .keysyms
                .chunks(map.keysyms_per_keycode as usize)
                .position(|symbols| symbols.contains(&symbol))
                .unwrap() as u8
    };
    let escape = key(0xff1b);
    let alt = key(0xffe9);
    let e = key(b'e' as u32);
    let focus = |id| {
        conn.set_input_focus(InputFocus::PARENT, id, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
    };
    let inject = |key, pressed| {
        conn.xtest_fake_input(
            if pressed {
                xproto::KEY_PRESS_EVENT
            } else {
                xproto::KEY_RELEASE_EVENT
            },
            key,
            0,
            root,
            0,
            0,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
    };
    let drain = || {
        let mut keys = Vec::new();
        while let Some(event) = conn.poll_for_event().unwrap() {
            if let Event::KeyPress(event) = event {
                keys.push((event.event, event.detail));
            }
        }
        keys
    };
    let poll = |input: &mut WindowInput| {
        std::thread::sleep(Duration::from_millis(20));
        input.poll().unwrap()
    };

    focus(game);
    let mut events =
        smithay_client_toolkit::reexports::calloop::EventLoop::<bool>::try_new().unwrap();
    use smithay_client_toolkit::reexports::calloop::{
        Interest, Mode, PostAction, generic::Generic,
    };
    let registration = events
        .handle()
        .insert_source(
            Generic::new(input.event_fd().unwrap(), Interest::READ, Mode::Level),
            |_, _, woke| {
                *woke = true;
                Ok(PostAction::Continue)
            },
        )
        .unwrap();
    inject(alt, true);
    inject(e, true);
    inject(e, false);
    inject(alt, false);
    let mut woke = false;
    events.dispatch(Duration::from_secs(2), &mut woke).unwrap();
    assert!(
        woke,
        "focused shortcut wakes the renderer without a poll timer"
    );
    events.handle().remove(registration);
    assert_eq!(poll(&mut input), [0]);
    assert!(!drain().contains(&(game, e)));
    inject(escape, true);
    inject(escape, false);
    assert_eq!(poll(&mut input), [1]);
    assert!(!drain().contains(&(game, escape)));

    focus(other);
    inject(alt, true);
    inject(e, true);
    inject(e, false);
    inject(alt, false);
    assert!(poll(&mut input).is_empty());
    assert!(drain().contains(&(other, e)));
    inject(escape, true);
    inject(escape, false);
    assert!(poll(&mut input).is_empty());
    assert!(drain().contains(&(other, escape)));

    focus(game);
    input.bind(Some(game), &[0]).unwrap();
    inject(escape, true);
    inject(escape, false);
    assert!(poll(&mut input).is_empty());
    assert!(drain().contains(&(game, escape)));
    input.bind(None, &[]).unwrap();
    input.bind(Some(game), &[0, 1]).unwrap();
    inject(escape, true);
    inject(escape, false);
    assert_eq!(poll(&mut input), [1]);
    drain();
    drop(input);
    inject(escape, true);
    inject(escape, false);
    assert!(drain().contains(&(game, escape)));
}
