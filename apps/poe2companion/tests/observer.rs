use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Duration;

use poe2companion::{EventKind, MAX_EVENTS, READ_BUDGET, Tailer, Watcher, parse_line};

const AREA: &str = "2026/05/30 18:39:27 5919181 2caa233f [DEBUG Client 312] Generating level 1 area \"G1_1\" with seed 1850020125\r\n";
const NAME: &str =
    "2026/05/30 18:39:28 5919764 7fbd1225 [INFO Client 312] [SCENE] Set Source [The Riverbank]\r\n";
const LEVEL: &str = "2026/05/30 18:45:25 6276613 3ef23347 [INFO Client 312] : ExampleCharacter (Huntress) is now level 2\r\n";
const DEATH: &str =
    "2026/05/30 19:16:35 8147076 3ef23347 [INFO Client 312] : ExampleCharacter has been slain.\r\n";

fn append(path: &std::path::Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

#[test]
fn confirmed_anonymized_shapes_preserve_prefix_and_domain_fields() {
    let event = parse_line(AREA.as_bytes(), AREA.len() as u64).unwrap();
    assert_eq!(event.offset, AREA.len() as u64);
    assert_eq!(event.client_pid, 312);
    assert_eq!(event.tick_ms, 5919181);
    assert_eq!(event.timestamp_local, "2026/05/30 18:39:27");
    assert_eq!(
        event.data,
        EventKind::AreaGenerated {
            area_id: "G1_1".into(),
            area_level: 1,
            seed: 1850020125
        }
    );
    assert_eq!(
        parse_line(NAME.as_bytes(), 1).unwrap().data,
        EventKind::AreaName {
            area_name: "The Riverbank".into()
        }
    );
    assert_eq!(
        parse_line(LEVEL.as_bytes(), 1).unwrap().data,
        EventKind::CharacterLevel {
            character: "ExampleCharacter".into(),
            character_class: "Huntress".into(),
            character_level: 2
        }
    );
    assert_eq!(
        parse_line(DEATH.as_bytes(), 1).unwrap().data,
        EventKind::Death {
            character: "ExampleCharacter".into()
        }
    );
    let json = serde_json::to_value(event).unwrap();
    assert!(json.get("character_level").is_none());
    assert!(json.get("pid").is_none());
}

#[test]
fn current_proton_scene_is_parsed_after_cursor_only_progress() {
    let scene = "2026/10/11 16:55:39 9945934 7fbd1225 [INFO Client 480] [SCENE] Set Source [Beacon of Salvation Hideout]\r\n";
    let parsed = parse_line(scene.as_bytes(), 54_214_987).unwrap();
    assert_eq!(parsed.client_pid, 480);
    assert_eq!(parsed.tick_ms, 9_945_934);
    assert_eq!(parsed.timestamp_local, "2026/10/11 16:55:39");
    assert_eq!(
        parsed.data,
        EventKind::AreaName {
            area_name: "Beacon of Salvation Hideout".into()
        }
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, NAME).unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let initial = reader.next_batch().unwrap().unwrap();
    assert!(initial.events.is_empty());
    let ignored =
        b"2026/10/11 16:54:48 9895000 7fbd1225 [INFO Client 480] unrelated system line\r\n";
    append(&path, ignored);
    let watermark = reader.next_batch().unwrap().unwrap();
    assert!(watermark.events.is_empty());
    assert_eq!(watermark.cursor, (NAME.len() + ignored.len()) as u64);
    append(&path, scene.as_bytes());
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.generation, initial.generation);
    assert_eq!(batch.cursor, watermark.cursor + scene.len() as u64);
    let mut expected = parsed;
    expected.offset = batch.cursor;
    assert_eq!(batch.events, vec![expected]);
}

