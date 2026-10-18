use std::fs;
use std::process::Command;

use serde_json::Value;

#[test]
fn cli_exposes_native_observation_and_rejects_obsolete_log_options() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("must-not-create.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .arg("--help")
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .env_remove("POE2COMPANION_RELOAD_FD")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("native observer"));
    assert!(help.contains("--pid"));
    assert!(help.contains("watch"));
    assert!(help.contains("launch"));
    assert!(!help.contains("--reload-check"));
    for args in [
        vec!["inspect"],
        vec!["watch", "--log", "Client.txt"],
        vec!["launch"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
            .args(args)
            .env("RETINUE_DAEMON_SOCKET", &socket)
            .env("RETINUE_COMMAND", "/bin/false")
            .env_remove("POE2COMPANION_RELOAD_FD")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
    assert!(!socket.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn paths_report_only_used_directories_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["paths", "--json"])
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("XDG_RUNTIME_DIR", dir.path().join("runtime"))
        .env_remove("RETINUE_DAEMON_SOCKET")
        .env_remove("POE2COMPANION_RELOAD_FD")
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["app"], "poe2companion");
    let entries = report["paths"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["kind"], "runtime");
    assert_eq!(entries[1]["kind"], "config");
    assert_eq!(
        entries[1]["path"],
        dir.path()
            .join("config/retinue/games/poe2/companion")
            .to_str()
            .unwrap()
    );
    assert_eq!(
        entries[2]["path"],
        dir.path().join("data/applications").to_str().unwrap()
    );
    assert_eq!(
        entries[0]["path"],
        dir.path().join("runtime/retinue").to_str().unwrap()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn paths_report_honors_socket_override() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("custom/daemon.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["paths", "--json"])
        .env("RETINUE_DAEMON_SOCKET", socket)
        .env_remove("POE2COMPANION_RELOAD_FD")
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["paths"][0]["path"],
        dir.path().join("custom").to_str().unwrap()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn preview_renders_without_game_daemon_or_desktop() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("preview.png");
    let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["preview", "--scale", "1.25", "--output"])
        .arg(&image)
        .env("HOME", dir.path())
        .env("RETINUE_DAEMON_SOCKET", dir.path().join("absent.sock"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("POE2COMPANION_RELOAD_FD")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = fs::read(image).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn preview_supports_active_numeric_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture: Value =
        serde_json::from_str(include_str!("fixtures/evaluate-preview.json")).unwrap();
    fixture["profile"] = serde_json::json!("fuzzy");
    fixture["editor"] = serde_json::json!({"modifier": 0, "bound": "minimum", "select_all": true});
    let input = dir.path().join("editor.json");
    fs::write(&input, serde_json::to_vec(&fixture).unwrap()).unwrap();
    for scale in ["1", "1.25"] {
        let path = dir.path().join(format!("editor-{scale}.png"));
        let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
            .args(["preview", "--scale", scale, "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&path)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("POE2COMPANION_RELOAD_FD")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(&fs::read(path).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }
}

#[test]
fn preview_states_keep_requested_full_frame_resolution() {
    let dir = tempfile::tempdir().unwrap();
    for (width, height, scale) in [(1920u32, 1080u32, "1"), (2560, 1440, "1.25")] {
        for state in [
            "ready",
            "searching",
            "results",
            "listing-preview",
            "empty",
            "search-error",
        ] {
            let path = dir.path().join(format!("{state}-{width}.png"));
            let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
                .args([
                    "preview",
                    "--full-frame",
                    "--state",
                    state,
                    "--scale",
                    scale,
                    "--width",
                    &width.to_string(),
                    "--height",
                    &height.to_string(),
                    "--input",
                ])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/evaluate-reference.json"
                ))
                .arg("--output")
                .arg(&path)
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DBUS_SESSION_BUS_ADDRESS")
                .env_remove("POE2COMPANION_RELOAD_FD")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let image = fs::read(&path).unwrap();
            assert_eq!(u32::from_be_bytes(image[16..20].try_into().unwrap()), width);
            assert_eq!(
                u32::from_be_bytes(image[20..24].try_into().unwrap()),
                height
            );
            let data: Value =
                serde_json::from_slice(&fs::read(path.with_extension("json")).unwrap()).unwrap();
            assert_eq!(data["screen"]["width"], width);
            assert_eq!(data["screen"]["height"], height);
            let bounds = &data["bounds"];
            assert!(
                bounds["x"].as_u64().unwrap() + bounds["width"].as_u64().unwrap() <= width as u64
            );
            assert!(
                bounds["y"].as_u64().unwrap() + bounds["height"].as_u64().unwrap() <= height as u64
            );
        }
    }
}
