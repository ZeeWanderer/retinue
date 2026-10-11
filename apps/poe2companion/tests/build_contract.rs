use std::{fs, path::Path, process::Command};

#[test]
fn build_guard_rejects_version_and_protocol_drift() {
    let dir = tempfile::tempdir().unwrap();
    let guard = dir.path().join("contract-check");
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--edition=2024")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .arg("-o")
        .arg(&guard)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = dir.path().join("apps/poe2companion");
    let protocol_dir = dir.path().join("apps/retinue_daemon/src/runtime");
    fs::create_dir_all(&manifest).unwrap();
    fs::create_dir_all(&protocol_dir).unwrap();
    for (version, envelope, session, expected_error) in [
        (env!("CARGO_PKG_VERSION"), 2, 1, None),
        ("999.0.0", 2, 1, Some("version must match root VERSION")),
        (env!("CARGO_PKG_VERSION"), 3, 1, Some("supports envelope 2")),
        (env!("CARGO_PKG_VERSION"), 2, 2, Some("supports session 1")),
    ] {
        fs::write(dir.path().join("VERSION"), format!("{version}\n")).unwrap();
        fs::write(
            protocol_dir.join("retinue_local_protocol.erl"),
            format!(
                "-define(ENVELOPE_VERSION, {envelope}).\n-define(INTERFACE_SESSION, {session}).\n"
            ),
        )
        .unwrap();
        let output = Command::new(&guard)
            .env("CARGO_MANIFEST_DIR", &manifest)
            .env("CARGO_PKG_VERSION", env!("CARGO_PKG_VERSION"))
            .output()
            .unwrap();
        if let Some(message) = expected_error {
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        } else {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