#[test]
fn unknown_private_and_spoofed_chat_never_become_events() {
    let prefix = "2026/06/22 12:36:51 4104371 7fbd1225 [INFO Client 332] ";
    for body in [
        "[SCENE] Set Source [(unknown)]",
        "[SCENE] Set Source [(null)]",
        ": #Player: ExampleCharacter has been slain.",
        ": @From Player: ExampleCharacter (Huntress) is now level 2",
        ": Player: ExampleCharacter has been slain.",
        ": %ExampleCharacter has been slain.",
        ": ExampleCharacter (Huntress) is now level 2000",
        "Connecting to instance server at 192.0.2.1:12345",
        "Generating level 1 area \"G1_1\" with seed 1",
    ] {
        assert!(
            parse_line(format!("{prefix}{body}").as_bytes(), 20).is_none(),
            "{body}"
        );
    }
    assert!(parse_line(DEATH.replace("312", "4294967296").as_bytes(), 1).is_none());
    assert!(parse_line(DEATH.replace("Client 312", "Client 0").as_bytes(), 1).is_none());
    assert!(parse_line(DEATH.as_bytes(), 0).is_none());
    assert!(parse_line(DEATH.replace("3ef23347", "invalid-hash").as_bytes(), 1).is_none());
    assert!(parse_line(b"\xff\n", 1).is_none());
}

#[test]
fn watch_starts_at_eof_and_discards_existing_partial_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, DEATH).unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let initial = reader.next_batch().unwrap().unwrap();
    assert!(initial.events.is_empty());
    assert_eq!(initial.cursor, DEATH.len() as u64);
    assert!(reader.next_batch().unwrap().is_none());
    append(&path, LEVEL.as_bytes());
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.cursor, (DEATH.len() + LEVEL.len()) as u64);
    assert_eq!(batch.generation, initial.generation);

    fs::write(&path, &DEATH.as_bytes()[..40]).unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    assert!(reader.next_batch().unwrap().is_none());
    append(&path, &DEATH.as_bytes()[40..]);
    append(&path, NAME.as_bytes());
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert!(matches!(batch.events[0].data, EventKind::AreaName { .. }));
}

#[test]
fn fragmented_utf8_and_crlf_keep_exclusive_line_end_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, b"").unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    reader.next_batch().unwrap();
    let line = DEATH.replace(
        "ExampleCharacter",
        "\u{0413}\u{0435}\u{0440}\u{043e}\u{0439}",
    );
    let split = line.find('\u{0413}').unwrap() + 1;
    append(&path, &line.as_bytes()[..split]);
    assert!(reader.next_batch().unwrap().is_none());
    append(&path, &line.as_bytes()[split..line.len() - 1]);
    assert!(reader.next_batch().unwrap().is_none());
    append(&path, b"\n");
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].offset, line.len() as u64);
    assert_eq!(batch.cursor, line.len() as u64);
}

#[test]
fn eof_restart_and_unknown_lines_only_advance_watermark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, DEATH).unwrap();
    let initial = Tailer::open(&path, true)
        .unwrap()
        .next_batch()
        .unwrap()
        .unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let restart = reader.next_batch().unwrap().unwrap();
    assert_eq!(restart.generation, initial.generation);
    assert_eq!(restart.cursor, initial.cursor);
    assert!(restart.events.is_empty());
    let unknown =
        "2026/05/30 19:16:36 8148076 3ef23347 [INFO Client 312] unrelated system line\r\n";
    append(&path, unknown.as_bytes());
    let batch = reader.next_batch().unwrap().unwrap();
    assert!(batch.events.is_empty());
    assert_eq!(batch.cursor, (DEATH.len() + unknown.len()) as u64);
    assert_eq!(batch.generation, initial.generation);
}

#[test]
fn replacement_after_missing_path_never_combines_partial_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, b"").unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let old = reader.next_batch().unwrap().unwrap();
    append(&path, &DEATH.as_bytes()[..40]);
    assert!(reader.next_batch().unwrap().is_none());
    fs::rename(&path, dir.path().join("Client.old")).unwrap();
    assert!(reader.next_batch().unwrap().is_none());
    fs::write(&path, NAME).unwrap();
    let initial = reader.next_batch().unwrap().unwrap();
    assert_ne!(initial.generation, old.generation);
    assert_eq!(initial.cursor, 0);
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].offset, NAME.len() as u64);
    assert!(matches!(batch.events[0].data, EventKind::AreaName { .. }));
}

#[test]
fn confirmed_scene_padding_is_trimmed() {
    let line = "2026/06/03 18:31:05 25500598 7fbd1225 [INFO Client 320] [SCENE] Set Source [Sel Khari Sanctuary ]\r\n";
    let event = parse_line(line.as_bytes(), line.len() as u64).unwrap();
    assert_eq!(
        event.data,
        EventKind::AreaName {
            area_name: "Sel Khari Sanctuary".into()
        }
    );
    assert_eq!(event.client_pid, 320);
    assert_eq!(event.tick_ms, 25500598);
}

