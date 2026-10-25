use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{self, ConfigureWindowAux, ConnectionExt as _, InputFocus},
    protocol::xtest::ConnectionExt as _,
    rust_connection::RustConnection,
};

struct Process(Child);

#[test]
fn foundry_search_uses_system_clipboard_without_blocking_or_losing_selection() {
    let Some((_server, display)) = xvfb() else {
        return;
    };
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("search.json");
    let mut gui = Process(
        Command::new(env!("CARGO_BIN_EXE_wfgui"))
            .args([
                "--fixture",
                "--search",
                "Ash Prime",
                "--size",
                "1200x900",
                "--ui-scale",
                "125",
                "--report",
            ])
            .arg(&report)
            .env("DISPLAY", &display)
            .env_remove("WAYLAND_DISPLAY")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let window = gui_window(&conn, root);
    conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let first = conn.setup().min_keycode;
    let mapping = conn
        .get_keyboard_mapping(first, conn.setup().max_keycode - first + 1)
        .unwrap()
        .reply()
        .unwrap();
    let code = |symbol| {
        first
            + mapping
                .keysyms
                .chunks(mapping.keysyms_per_keycode as usize)
                .position(|keys| keys.contains(&symbol))
                .unwrap() as u8
    };
    let send = |symbol, down| {
        conn.xtest_fake_input(
            if down {
                xproto::KEY_PRESS_EVENT
            } else {
                xproto::KEY_RELEASE_EVENT
            },
            code(symbol),
            x11rb::CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
    };
    let key = |symbol, ctrl| {
        if ctrl {
            send(0xffe3, true);
        }
        send(symbol, true);
        send(symbol, false);
        if ctrl {
            send(0xffe3, false);
        }
        conn.flush().unwrap();
    };
    let query = || -> Option<String> {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report).ok()?).ok()?;
        assert!(value["input_error"].is_null(), "{value}");
        value["search"].as_str().map(str::to_owned)
    };
    wait("initial query", || query().filter(|q| q == "Ash Prime"));
    key(b'f' as u32, true);
    key(b'c' as u32, true);
    let clipboard = conn
        .intern_atom(false, b"CLIPBOARD")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    wait("clipboard ownership", || {
        conn.get_selection_owner(clipboard)
            .ok()?
            .reply()
            .ok()
            .filter(|r| r.owner != x11rb::NONE)
    });
    key(0xff08, false);
    wait("selection deleted", || query().filter(String::is_empty));
    key(b'v' as u32, true);
    wait("clipboard pasted", || query().filter(|q| q == "Ash Prime"));
    key(b'f' as u32, true);
    key(b'x' as u32, true);
    wait("selection cut after copy succeeds", || {
        query().filter(String::is_empty)
    });
    key(b'v' as u32, true);
    wait("cut text retained", || query().filter(|q| q == "Ash Prime"));
    let size = conn.get_geometry(window).unwrap().reply().unwrap();
    conn.xtest_fake_input(
        xproto::MOTION_NOTIFY_EVENT,
        0,
        x11rb::CURRENT_TIME,
        root,
        size.width as i16 - 18,
        24,
        0,
    )
    .unwrap()
    .check()
    .unwrap();
    for event in [xproto::BUTTON_PRESS_EVENT, xproto::BUTTON_RELEASE_EVENT] {
        conn.xtest_fake_input(
            event,
            1,
            x11rb::CURRENT_TIME,
            root,
            size.width as i16 - 18,
            24,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
    }
    conn.flush().unwrap();
    wait("window closes after clipboard use", || {
        gui.0.try_wait().ok().flatten()
    });
}

#[test]
fn foundry_filter_popup_handles_real_keyboard_and_consumes_window_close_click() {
    let Some((_server, display)) = xvfb() else {
        return;
    };
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let first = conn.setup().min_keycode;
    let mapping = conn
        .get_keyboard_mapping(first, conn.setup().max_keycode - first + 1)
        .unwrap()
        .reply()
        .unwrap();
    let key = |symbol| {
        let index = mapping
            .keysyms
            .chunks(mapping.keysyms_per_keycode as usize)
            .position(|keys| keys.contains(&symbol))
            .unwrap();
        for event in [xproto::KEY_PRESS_EVENT, xproto::KEY_RELEASE_EVENT] {
            conn.xtest_fake_input(
                event,
                first + index as u8,
                x11rb::CURRENT_TIME,
                root,
                0,
                0,
                0,
            )
            .unwrap()
            .check()
            .unwrap();
        }
        conn.flush().unwrap();
    };
    for scale in ["100", "125"] {
        let mut gui = Process(
            Command::new(env!("CARGO_BIN_EXE_wfgui"))
                .args([
                    "--fixture",
                    "--show-filters",
                    "--size",
                    "1200x900",
                    "--ui-scale",
                    scale,
                ])
                .env("DISPLAY", &display)
                .env_remove("WAYLAND_DISPLAY")
                .env("WINIT_UNIX_BACKEND", "x11")
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let window = gui_window(&conn, root);
        conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        let size = conn.get_geometry(window).unwrap().reply().unwrap();
        let colors = || -> Option<(usize, usize)> {
            let image = conn
                .get_image(
                    xproto::ImageFormat::Z_PIXMAP,
                    window,
                    0,
                    0,
                    size.width,
                    size.height,
                    u32::MAX,
                )
                .ok()?
                .reply()
                .ok()?;
            let border = image
                .data
                .chunks_exact(4)
                .filter(|p| p[..3] == [0xa8, 0x97, 0x90])
                .count();
            let selected = image
                .data
                .chunks_exact(4)
                .filter(|p| p[..3] == [0xcb, 0x23, 0x59])
                .count();
            Some((border, selected))
        };
        let initial = wait("Foundry filter popup", || {
            colors().filter(|(border, _)| *border > 50)
        });
        key(0x20);
        wait("Prime choice selected", || {
            colors().filter(|(_, selected)| *selected > initial.1 + 1000)
        });
        key(0xff57);
        key(0xff09);
        key(0xff1b);
        wait("Escape closes popup", || {
            colors().filter(|(border, _)| *border == 0)
        });
        key(0xff0d);
        wait("Enter reopens from trigger", || {
            colors().filter(|(border, _)| *border > 50)
        });
        conn.set_input_focus(InputFocus::PARENT, root, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        wait("focus loss closes popup", || {
            colors().filter(|(border, _)| *border == 0)
        });
        conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        key(0xff0d);
        wait("popup reopens after focus returns", || {
            colors().filter(|(border, _)| *border > 50)
        });
        let x = size.width as i16 - 15;
        conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            x11rb::CURRENT_TIME,
            root,
            x,
            20,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
        for event in [xproto::BUTTON_PRESS_EVENT, xproto::BUTTON_RELEASE_EVENT] {
            conn.xtest_fake_input(event, 1, x11rb::CURRENT_TIME, root, x, 20, 0)
                .unwrap()
                .check()
                .unwrap();
        }
        conn.flush().unwrap();
        wait("outside click dismisses", || {
            colors().filter(|(border, _)| *border == 0)
        });
        assert!(
            gui.0.try_wait().unwrap().is_none(),
            "dismissal activated window Close"
        );
        for event in [xproto::BUTTON_PRESS_EVENT, xproto::BUTTON_RELEASE_EVENT] {
            conn.xtest_fake_input(event, 1, x11rb::CURRENT_TIME, root, x, 20, 0)
                .unwrap()
                .check()
                .unwrap();
        }
        conn.flush().unwrap();
        assert!(wait("window shutdown", || gui.0.try_wait().unwrap()).success());
    }
}

#[test]
fn paths_report_is_headless_and_uses_native_cache() {
    let result = Command::new(env!("CARGO_BIN_EXE_wfgui"))
        .arg("--paths-json")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_CACHE_HOME", "/home/player/cache")
        .env("XDG_CONFIG_HOME", "/home/player/config")
        .env(
            "RETINUE_DAEMON_SOCKET",
            "/run/user/1234/retinue/daemon.sock",
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["app"], "wfgui");
    let paths = report["paths"].as_array().unwrap();
    assert!(paths.iter().any(|p| p["kind"] == "derivatives"
        && p["path"] == "/home/player/cache/retinue/games/warframe/gui/derivatives/native-v1"));
    assert!(paths.iter().any(
        |p| p["kind"] == "config" && p["path"] == "/home/player/config/retinue/games/warframe"
    ));
    assert!(paths.iter().any(|p| p["kind"] == "preferences"
        && p["path"] == "/home/player/config/retinue/games/warframe/gui.json"));
    assert!(
        paths
            .iter()
            .any(|p| p["kind"] == "runtime" && p["path"] == "/run/user/1234/retinue")
    );
}

#[test]
fn filters_cannot_block_a_different_start_page() {
    let result = Command::new(env!("CARGO_BIN_EXE_wfgui"))
        .args(["--fixture", "--page", "settings", "--show-filters"])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("require --page foundry"));
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait<T>(label: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "desktop event did not settle: {label}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn xvfb() -> Option<(Process, String)> {
    let child = match Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1280x1024x24",
            "-nolisten",
            "tcp",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP: Xvfb is needed for the isolated desktop test");
            return None;
        }
        Err(error) => panic!("Xvfb: {error}"),
    };
    let mut server = Process(child);
    let mut number = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut number)
        .unwrap();
    let display = format!(":{}", number.trim());
    Some((server, display))
}

