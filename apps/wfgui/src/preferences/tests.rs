use super::*;
use clap::Parser;

fn paths(root: &Path) -> Paths {
    Paths::from_lookup(|key| match key {
        "HOME" => Some(root.into()),
        _ => None,
    })
}

#[test]
fn shared_model_tracks_only_actual_edits() {
    let preferences = Preferences::default();
    let other_page = preferences.clone();
    preferences.set_scale(100);
    assert!(preferences.take_changes().is_empty());
    preferences.set_scale(125);
    preferences.toggle_favorite("/Lotus/Weapons/Example");
    assert!(other_page.is_favorite("/Lotus/Weapons/Example"));
    assert_eq!(other_page.scale(), 1.25);
    let changes = other_page.take_changes();
    assert_eq!(changes.ui_scale, Some(125));
    assert_eq!(changes.favorites.len(), 1);
    assert!(preferences.take_changes().is_empty());
}

#[test]
fn merges_edits_from_independent_windows_and_preserves_other_keys() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("gui.json");
    std::fs::write(
        &file,
        br#"{"ui_scale":120,"favorites":["existing"],"other":{"setting":true}}"#,
    )
    .unwrap();
    let first = Preferences::new(Values::load(&file).unwrap());
    let second = Preferences::new(Values::load(&file).unwrap());
    first.toggle_favorite("first");
    second.toggle_favorite("second");
    second.toggle_favorite("existing");
    first.set_scale(125);
    save(&file, &first.take_changes()).unwrap();
    save(&file, &second.take_changes()).unwrap();
    let saved = Values::load(&file).unwrap();
    assert_eq!(saved.ui_scale, 125);
    assert_eq!(
        saved.favorites,
        BTreeSet::from(["first".into(), "second".into()])
    );
    assert_eq!(saved.extra["other"], serde_json::json!({"setting":true}));
    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&file).unwrap());
    save(
        &file,
        &Changes {
            ui_scale: Some(125),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&file).unwrap()),
        inode
    );
}

#[test]
fn defaults_do_not_create_config_and_invalid_files_are_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("gui.json");
    assert_eq!(Values::load(&file).unwrap(), Values::default());
    save(&file, &Changes::default()).unwrap();
    assert!(!file.exists());
    for bytes in [
        b"broken".as_slice(),
        br#"{"ui_scale":1}"#,
        br#"{"favorites":true}"#,
    ] {
        std::fs::write(&file, bytes).unwrap();
        assert!(Values::load(&file).is_err());
        assert!(
            save(
                &file,
                &Changes {
                    ui_scale: Some(125),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
}

#[test]
fn writes_through_symlinked_parent_and_flushes_on_close() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("drive");
    std::fs::create_dir(&target).unwrap();
    let linked = root.path().join("config");
    std::os::unix::fs::symlink(&target, &linked).unwrap();
    let file = linked.join("gui.json");
    let writer = Writer::new(file.clone(), |result| result.unwrap()).unwrap();
    let preferences = Preferences::default();
    for scale in [105, 110, 115, 120, 125] {
        preferences.set_scale(scale);
        writer.submit(preferences.take_changes()).unwrap();
    }
    preferences.toggle_favorite("stable-id");
    writer.submit(preferences.take_changes()).unwrap();
    drop(writer);
    let restarted = Preferences::new(Values::load(&file).unwrap());
    assert_eq!(restarted.scale(), 1.25);
    assert!(restarted.is_favorite("stable-id"));
    assert!(linked.is_symlink());
    assert!(target.join("gui.json").is_file());
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&file).unwrap().permissions())
            & 0o777,
        0o600
    );
}

#[test]
fn failed_save_is_reported_and_pending_edits_survive_retry() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("gui.json");
    std::fs::write(&file, b"broken").unwrap();
    let (sent, received) = mpsc::channel();
    let writer = Writer::new(file.clone(), move |result| {
        sent.send(result).unwrap();
    })
    .unwrap();
    let preferences = Preferences::default();
    preferences.toggle_favorite("stable-id");
    writer.submit(preferences.take_changes()).unwrap();
    assert!(
        received
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err()
    );
    std::fs::remove_file(&file).unwrap();
    preferences.set_scale(130);
    writer.submit(preferences.take_changes()).unwrap();
    received
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    drop(writer);
    let saved = Values::load(&file).unwrap();
    assert!(saved.favorites.contains("stable-id"));
    assert_eq!(saved.ui_scale, 130);
}

#[test]
fn lock_contention_does_not_clobber_settings_or_wait_indefinitely() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("gui.json");
    let lock = File::create(file.with_extension("lock")).unwrap();
    lock.lock().unwrap();
    let error = save(
        &file,
        &Changes {
            ui_scale: Some(125),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert!(!file.exists());
}

#[test]
fn launch_override_is_temporary_and_captures_ignore_saved_preferences() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    let file = path(&paths);
    let preferences = Preferences::default();
    preferences.set_scale(125);
    preferences.toggle_favorite("saved");
    save(&file, &preferences.take_changes()).unwrap();
    let (loaded, error) = crate::Args::parse_from(["wfgui"]).preferences(&paths);
    assert!(error.is_none());
    assert_eq!(loaded.scale(), 1.25);
    assert!(loaded.is_favorite("saved"));
    let args = crate::Args::parse_from(["wfgui", "--ui-scale", "140"]);
    let (overridden, _) = args.preferences(&paths);
    assert_eq!(overridden.scale(), 1.4);
    assert!(overridden.take_changes().is_empty());
    overridden.toggle_favorite("added");
    save(&file, &overridden.take_changes()).unwrap();
    assert_eq!(Values::load(&file).unwrap().ui_scale, 125);
    for options in [
        vec!["--fixture"],
        vec!["--screenshot", "image.png"],
        vec!["--window-screenshot", "image.png"],
    ] {
        let args = crate::Args::parse_from(std::iter::once("wfgui").chain(options));
        assert!(!args.save_preferences());
        let (preview, error) = args.preferences(&paths);
        assert!(error.is_none());
        assert_eq!(preview.scale(), 1.0);
        assert!(!preview.is_favorite("saved"));
    }
}

#[test]
fn corrupt_preferences_do_not_prevent_startup() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    let file = path(&paths);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"broken").unwrap();
    let (values, error) = crate::Args::parse_from(["wfgui"]).preferences(&paths);
    assert_eq!(values.scale(), 1.0);
    assert!(error.unwrap().contains("left unchanged"));
}
