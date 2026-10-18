use super::*;
use crate::layout::{GoldLayout, PanelLayout};
use retinue_native::process::CaptureBlock;

const BASE: u64 = 0x100000;

fn u64_at(bytes: &mut [u8], address: usize, value: u64) {
    bytes[address..address + 8].copy_from_slice(&value.to_le_bytes());
}

fn u32_at(bytes: &mut [u8], address: usize, value: u32) {
    bytes[address..address + 4].copy_from_slice(&value.to_le_bytes());
}

fn string(bytes: &mut [u8], address: usize, value: &str) {
    for (i, unit) in value.encode_utf16().chain([0]).enumerate() {
        bytes[address + i * 2..address + i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
}

fn fixture(patch: impl FnOnce(&mut [u8])) -> (ProcessMemory, Layout) {
    let mut bytes = vec![0; 0x40000];
    u64_at(&mut bytes, 0x1000, BASE + 0x10000);
    for index in 0..12 {
        u64_at(
            &mut bytes,
            0x10050 + index * 16,
            BASE + 0x30000 + index as u64 * 0x400,
        );
    }
    for (at, value) in [
        (0x10028, 0x10150),
        (0x10030, 0x10158),
        (0x10150, 0x31000),
        (0x31290, 0x11000),
        (0x115b8, 0x12000),
        (0x115c0, 0x12008),
        (0x12000, 0x13000),
        (0x13010, 0x14000),
        (0x13018, 0x14018),
        (0x14000, 0x15000),
        (0x14008, 0x16000),
        (0x14010, 0x17000),
        (0x15000, 0x5100),
        (0x16000, 0x5000),
        (0x17000, 0x5200),
        (0x5010, 0x4000),
        (0x5110, 0x4100),
        (0x5210, 0x4200),
        (0x115b0, 0x18000),
        (0x18060, 0x20000),
        (0x201b0, 0x21000),
        (0x11098, 0x1b000),
        (0x1b008, 0x1b040),
        (0x3008, 0x19000),
        (0x19008, 0x1a000),
        (0x1a6f0, 0x25000),
    ] {
        u64_at(&mut bytes, at, BASE + value);
    }
    string(&mut bytes, 0x151b0, "Exile");
    u64_at(&mut bytes, 0x151c0, 5);
    u64_at(&mut bytes, 0x151c8, 7);
    bytes[0x15204] = 10;
    u32_at(&mut bytes, 0x151d8, 123456);
    u32_at(&mut bytes, 0x171e0, 321);
    u32_at(&mut bytes, 0x249d0, 450);
    string(&mut bytes, 0x1b040, "Example Hideout");
    bytes[0x25169] = 8;
    patch(&mut bytes);
    let memory = ProcessMemory::from_capture(
        1,
        format!(
            "{BASE:x}-{:x} rw-p 00000000 00:00 0\n",
            BASE + bytes.len() as u64
        ),
        vec![CaptureBlock {
            address: BASE,
            length: bytes.len(),
            file_offset: 0,
        }],
        bytes,
    )
    .unwrap();
    let layout = Layout {
        root_rva: 0x1000,
        ingame_data_offset: 0x290,
        gold: Ok(GoldLayout {
            server_offset: 0x5b0,
            data_offset: 0x1b0,
            value_offset: 0x39d0,
        }),
        panels: Ok(PanelLayout {
            root_rva: 0x3000,
            flags_offset: 0x168,
        }),
        image_size: 0x10000,
        image_sha256: "0".repeat(64),
        component_getters: BTreeMap::from([
            (0x4000, "Stats".into()),
            (0x4100, "Player".into()),
            (0x4200, "Life".into()),
        ]),
    };
    (memory, layout)
}

#[test]
fn reads_player_area_and_panels_without_executing_getters() {
    let (memory, layout) = fixture(|_| {});
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.scene, "in_game");
    let player = snapshot.player.unwrap();
    assert_eq!(player.name, "Exile");
    assert_eq!(player.level, 10);
    assert_eq!(player.experience, 123456);
    assert_eq!(player.gold, Some(450));
    assert_eq!(player.life, Some(321));
    assert!(snapshot.area.unwrap().in_town);
    assert_eq!(snapshot.panels["inventory"], Some(true));
    assert_eq!(snapshot.panels["stash"], None);
}

#[test]
fn heap_names_use_length_and_capacity_not_an_unbounded_c_string() {
    let (memory, layout) = fixture(|bytes| {
        u64_at(bytes, 0x151b0, BASE + 0x28000);
        string(bytes, 0x28000, "LongCharacterName");
        u64_at(bytes, 0x151c0, 17);
        u64_at(bytes, 0x151c8, 31);
    });
    assert_eq!(
        read(&memory, BASE, &layout).unwrap().player.unwrap().name,
        "LongCharacterName"
    );
}

#[test]
fn bad_player_data_remains_unknown_instead_of_zero() {
    for fault in 0..4 {
        let (memory, mut layout) = fixture(|bytes| match fault {
            0 => u64_at(bytes, 0x13018, BASE + 0x15000),
            1 => u64_at(bytes, 0x151c0, 8192),
            2 => bytes[0x15204] = 255,
            _ => {}
        });
        if fault == 3 {
            layout.component_getters.clear();
        }
        let snapshot = read(&memory, BASE, &layout).unwrap();
        assert!(snapshot.player.is_none());
        assert_eq!(snapshot.issues[0].field, "player");
        assert!(snapshot.area.is_some());
    }
}

#[test]
fn loading_suppresses_player_and_escape_has_scene_precedence() {
    let (memory, layout) = fixture(|bytes| {
        u64_at(bytes, 0x10030, BASE + 0x10160);
        u64_at(bytes, 0x10158, BASE + 0x30000);
    });
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.scene, "loading");
    assert!(snapshot.player.is_none());
    assert_eq!(
        scene(&["InGameState".into(), "EscapeState".into()]),
        "escape_menu"
    );
}