fn gui_window(conn: &RustConnection, root: u32) -> u32 {
    wait("window creation", || {
        conn.query_tree(root)
            .ok()?
            .reply()
            .ok()?
            .children
            .into_iter()
            .find(|window| {
                if !conn
                    .get_window_attributes(*window)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .is_some_and(|attributes| attributes.map_state == xproto::MapState::VIEWABLE)
                {
                    return false;
                }
                conn.get_property(
                    false,
                    *window,
                    xproto::AtomEnum::WM_CLASS,
                    xproto::AtomEnum::STRING,
                    0,
                    64,
                )
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .is_some_and(|reply| reply.value.starts_with(b"wfgui\0"))
            })
    })
}

#[test]
fn native_window_presents_resizes_handles_keyboard_modal_and_closes() {
    let Some((_server, display)) = xvfb() else {
        return;
    };
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let mut gui = Process(
        Command::new(env!("CARGO_BIN_EXE_wfgui"))
            .args(["--fixture", "--page", "settings", "--size", "1000x760"])
            .env("DISPLAY", &display)
            .env_remove("WAYLAND_DISPLAY")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let window = gui_window(&conn, root);
    let pixel = |x, y| -> Option<Vec<u8>> {
        Some(
            conn.get_image(xproto::ImageFormat::Z_PIXMAP, window, x, y, 1, 1, u32::MAX)
                .ok()?
                .reply()
                .ok()?
                .data[..3]
                .to_vec(),
        )
    };
    wait("initial frame", || {
        (pixel(160, 140)? == [47, 30, 24]).then_some(())
    });
    conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let first = conn.setup().min_keycode;
    let mapping = conn
        .get_keyboard_mapping(first, conn.setup().max_keycode - first + 1)
        .unwrap()
        .reply()
        .unwrap();
    let key = |symbol| {
        let index = mapping
            .keysyms
            .chunks(mapping.keysyms_per_keycode as usize)
            .position(|keys| keys.contains(&symbol))
            .unwrap();
        let code = first + index as u8;
        for event in [xproto::KEY_PRESS_EVENT, xproto::KEY_RELEASE_EVENT] {
            conn.xtest_fake_input(event, code, x11rb::CURRENT_TIME, root, 0, 0, 0)
                .unwrap()
                .check()
                .unwrap();
        }
        conn.flush().unwrap();
    };
    key(0xff09);
    key(0xffc3); // F6 enters content without depending on navigation entries.
    for _ in 0..3 {
        key(0xff09);
    }
    key(0xff0d);
    wait("confirmation opened", || {
        (pixel(160, 140)? != [47, 30, 24]).then_some(())
    });
    key(0xff1b);
    wait("confirmation closed", || {
        (pixel(160, 140)? == [47, 30, 24]).then_some(())
    });
    let click = |x, y| {
        conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            x11rb::CURRENT_TIME,
            root,
            x,
            y,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
        for event in [xproto::BUTTON_PRESS_EVENT, xproto::BUTTON_RELEASE_EVENT] {
            conn.xtest_fake_input(event, 1, x11rb::CURRENT_TIME, root, x, y, 0)
                .unwrap()
                .check()
                .unwrap();
        }
        conn.flush().unwrap();
    };
    click(50, 20);
    wait("navigation collapsed", || {
        (pixel(80, 140)? == [47, 30, 24]).then_some(())
    });
    click(50, 20);
    wait("navigation expanded", || {
        (pixel(80, 140)? == [35, 22, 16]).then_some(())
    });
    let focus_color = [249, 154, 170];
    assert_ne!(pixel(37, 20).unwrap(), focus_color);
    key(0x20);
    wait("keyboard focus shows a complete ring", || {
        (pixel(37, 20)? == focus_color && pixel(51, 6)? == focus_color).then_some(())
    });
    click(50, 20);
    wait("mouse activation hides the ring", || {
        (pixel(37, 20)? != focus_color && pixel(80, 140)? == [35, 22, 16]).then_some(())
    });
    conn.configure_window(window, &ConfigureWindowAux::new().width(960).height(600))
        .unwrap()
        .check()
        .unwrap();
    wait("resize", || {
        let size = conn.get_geometry(window).ok()?.reply().ok()?;
        (size.width == 960 && size.height == 600).then_some(())
    });
    key(0xff56);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(pixel(160, 140).unwrap(), [47, 30, 24]);
    click(935, 20);
    assert!(wait("shutdown", || gui.0.try_wait().unwrap()).success());

    let captures = tempfile::tempdir().unwrap();
    let screenshot = captures.path().join("foundry.png");
    let report = captures.path().join("foundry.json");
    let mut capture = Process(
        Command::new(env!("CARGO_BIN_EXE_wfgui"))
            .args([
                "--fixture",
                "--page",
                "foundry",
                "--size",
                "960x600",
                "--window-screenshot",
            ])
            .arg(&screenshot)
            .arg("--report")
            .arg(&report)
            .env("DISPLAY", &display)
            .env_remove("WAYLAND_DISPLAY")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(
        wait("asynchronous image capture", || capture
            .0
            .try_wait()
            .unwrap())
        .success()
    );
    assert!(screenshot.exists());
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert!(data["prepared_images"].as_u64().unwrap() > 0);
    assert!(data["image_errors"].as_array().unwrap().is_empty());
}

#[test]
fn interactive_scale_is_saved_restored_and_fixture_changes_are_isolated() {
    let Some((_server, display)) = xvfb() else {
        return;
    };
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let storage = tempfile::tempdir().unwrap();
    let file = storage.path().join("retinue/games/warframe/gui.json");
    let launch = |options: &[&str]| {
        Process(
            Command::new(env!("CARGO_BIN_EXE_wfgui"))
                .args(["--page", "settings", "--size", "1000x760"])
                .args(options)
                .env("DISPLAY", &display)
                .env_remove("WAYLAND_DISPLAY")
                .env("WINIT_UNIX_BACKEND", "x11")
                .env("XDG_CONFIG_HOME", storage.path())
                .env("XDG_CACHE_HOME", storage.path())
                .env("RETINUE_DAEMON_SOCKET", storage.path().join("missing.sock"))
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let click = |x, y| {
        conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            x11rb::CURRENT_TIME,
            root,
            x,
            y,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
        for event in [xproto::BUTTON_PRESS_EVENT, xproto::BUTTON_RELEASE_EVENT] {
            conn.xtest_fake_input(event, 1, x11rb::CURRENT_TIME, root, x, y, 0)
                .unwrap()
                .check()
                .unwrap();
        }
        conn.flush().unwrap();
    };
    let pixel = |window, x, y| -> Option<Vec<u8>> {
        Some(
            conn.get_image(xproto::ImageFormat::Z_PIXMAP, window, x, y, 1, 1, u32::MAX)
                .ok()?
                .reply()
                .ok()?
                .data[..3]
                .to_vec(),
        )
    };
    let title_click = |window, from_right: f32, scale: f32| {
        let geometry = conn.get_geometry(window).unwrap().reply().unwrap();
        click(
            geometry.width as i16 - (from_right * scale).round() as i16,
            (20.0 * scale).round() as i16,
        );
    };
    let mut first = launch(&[]);
    let window = gui_window(&conn, root);
    wait("default scale", || {
        (pixel(window, 300, 47)? == [47, 30, 24]).then_some(())
    });
    title_click(window, 164.0, 1.0);
    wait("scale edit saved", || {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).ok()?).ok()?;
        (value["ui_scale"] == 105).then_some(())
    });
    wait("scaled window minimum", || {
        assert!(
            first.0.try_wait().unwrap().is_none(),
            "GUI exited during resize"
        );
        (conn.get_geometry(window).ok()?.reply().ok()?.width >= 1076).then_some(())
    });
    title_click(window, 25.0, 1.05);
    assert!(wait("first close", || first.0.try_wait().unwrap()).success());
    let saved = std::fs::read(&file).unwrap();
    let mut second = launch(&[]);
    let window = gui_window(&conn, root);
    wait("restored scaled frame", || {
        (pixel(window, 300, 60)? == [47, 30, 24] && pixel(window, 300, 47)? == [35, 22, 16])
            .then_some(())
    });
    title_click(window, 25.0, 1.05);
    assert!(wait("second close", || second.0.try_wait().unwrap()).success());
    assert_eq!(std::fs::read(&file).unwrap(), saved);
    let mut fixture = launch(&["--fixture"]);
    let window = gui_window(&conn, root);
    wait("fixture defaults", || {
        (pixel(window, 300, 47)? == [47, 30, 24]).then_some(())
    });
    title_click(window, 164.0, 1.0);
    wait("fixture changes locally", || {
        (pixel(window, 300, 47)? == [35, 22, 16]).then_some(())
    });
    title_click(window, 25.0, 1.05);
    assert!(wait("fixture close", || fixture.0.try_wait().unwrap()).success());
    assert_eq!(std::fs::read(&file).unwrap(), saved);
}

#[test]
fn rapid_resizes_keep_paint_dimensions_tied_to_the_backing_buffer() {
    let Some((_server, display)) = xvfb() else {
        return;
    };
    let (conn, screen) = RustConnection::connect(Some(&display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let mut gui = Process(
        Command::new(env!("CARGO_BIN_EXE_wfgui"))
            .args(["--fixture", "--page", "settings", "--size", "1024x760"])
            .env("DISPLAY", &display)
            .env_remove("WAYLAND_DISPLAY")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let window = gui_window(&conn, root);
    for index in 0..200 {
        assert!(
            gui.0.try_wait().unwrap().is_none(),
            "GUI exited during resize {index}"
        );
        conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .width(1024 + (index * 19) % 240)
                .height(600 + (index * 23) % 300),
        )
        .unwrap()
        .check()
        .unwrap();
        conn.flush().unwrap();
        thread::sleep(Duration::from_millis(3));
    }
    conn.configure_window(window, &ConfigureWindowAux::new().width(1200).height(900))
        .unwrap()
        .check()
        .unwrap();
    wait("final resized frame", || {
        assert!(
            gui.0.try_wait().unwrap().is_none(),
            "GUI exited after resize"
        );
        let image = conn
            .get_image(
                xproto::ImageFormat::Z_PIXMAP,
                window,
                1198,
                880,
                1,
                1,
                u32::MAX,
            )
            .ok()?
            .reply()
            .ok()?;
        (image.data[..3] == [35, 22, 16]).then_some(())
    });
}
