use super::*;
use crate::game_observer::memory::CaptureBlock;
use std::collections::BTreeMap;
use std::time::Duration;

// Arbitrary bindings: runtime reads must use the discovered layout, not game RVAs.
const HUD_VTABLE: u64 = 0x1200;
const NATIVE_VTABLE: u64 = 0x2300;
const FLASH_VTABLE: u64 = 0x3400;
const MODE: u64 = 0x4500;

fn layout() -> Layout {
    Layout {
        schema: layout::SCHEMA,
        hud_vtable_rva: HUD_VTABLE,
        native_vtable_rva: NATIVE_VTABLE,
        flash_vtable_rva: FLASH_VTABLE,
        mode_rva: MODE,
        evidence: Default::default(),
    }
}

fn baseline() -> Snapshot {
    Snapshot {
        native: 0x11000,
        counter: 900,
        mode: 1,
        records: (1..=SLOTS)
            .map(|slot| Record {
                slot,
                address: 0x30000 + slot as u64 * 0x1000,
                sequence: 844 + slot as u16,
                text: "47".into(),
                displayed_amount: Some(47),
                glyph_tokens: vec![],
                rgb: 0xffffff,
                critical_tier: 0,
                remaining_seconds: 0.0,
                retired: true,
                relative_anchor: false,
                anchor_handle: 0,
                flags_hex: "01000000".into(),
                raw_hex: "00".repeat(0x190),
            })
            .collect(),
    }
}

fn hit(snapshot: &mut Snapshot, slot: usize, sequence: u16, text: &str) {
    let record = &mut snapshot.records[slot - 1];
    record.sequence = sequence;
    record.text = text.into();
    (record.displayed_amount, record.glyph_tokens) = display_text(text);
    record.retired = false;
    record.remaining_seconds = 1.0;
}

#[test]
fn baseline_is_not_history_and_identical_hits_have_distinct_sequences() {
    let mut state = baseline();
    let mut tracker = Tracker::new(state.clone());
    assert!(tracker.update(state.clone()).is_empty());
    state.records[4].text = "53".into();
    assert!(tracker.update(state.clone()).is_empty());
    assert!(tracker.update(state.clone()).is_empty());
    for sequence in 900..903 {
        hit(&mut state, 1, sequence, "47");
        state.counter = sequence + 1;
        assert!(tracker.update(state.clone()).is_empty());
        let events = tracker.update(state.clone());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["record"]["sequence"], sequence);
    }
    assert_eq!(tracker.counts.displays, 3);
    state.records[0].remaining_seconds = 0.0;
    state.records[0].retired = true;
    assert!(tracker.update(state).is_empty());
}

#[test]
fn counter_before_payload_waits_and_revisions_keep_identity() {
    let mut state = baseline();
    let mut tracker = Tracker::new(state.clone());
    state.counter += 1;
    for _ in 0..5 {
        assert!(tracker.update(state.clone()).is_empty());
    }
    hit(&mut state, 1, 900, "47");
    assert!(tracker.update(state.clone()).is_empty());
    assert_eq!(tracker.update(state.clone())[0]["kind"], "display");
    hit(&mut state, 1, 900, "47<DT_IMPACT_OUTLINE>");
    assert!(tracker.update(state.clone()).is_empty());
    let events = tracker.update(state);
    assert_eq!(events[0]["kind"], "record_update");
    assert_eq!(events[0]["ordinal"], 0);
    assert_eq!(tracker.counts.gaps, 0);
}

