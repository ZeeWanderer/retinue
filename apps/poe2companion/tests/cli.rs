use std::fs;
use std::process::Command;

use serde_json::Value;

#[test]
fn inspect_is_explicit_read_only_and_ignores_chat() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    let content = concat!(
        "2026/05/30 18:45:25 6276613 3ef23347 [INFO Client 312] : ExampleCharacter (Huntress) is now level 2\r\n",
        "2026/05/30 18:45:26 6276614 3ef23347 [INFO Client 312] : @From Private: private content\r\n",
        "2026/05/30 18:45:27 6276615 3ef23347 [INFO Client 312] Connecting to instance server at 192.0.2.1:12345\r\n"
    );
    fs::write(&path, content).unwrap();
    let socket = dir.path().join("must-not-create.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["inspect", "--log"])
        .arg(&path)
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "character_level");
    assert_eq!(records[0]["client_pid"], 312);
    assert_eq!(fs::read(&path).unwrap(), content.as_bytes());
    assert!(!socket.exists());
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_poe2companion"))
            .arg("inspect")
            .output()
            .unwrap()
            .status
            .success()
    );
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
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["app"], "poe2companion");
    let entries = report["paths"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["kind"], "runtime");
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