#[test]
fn changed_state_table_is_not_guessed() {
    let (memory, layout) = fixture(|bytes| u64_at(bytes, 0x10150, BASE + 0x28000));
    assert!(
        read(&memory, BASE, &layout)
            .unwrap_err()
            .contains("unknown active")
    );
}

#[test]
fn active_shared_pointer_control_blocks_are_not_treated_as_states() {
    let (memory, layout) = fixture(|bytes| {
        u64_at(bytes, 0x10030, BASE + 0x10160);
        u64_at(bytes, 0x10158, BASE + 0x30ff0);
    });
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.states, ["InGameState"]);
    assert!(snapshot.player.is_some());
}

#[test]
fn missing_optional_anchors_leave_core_readers_enabled() {
    let (memory, mut layout) = fixture(|_| {});
    layout.gold = Err("gold signature not found".into());
    layout.panels = Err("UI signature not found".into());
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.player.unwrap().experience, 123456);
    assert_eq!(snapshot.issues.len(), 2);
}

#[test]
fn missing_gold_and_panels_do_not_erase_known_player_fields() {
    let (memory, layout) = fixture(|bytes| {
        u64_at(bytes, 0x115b0, 0);
        u64_at(bytes, 0x3008, 0);
    });
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.player.unwrap().experience, 123456);
    assert_eq!(
        snapshot
            .issues
            .iter()
            .map(|i| i.field.as_str())
            .collect::<Vec<_>>(),
        ["gold", "panels"]
    );
}

#[test]
fn typed_read_replays_from_recorded_ranges_without_the_game() {
    use retinue_native::inspect::{capture, memory::SourceMetadata};
    let (memory, layout) = fixture(|_| {});
    let memory = memory.record_reads(1024 * 1024, 16384);
    let expected = read(&memory, BASE, &layout).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("poe2");
    capture::write_recorded(
        memory,
        SourceMetadata {
            kind: "live".into(),
            pid: 1,
            identity: None,
            capture: None,
        },
        &destination,
        123,
        4,
    )
    .unwrap();
    let manifest = serde_json::json!({"schema":1, "layout":layout,
        "process":{"pid":1,"started":99,"image_base":BASE,"executable":"/fixture/PathOfExileSteam.exe"}});
    std::fs::write(
        destination.join("poe2.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::inspect::replay(&destination)
            .unwrap()
            .snapshot
            .unwrap(),
        expected
    );
    let mut wrong = manifest;
    wrong["process"]["pid"] = serde_json::json!(2);
    std::fs::write(
        destination.join("poe2.json"),
        serde_json::to_vec(&wrong).unwrap(),
    )
    .unwrap();
    assert!(
        crate::inspect::replay(&destination)
            .err()
            .unwrap()
            .contains("identity mismatch")
    );
}