#[test]
fn bounded_batches_drain_old_inode_before_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, b"").unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let original = reader.next_batch().unwrap().unwrap();
    append(&path, DEATH.repeat(270).as_bytes());
    fs::rename(&path, dir.path().join("Client.old")).unwrap();
    fs::write(&path, NAME).unwrap();
    let mut counts = Vec::new();
    let mut total = 0;
    let mut new_generation = None;
    loop {
        if let Some(batch) = reader.next_batch().unwrap() {
            assert!(batch.events.len() <= MAX_EVENTS);
            assert!(batch.events.windows(2).all(|w| w[0].offset < w[1].offset));
            assert!(
                batch
                    .events
                    .iter()
                    .all(|event| event.offset > 0 && event.offset <= batch.cursor)
            );
            if batch.generation == original.generation {
                counts.push(batch.events.len());
            } else {
                new_generation = Some(batch.generation);
            }
            total += batch.events.len();
        } else if !reader.has_more() {
            break;
        }
    }
    assert_eq!(counts, [128, 128, 14]);
    assert_eq!(total, 271);
    assert_ne!(new_generation.unwrap(), original.generation);
}

#[test]
fn truncate_and_regrow_resets_generation_even_beyond_old_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, NAME).unwrap();
    let mut reader = Tailer::open(&path, true).unwrap();
    let old = reader.next_batch().unwrap().unwrap();
    fs::write(&path, DEATH.repeat(4)).unwrap();
    let reset = reader.next_batch().unwrap().unwrap();
    assert_ne!(old.generation, reset.generation);
    assert_eq!(reset.cursor, 0);
    let batch = reader.next_batch().unwrap().unwrap();
    assert_eq!(batch.events.len(), 4);
    assert_eq!(batch.generation, reset.generation);
}

#[test]
fn unread_truncation_or_multiple_rotation_is_not_silent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    fs::write(&path, DEATH.repeat(4)).unwrap();
    let mut reader = Tailer::open(&path, false).unwrap();
    reader.next_batch().unwrap();
    fs::write(&path, b"").unwrap();
    assert!(
        reader
            .next_batch()
            .unwrap_err()
            .to_string()
            .contains("unread")
    );

    fs::write(&path, DEATH).unwrap();
    let mut reader = Tailer::open(&path, false).unwrap();
    fs::rename(&path, dir.path().join("old1")).unwrap();
    fs::write(&path, LEVEL).unwrap();
    reader.refresh().unwrap();
    fs::rename(&path, dir.path().join("old2")).unwrap();
    fs::write(&path, AREA).unwrap();
    assert!(
        reader
            .refresh()
            .unwrap_err()
            .to_string()
            .contains("multiple rotations")
    );
}

#[test]
fn missing_file_and_oversized_non_domain_lines_stay_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    let mut reader = Tailer::open(&path, true).unwrap();
    assert!(reader.next_batch().unwrap().is_none());
    fs::write(
        &path,
        [
            vec![b'x'; READ_BUDGET * 2],
            b"\n".to_vec(),
            DEATH.as_bytes().to_vec(),
        ]
        .concat(),
    )
    .unwrap();
    reader.next_batch().unwrap();
    assert!(reader.next_batch().unwrap().is_none());
    assert!(reader.has_more());
    let mut total = 0;
    loop {
        if let Some(batch) = reader.next_batch().unwrap() {
            total += batch.events.len();
        } else if !reader.has_more() {
            break;
        }
    }
    assert_eq!(total, 1);
}

#[tokio::test]
async fn inotify_wakes_on_file_creation_and_append() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Client.txt");
    let watcher = Watcher::new(&path).unwrap();
    fs::write(&path, AREA).unwrap();
    tokio::time::timeout(Duration::from_secs(2), watcher.changed())
        .await
        .unwrap()
        .unwrap();
    append(&path, NAME.as_bytes());
    tokio::time::timeout(Duration::from_secs(2), watcher.changed())
        .await
        .unwrap()
        .unwrap();
}
