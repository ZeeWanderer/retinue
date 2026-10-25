use super::*;
use crate::layout::{EntityLayout, GoldLayout, PanelLayout, SocialLayout};
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
    u64_at(&mut bytes, 0x116f0, BASE + 0x26000);
    u64_at(&mut bytes, 0x116f8, 1);
    for offset in [0, 8, 16] {
        u64_at(&mut bytes, 0x26000 + offset, BASE + 0x26100);
        u64_at(&mut bytes, 0x26100 + offset, BASE + 0x26000);
    }
    bytes[0x26019] = 1;
    u32_at(&mut bytes, 0x26120, 1);
    u64_at(&mut bytes, 0x26128, BASE + 0x13000);
    u32_at(&mut bytes, 0x13088, 1);
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
        social: Ok(SocialLayout {
            server_offset: 0x5b0,
            records_offset: 0x27d8,
            party_offset: 0x2300,
        }),
        entities: Ok(EntityLayout {
            tree_offset: 0x6f0,
            id_offset: 0x88,
        }),
        image_size: 0x10000,
        image_sha256: "0".repeat(64),
        component_getters: BTreeMap::from([
            (0x4000, "Stats".into()),
            (0x4100, "Player".into()),
            (0x4200, "Life".into()),
            (0x4300, "NPC".into()),
            (0x4400, "Monster".into()),
            (0x4500, "Render".into()),
            (0x4600, "ObjectMagicProperties".into()),
            (0x4700, "Positioned".into()),
        ]),
    };
    (memory, layout)
}

fn monster(bytes: &mut [u8]) {
    u64_at(bytes, 0x116f8, 2);
    u64_at(bytes, 0x26010, BASE + 0x26200);
    u64_at(bytes, 0x26110, BASE + 0x26200);
    for offset in [0, 16] {
        u64_at(bytes, 0x26200 + offset, BASE + 0x26000);
    }
    u64_at(bytes, 0x26208, BASE + 0x26100);
    u32_at(bytes, 0x26220, 2);
    u64_at(bytes, 0x26228, BASE + 0x27000);
    u32_at(bytes, 0x27088, 2);
    u64_at(bytes, 0x27010, BASE + 0x27100);
    u64_at(bytes, 0x27018, BASE + 0x27128);
    for (i, getter) in [0x4400, 0x4500, 0x4600, 0x4700, 0x4200]
        .into_iter()
        .enumerate()
    {
        let component = 0x28000 + i * 0x400;
        let table = 0x6000 + i * 0x100;
        u64_at(bytes, 0x27100 + i * 8, BASE + component as u64);
        u64_at(bytes, component, BASE + table as u64);
        u64_at(bytes, table + 16, BASE + getter);
    }
    string(bytes, 0x28560, "Boss");
    u64_at(bytes, 0x28570, 4);
    u64_at(bytes, 0x28578, 7);
    u32_at(bytes, 0x28944, 3);
    u32_at(bytes, 0x291e0, 5000);
}

#[test]
fn unique_monsters_use_typed_components_not_component_positions() {
    let (memory, layout) = fixture(monster);
    let snapshot = read(&memory, BASE, &layout).unwrap();
    let entities = snapshot.entities.unwrap();
    assert_eq!(entities.count, 2);
    assert_eq!(
        entities.unique_monsters,
        vec![UniqueMonster {
            id: 2,
            address: BASE + 0x27000,
            name: "Boss".into(),
            life: 5000,
        }]
    );
}

#[test]
fn tree_or_component_failures_are_not_empty_monsters() {
    for fault in 0..9 {
        let (memory, layout) = fixture(|bytes| {
            monster(bytes);
            match fault {
                0 => u64_at(bytes, 0x26100, BASE + 0x26100),
                1 => u64_at(bytes, 0x116f8, 4097),
                2 => bytes[0x26019] = 0,
                3 => u32_at(bytes, 0x27088, 3),
                4 => u64_at(bytes, 0x26208, BASE + 0x26000),
                5 => u64_at(bytes, 0x6010, BASE + 0x9999),
                6 => u32_at(bytes, 0x28944, 99),
                7 => u32_at(bytes, 0x291e0, u32::MAX),
                _ => u64_at(bytes, 0x28570, 1024),
            }
        });
        let snapshot = read(&memory, BASE, &layout).unwrap();
        assert!(snapshot.entities.is_none(), "fault {fault}");
        assert!(
            snapshot.issues.iter().any(|i| i.field == "entities"),
            "fault {fault}"
        );
        assert!(snapshot.player.is_some());
    }
}