#[test]
fn modulo_wrap_and_overwritten_records_are_explicit() {
    let mut state = baseline();
    state.counter = 998;
    let mut tracker = Tracker::new(state.clone());
    for (slot, sequence) in [(1, 998), (2, 999), (3, 0)] {
        hit(&mut state, slot, sequence, "20");
    }
    state.counter = 1;
    assert!(tracker.update(state.clone()).is_empty());
    let events = tracker.update(state);
    assert_eq!(
        events
            .iter()
            .map(|v| v["record"]["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [998, 999, 0]
    );

    let mut state = baseline();
    let mut tracker = Tracker::new(state.clone());
    state.counter = 970;
    for index in 0..SLOTS {
        hit(&mut state, index + 1, 915 + index as u16, "20");
    }
    let gaps = tracker.update(state.clone());
    assert_eq!(gaps.len(), 15);
    assert!(gaps.iter().all(|v| v["kind"] == "gap"));
    assert_eq!(tracker.update(state).len(), 55);
    assert_eq!(tracker.counts.gaps, 15);
}

#[test]
fn stale_record_matching_wrapped_sequence_is_not_fresh_evidence() {
    let mut state = baseline();
    state.records[0].sequence = 900;
    let mut tracker = Tracker::new(state.clone());
    state.counter = 901;
    assert!(tracker.update(state.clone()).is_empty());
    assert!(tracker.update(state).is_empty());
    assert_eq!(tracker.pending().len(), 1);
    assert_eq!(tracker.counts.displays, 0);
}

#[test]
fn raw_text_is_retained_without_inventing_damage_or_status_semantics() {
    assert_eq!(
        display_text("74<DT_SLASH_OUTLINE>"),
        (Some(74), vec!["DT_SLASH_OUTLINE".into()])
    );
    assert_eq!(display_text("1.2K").0, None);
    assert_eq!(display_text("1,200").0, None);
    assert_eq!(display_text("").0, None);
    assert_eq!(display_text("47<unfinished").0, None);
}

fn store64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn store32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(mode: u64, labels_valid: bool) -> ProcessMemory {
    let base = 0x140000000;
    let mut chunks = BTreeMap::new();
    let mut header = vec![0; 0x3b8];
    store64(&mut header, 0, base + HUD_VTABLE);
    store64(&mut header, 0x28, 0x14000);
    store64(&mut header, 0x30, 0x14008);
    store64(&mut header, 0x3a8, 0x15000);
    store32(&mut header, 0x3b0, (SLOTS * 8) as u32);
    store32(&mut header, 0x3b4, 448);
    chunks.insert(0x10000, header);
    let mut native = vec![0; 0xc40];
    store64(&mut native, 0, base + NATIVE_VTABLE);
    native[0x2d8..0x2da].copy_from_slice(&900_u16.to_le_bytes());
    store64(&mut native, 0xc30, 0x15000);
    store32(&mut native, 0xc38, (SLOTS * 8) as u32);
    store32(&mut native, 0xc3c, 448);
    chunks.insert(0x11000, native);
    chunks.insert(0x12000, (base + FLASH_VTABLE).to_le_bytes().to_vec());
    chunks.insert(
        0x14000,
        [0x11000_u64.to_le_bytes(), 0x12000_u64.to_le_bytes()].concat(),
    );
    chunks.insert(base + MODE, mode.to_le_bytes().to_vec());
    let mut pool = vec![0; SLOTS * 8];
    for index in 0..SLOTS {
        let handle = 0x20000 + index as u64 * 16;
        let address = 0x30000 + index as u64 * 0x1000;
        store64(&mut pool, index * 8, handle);
        chunks.insert(handle, address.to_le_bytes().to_vec());
        let mut raw = vec![0; 0x190];
        let label = if labels_valid {
            format!("TargetHit{}.Label", index + 1)
        } else {
            "wrong".into()
        };
        let label_address = 0x80000 + index as u64 * 0x200;
        store64(&mut raw, 0x170, label_address);
        store32(&mut raw, 0x178, label.len() as u32 | 0xf000_0000);
        raw[0x17f] = 255;
        raw[0x30..0x32].copy_from_slice(b"47");
        store32(&mut raw, 0x40, 0xffffff);
        chunks.insert(address, raw);
        chunks.insert(label_address, label.into_bytes());
    }
    chunks.insert(0x15000, pool);
    let mut bytes = Vec::new();
    let blocks = chunks
        .into_iter()
        .map(|(address, data)| {
            let block = CaptureBlock {
                address,
                length: data.len(),
                file_offset: bytes.len(),
            };
            bytes.extend(data);
            block
        })
        .collect();
    ProcessMemory::from_capture(1, "00010000-01000000 rw-p 0 00:00 0\n140000000-144000000 r--p 0 00:00 0 /test/Warframe.x64.exe\n".into(), blocks, bytes).unwrap()
}

#[test]
fn native_pool_layout_checks_both_owners_and_tagged_strings() {
    let memory = fixture(1, true);
    let state = snapshot(&memory, 0x140000000, &layout(), 0x10000)
        .unwrap()
        .unwrap();
    assert_eq!(state.counter, 900);
    assert_eq!(state.records.len(), 55);
    assert_eq!(state.records[0].displayed_amount, Some(47));
    assert_eq!(state.records[54].slot, 55);
    assert!(snapshot(&fixture(0, true), 0x140000000, &layout(), 0x10000).is_err());
    assert!(snapshot(&fixture(1, false), 0x140000000, &layout(), 0x10000).is_err());
    assert!(snapshot(&memory, 0x140000000, &layout(), 0x10008).is_err());
    let mut wrong = layout();
    wrong.hud_vtable_rva += 8;
    assert!(snapshot(&memory, 0x140000000, &wrong, 0x10000).is_err());
}

#[test]
fn inactive_hud_reports_its_lifecycle_state() {
    let mut header = vec![0; 0x3b8];
    store64(&mut header, 0, 0x140000000 + HUD_VTABLE);
    store64(&mut header, 0x28, 0x14000);
    store64(&mut header, 0x30, 0x14000);
    let blocks = vec![
        CaptureBlock {
            address: 0x10000,
            length: header.len(),
            file_offset: 0,
        },
        CaptureBlock {
            address: 0x14000,
            length: 8,
            file_offset: header.len(),
        },
    ];
    header.extend_from_slice(&0_u64.to_le_bytes());
    let memory = ProcessMemory::from_capture(
        1,
        "00010000-00015000 rw-p 0 00:00 0\n".into(),
        blocks,
        header,
    )
    .unwrap();
    let error = snapshot(&memory, 0x140000000, &layout(), 0x10000).unwrap_err();
    assert_eq!(
        error,
        "combat HUD is inactive; enter a mission or Simulacrum"
    );
}

#[test]
fn saved_snapshots_replay_through_the_same_decoder() {
    let path = std::env::temp_dir().join(format!(
        "wfinspect-combat-fixture-{}.jsonl",
        std::process::id()
    ));
    let options = Options {
        duration: Duration::from_secs(1),
        interval: Duration::from_millis(50),
        max_bytes: 1024 * 1024,
    };
    let mut state = baseline();
    let mut journal = Journal::create(
        &path,
        "wfinspect.combat-watch",
        json!({
            "memory": {"identity": {"executable": {"sha256": "synthetic"}}, "kind": "synthetic_test"},
            "owner": 0x10000, "baseline": state,
        }),
        options,
    )
    .unwrap();
    hit(&mut state, 1, 900, "47");
    state.counter = 901;
    journal
        .append("sample", &json!({"index": 0, "snapshot": state}))
        .unwrap();
    journal
        .append("sample", &json!({"index": 1, "snapshot": state}))
        .unwrap();
    journal.finish(&json!({"synthetic_test": true})).unwrap();
    drop(journal);
    let mut events = Vec::new();
    let report = replay(&path, |event| {
        events.push(event.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["events"]["displays"], 1);
    assert_eq!(events[0]["event"]["record"]["displayed_amount"], 47);
    // Retain the synthetic recording for manual CLI replay checks.
    eprintln!("combat replay fixture: {}", path.display());
}
