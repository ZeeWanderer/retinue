use std::{fs, panic::catch_unwind};

#[path = "../build/support.rs"]
mod support;

#[test]
fn build_guard_rejects_version_and_protocol_drift() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("apps/poe2companion");
    let protocol_dir = dir.path().join("apps/retinue_daemon/src/runtime");
    fs::create_dir_all(&manifest).unwrap();
    fs::create_dir_all(&protocol_dir).unwrap();
    let scripts = dir.path().join("scripts");
    fs::create_dir(&scripts).unwrap();
    let font = dir.path().join("fixture.ttf");
    fs::write(&font, "font fixture").unwrap();
    fs::write(
        scripts.join("setup-fontin.py"),
        format!("print({:?})\n", font.to_str().unwrap()),
    )
    .unwrap();
    for (version, envelope, session, trade, expected_error) in [
        (env!("CARGO_PKG_VERSION"), 2, 2, 5, None),
        ("999.0.0", 2, 2, 5, Some("version must match root VERSION")),
        (
            env!("CARGO_PKG_VERSION"),
            3,
            2,
            2,
            Some("supports envelope 2"),
        ),
        (
            env!("CARGO_PKG_VERSION"),
            2,
            1,
            2,
            Some("supports session 2"),
        ),
        (
            env!("CARGO_PKG_VERSION"),
            2,
            3,
            2,
            Some("supports session 2"),
        ),
        (env!("CARGO_PKG_VERSION"), 2, 2, 4, Some("supports trade 5")),
    ] {
        fs::write(dir.path().join("VERSION"), format!("{version}\n")).unwrap();
        fs::write(
            protocol_dir.join("retinue_local_protocol.erl"),
            format!(
                "-define(ENVELOPE_VERSION, {envelope}).\n-define(INTERFACE_SESSION, {session}).\n-define(INTERFACE_TRADE, {trade}).\n"
            ),
        )
        .unwrap();
        let result =
            catch_unwind(|| support::prepare(&manifest, env!("CARGO_PKG_VERSION"), dir.path()));
        if let Some(message) = expected_error {
            let error = result.expect_err("build guard accepted drift");
            let error = error
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| error.downcast_ref::<&str>().copied())
                .unwrap();
            assert!(error.contains(message), "{error}");
        } else {
            result.unwrap();
            assert_eq!(
                fs::read(dir.path().join("Fontin-SmallCaps.ttf")).unwrap(),
                b"font fixture"
            );
        }
    }
}