#[test]
fn only_provider_eligible_unique_monsters_are_observed() {
    for fault in 0..4 {
        let (memory, layout) = fixture(|bytes| {
            monster(bytes);
            match fault {
                0 => u32_at(bytes, 0x28944, 2),
                1 => bytes[0x28de1] = 1,
                2 => u64_at(bytes, 0x6010, BASE + 0x4300),
                _ => {
                    u32_at(bytes, 0x26220, 0x40000002);
                    u32_at(bytes, 0x27088, 0x40000002);
                }
            }
        });
        assert!(
            read(&memory, BASE, &layout)
                .unwrap()
                .entities
                .unwrap()
                .unique_monsters
                .is_empty(),
            "fault {fault}"
        );
    }
}

#[test]
fn entity_fields_can_move() {
    let (memory, mut layout) = fixture(|bytes| {
        monster(bytes);
        bytes.copy_within(0x116f0..0x11700, 0x11780);
        bytes[0x116f0..0x11700].fill(0);
        u32_at(bytes, 0x13098, 1);
        u32_at(bytes, 0x27098, 2);
    });
    layout.entities = Ok(EntityLayout {
        tree_offset: 0x780,
        id_offset: 0x98,
    });
    let snapshot = read(&memory, BASE, &layout).unwrap();
    assert_eq!(snapshot.entities.unwrap().unique_monsters.len(), 1);
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

fn social_fixture(bytes: &mut [u8]) {
    for (at, value) in [
        (0x1a7d8, 0x28000),
        (0x1a7e0, 0x28210),
        (0x1a7e8, 0x28210),
        (0x1a300, 0x29000),
        (0x1a308, 0x29050),
        (0x1a310, 0x29050),
    ] {
        u64_at(bytes, at, BASE + value);
    }
    for (at, text) in [
        (0x28000, "Account"),
        (0x28040, "Exile"),
        (0x28060, "League"),
        (0x29008, "Account"),
        (0x29028, "Exile"),
    ] {
        string(bytes, at, text);
        u64_at(bytes, at + 16, text.len() as u64);
        u64_at(bytes, at + 24, 7);
    }
    bytes[0x28200] = 3;
    bytes[0x28201] = 10;
    bytes[0x28202] = 1;
}

#[test]
fn social_identity_matches_character_not_first_online_account() {
    let (memory, layout) = fixture(social_fixture);
    let social = read(&memory, BASE, &layout).unwrap().social.unwrap();
    let local = social.local.unwrap();
    assert_eq!(local.character, "Exile");
    assert_eq!(local.account, "Account");
    assert_eq!(local.league, "League");
    assert_eq!(social.party[0].league.as_deref(), Some("League"));
    assert_eq!(social.party[0].class_id, Some(3));
    assert_eq!(social.party[0].level, Some(10));

    let (memory, layout) = fixture(|bytes| {
        social_fixture(bytes);
        string(bytes, 0x28040, "Other");
    });
    let social = read(&memory, BASE, &layout).unwrap().social.unwrap();
    assert!(social.local.is_none());
    assert!(social.party[0].league.is_none());
    assert_eq!(social.party[0].character, "Exile");
}

#[test]
fn valid_empty_party_is_distinct_from_failed_social_read() {
    let (memory, layout) = fixture(|_| {});
    assert_eq!(
        read(&memory, BASE, &layout).unwrap().social,
        Some(Social {
            local: None,
            party: vec![]
        })
    );
    for fault in 0..7 {
        let (memory, mut layout) = fixture(|bytes| {
            social_fixture(bytes);
            match fault {
                0 => u64_at(bytes, 0x1a308, BASE + 0x29051),
                1 => u64_at(bytes, 0x1a7e8, BASE + 0x28000),
                2 => u64_at(bytes, 0x1a7e0, BASE + 0x28000 + 0x210 * 0x513),
                3 => bytes[0x28202] = 7,
                4 => u64_at(bytes, 0x28050, 4096),
                5 => string(bytes, 0x29008, "Changed"),
                _ => {}
            }
        });
        if fault == 6 {
            layout.social = Err("social signature not found".into());
        }
        let snapshot = read(&memory, BASE, &layout).unwrap();
        assert!(snapshot.social.is_none(), "fault {fault}");
        assert!(snapshot.player.is_some());
        assert!(snapshot.panels.contains_key("inventory"));
        assert_eq!(snapshot.issues[0].field, "social");
    }
}

#[test]
fn social_reserve_capacity_can_exceed_the_live_record_limit() {
    let (memory, layout) = fixture(|bytes| {
        social_fixture(bytes);
        u64_at(bytes, 0x1a7e8, BASE + 0x28000 + 0x210 * 0x600);
        u64_at(bytes, 0x1a310, BASE + 0x29000 + 0x50 * 96);
    });
    assert!(
        read(&memory, BASE, &layout)
            .unwrap()
            .social
            .unwrap()
            .local
            .is_some()
    );
}

#[test]
fn stale_or_ambiguous_social_records_never_identify_local_account() {
    for fault in 0..3 {
        let (memory, layout) = fixture(|bytes| {
            social_fixture(bytes);
            match fault {
                0 => bytes[0x28201] = 20,
                1 => bytes[0x28202] = 0,
                _ => {
                    bytes.copy_within(0x28000..0x28210, 0x28210);
                    u64_at(bytes, 0x1a7e0, BASE + 0x28420);
                    u64_at(bytes, 0x1a7e8, BASE + 0x28420);
                }
            }
        });
        let social = read(&memory, BASE, &layout).unwrap().social;
        assert!(social.is_none_or(|s| s.local.is_none()));
    }
}

#[test]
fn social_heap_strings_empty_names_and_unknown_leagues() {
    let (memory, layout) = fixture(|bytes| {
        social_fixture(bytes);
        for at in [0x28000, 0x29008] {
            u64_at(bytes, at, BASE + 0x2b000);
            u64_at(bytes, at + 16, 15);
            u64_at(bytes, at + 24, 31);
        }
        string(bytes, 0x2b000, "ExampleAccount1");
    });
    assert_eq!(
        read(&memory, BASE, &layout)
            .unwrap()
            .social
            .unwrap()
            .local
            .unwrap()
            .account,
        "ExampleAccount1"
    );

    for at in [0x28040, 0x28060] {
        let (memory, layout) = fixture(|bytes| {
            social_fixture(bytes);
            string(bytes, at, "");
            u64_at(bytes, at + 16, 0);
        });
        let social = read(&memory, BASE, &layout).unwrap().social.unwrap();
        assert!(social.local.is_none());
        assert_eq!(social.party.len(), 1);
        assert!(social.party[0].league.is_none());
    }
}

#[test]
fn missing_server_and_panels_do_not_erase_known_player_fields() {
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
        ["gold", "panels", "social"]
    );
}

#[test]
fn typed_read_replays_from_recorded_ranges_without_the_game() {
    use retinue_native::inspect::{capture, memory::SourceMetadata};
    let (memory, layout) = fixture(social_fixture);
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

#[test]
fn unique_monsters_replay_from_sparse_capture() {
    use retinue_native::inspect::{
        capture,
        memory::{OpenedSource, Source, SourceMetadata},
    };
    let (memory, layout) = fixture(monster);
    let memory = memory.record_reads(1024 * 1024, 16384);
    let expected = read(&memory, BASE, &layout).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("entities");
    capture::write_recorded(
        memory,
        SourceMetadata {
            kind: "live".into(),
            pid: 1,
            identity: None,
            capture: None,
        },
        &destination,
        1,
        1,
    )
    .unwrap();
    let replay = OpenedSource::open(Source::Capture(destination)).unwrap();
    assert_eq!(read(&replay.memory, BASE, &layout).unwrap(), expected);
}
