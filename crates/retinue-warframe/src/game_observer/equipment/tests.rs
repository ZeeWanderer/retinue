use super::*;
use crate::game_observer::executable::fixture::{
    emit, image, put32, relative, resource_code, resources,
};
use crate::game_observer::memory::CaptureBlock;

struct Fixture {
    executable: Vec<u8>,
    mapped: Vec<u8>,
    heap: Vec<u8>,
    base: u64,
}

const HEAP: u64 = 0x80000;
const WEAPON: u64 = HEAP + 0x1000;
const PATHS: [&str; 5] = [
    "/Lotus/Weapons/Grineer/LongGuns/GrineerM16Homage/GrineerM16Rifle",
    "/Lotus/Upgrades/Skins/HolsterCustomizations/RifleUpperBack",
    "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod",
    "/Lotus/Upgrades/Mods/Rifle/WeaponStatusChanceSPMod",
    "/Lotus/Upgrades/Mods/Rifle/WeaponStatusChanceSPSubMod",
];

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

impl Fixture {
    fn new(base: u64, shift: usize, fields: u64) -> Self {
        let mut bytes = image();
        let caller = 0x1000 + shift;
        emit(&mut bytes, caller, "48 8d 15 00 00 00 00");
        relative(&mut bytes, caller + 3, 0x2000);
        emit(
            &mut bytes,
            caller + 0x20,
            "49 8b 07 49 8b cf ff 90 00 00 00 00 48 85 c0 74 4e 90
             49 8b 07 48 8b d3 49 8b cf ff 90 00 00 00 00 90
             49 8b 07 49 8b cf 48 ff c3 ff 90 00 00 00 00 48 3b d8 72 b6",
        );
        for (offset, slot) in [(8, 0x80), (29, 0x90), (45, 0x80)] {
            put32(&mut bytes, caller + 0x20 + offset, slot + shift as u32 / 8);
        }
        let vector = 0x180 + fields;
        emit(
            &mut bytes,
            0x1200 + shift,
            "8b 81 00 00 00 00 48 c1 e8 04 c3",
        );
        put32(&mut bytes, 0x1202 + shift, (vector + 8) as u32);
        emit(
            &mut bytes,
            0x1240 + shift,
            "8b 81 00 00 00 00 48 c1 e8 04 48 3b d0 72 03 33 c0 c3
             48 8b 81 00 00 00 00 48 03 d2 48 8b 04 d0 48 8b 00 c3",
        );
        put32(&mut bytes, 0x1242 + shift, (vector + 8) as u32);
        put32(&mut bytes, 0x1255 + shift, vector as u32);
        if fields != 0 {
            bytes[0x1209 + shift] = 5;
            bytes[0x1249 + shift] = 5;
            emit(
                &mut bytes,
                0x1259 + shift,
                "48 c1 e2 02 48 8b 44 d0 08 48 8b 40 08 c3",
            );
        }
        emit(
            &mut bytes,
            0x1300 + shift,
            "44 8b 01 48 8b da 48 8b 05 00 00 00 00 41 0f b7 c8 48 03 c9
             49 c1 e8 10 48 8b 0c c8 49 03 c8 48 89 0a",
        );
        relative(&mut bytes, 0x1309 + shift, 0x3000 + shift);
        let registration = 0x1400 + shift;
        emit(
            &mut bytes,
            registration,
            "48 8d 15 00 00 00 00 48 8d 4c 24 50 e8 00 00 00 00
             48 8d 15 00 00 00 00 48 8d 4c 24 58 8b 18 e8 00 00 00 00
             44 8b c3 8b 10 48 8d 0d 00 00 00 00 c6 44 24 20 00 e8 00 00 00 00",
        );
        relative(&mut bytes, registration + 3, 0x2040);
        relative(&mut bytes, registration + 20, 0x2060);
        relative(&mut bytes, registration + 44, 0x3100 + shift);
        relative(&mut bytes, registration + 54, 0x1600 + shift);
        resource_code(
            &mut bytes,
            0x1600 + shift,
            resources((0x3000 + shift) as u64, fields),
        );
        for (offset, text) in [
            (0x2000, "AddAttachedUpgradesToWeapon: "),
            (0x2040, "LotusWeapon"),
            (0x2060, "/Lotus/Types/Game/"),
        ] {
            bytes[offset..offset + text.len()].copy_from_slice(text.as_bytes());
        }
        put32(&mut bytes, 0x104, 16);
        put32(&mut bytes, 0x120, 0x2e00);
        put32(&mut bytes, 0x124, 24);
        for (row, start, end) in [
            (0, caller, caller + 0x100),
            (12, 0x1600 + shift, 0x1640 + shift),
        ] {
            put32(&mut bytes, 0x2e00 + row, start as u32);
            put32(&mut bytes, 0x2e04 + row, end as u32);
        }
        let mut mapped = bytes.clone();
        let mut heap = vec![0; 0x8000];
        let vtable = 0x2200 + shift;
        put64(&mut mapped, vtable, base + (0x1200 + shift) as u64);
        put64(
            &mut mapped,
            vtable + 0x80 + shift / 8,
            base + (0x1200 + shift) as u64,
        );
        put64(
            &mut mapped,
            vtable + 0x90 + shift / 8,
            base + (0x1240 + shift) as u64,
        );
        put64(&mut mapped, 0x3000 + shift, HEAP + 0x6800);
        put64(&mut heap, 0x6800, HEAP + 0x7000);
        let resource = resources((0x3000 + shift) as u64, fields);
        for (index, path) in PATHS.iter().enumerate() {
            let object = if index == 0 {
                0x1000
            } else {
                0x4000 + (index - 1) * 0x100
            };
            let descriptor = 0x6000 + index * 0x100;
            let holder = 0x5000 + index * 0x20;
            put64(&mut heap, object, base + vtable as u64);
            put64(
                &mut heap,
                object + resource.object_type_offset as usize,
                HEAP + descriptor as u64,
            );
            put64(
                &mut heap,
                object + resource.object_holder_offset as usize,
                HEAP + holder as u64,
            );
            put64(
                &mut heap,
                holder + if fields == 0 { 0 } else { 8 },
                HEAP + object as u64,
            );
            if index == 0 {
                put64(
                    &mut heap,
                    descriptor + resource.parent_offset as usize,
                    base + (0x3100 + shift) as u64,
                );
            }
            let text = (index + 1) * 0x100;
            put32(
                &mut heap,
                descriptor + resource.name_leaf_offset as usize,
                (text << 16) as u32,
            );
            heap[0x7000 + text..0x7000 + text + path.len()].copy_from_slice(path.as_bytes());
            if index != 0 {
                let stride = if fields == 0 { 16 } else { 32 };
                let row = 0x3000 + (index - 1) * stride;
                put64(
                    &mut heap,
                    row + if fields == 0 { 0 } else { 8 },
                    HEAP + holder as u64,
                );
                put32(&mut heap, row + stride - 8, 1);
            }
        }
        let used = if fields == 0 { 64 } else { 128 };
        put64(&mut heap, 0x1000 + vector as usize, HEAP + 0x3000);
        put32(&mut heap, 0x1008 + vector as usize, used);
        put32(&mut heap, 0x100c + vector as usize, used);
        Self {
            executable: bytes,
            mapped,
            heap,
            base,
        }
    }

    fn memory(&self) -> ProcessMemory {
        crate::game_observer::memory::from_capture(42, format!(
            "{:x}-{:x} r--p 0 00:00 0 /game/Warframe.x64.exe\n{:x}-{:x} r-xp 1000 00:00 0 /game/Warframe.x64.exe\n{:x}-{:x} r--p 2000 00:00 0 /game/Warframe.x64.exe\n{:x}-{:x} rw-p 3000 00:00 0 /game/Warframe.x64.exe\n{HEAP:x}-{:x} rw-p 0 00:00 0\n",
            self.base, self.base + 0x1000, self.base + 0x1000, self.base + 0x2000,
            self.base + 0x2000, self.base + 0x3000, self.base + 0x3000, self.base + 0x4000, HEAP + self.heap.len() as u64),
            vec![CaptureBlock { address: self.base, length: self.mapped.len(), file_offset: 0 },
                 CaptureBlock { address: HEAP, length: self.heap.len(), file_offset: self.mapped.len() }],
            [self.mapped.as_slice(), self.heap.as_slice()].concat()).unwrap()
    }

    fn report(&self) -> Result<Value, String> {
        inspect(&self.memory(), &self.executable, WEAPON)
    }
}

#[test]
fn equipment_discovers_relocated_slots_offsets_stride_and_holders() {
    for (base, shift, fields) in [(0x140000000, 0, 0), (0x180000000, 0x100, 0x20)] {
        let fixture = Fixture::new(base, shift, fields);
        let report = fixture.report().unwrap();
        assert_eq!(report["weapon"]["path"], PATHS[0]);
        assert_eq!(report["vector"]["count"], 4);
        assert_eq!(report["bindings"]["layout"]["count_slot"], 0x80 + shift / 8);
        assert_eq!(
            report["bindings"]["accessors"]["vector_offset"],
            0x180 + fields
        );
        assert_eq!(
            report["bindings"]["accessors"]["stride"],
            if fields == 0 { 16 } else { 32 }
        );
        assert_eq!(report["raw_rank"]["status"], "unavailable");
        for (index, upgrade) in report["attached_upgrades"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert_eq!(upgrade["path"], PATHS[index + 1]);
            assert!(upgrade.get("rank").is_none());
        }
    }
}

#[test]
fn equipment_rejects_missing_or_ambiguous_anchor_and_disagreeing_loop() {
    let mut fixture = Fixture::new(0x140000000, 0, 0);
    fixture.executable[0x2000] = b'X';
    assert!(
        fixture
            .report()
            .unwrap_err()
            .contains("anchor signature not found")
    );
    fixture.executable[0x2000] = b'A';
    emit(&mut fixture.executable, 0x1100, "48 8d 15 00 00 00 00");
    relative(&mut fixture.executable, 0x1103, 0x2000);
    assert!(
        fixture
            .report()
            .unwrap_err()
            .contains("ambiguous attached upgrades anchor")
    );
    fixture.executable[0x1100] = 0;
    put32(&mut fixture.executable, 0x1020 + 45, 0x88);
    assert!(
        fixture
            .report()
            .unwrap_err()
            .contains("virtual slots disagree")
    );
}

#[test]
fn equipment_rejects_corrupt_headers_before_following_data() {
    for (data, used, capacity) in [
        (0, 16, 16),
        (HEAP, 17, 32),
        (HEAP, 32, 16),
        (HEAP, 16, 17),
        (HEAP, 0, 129 * 16),
        (u64::MAX - 7, 16, 16),
    ] {
        let mut fixture = Fixture::new(0x140000000, 0, 0);
        put64(&mut fixture.heap, 0x1180, data);
        put32(&mut fixture.heap, 0x1188, used);
        put32(&mut fixture.heap, 0x118c, capacity);
        assert!(fixture.report().unwrap_err().contains("vector header"));
    }
    let mut fixture = Fixture::new(0x140000000, 0, 0);
    fixture.heap[0x1180..0x1190].fill(0);
    assert_eq!(fixture.report().unwrap()["vector"]["count"], 0);
}

#[test]
fn equipment_rejects_changed_code_non_code_methods_and_wrong_ancestry() {
    let mut fixture = Fixture::new(0x140000000, 0, 0);
    fixture.mapped[0x1209] = 5;
    assert!(fixture.report().unwrap_err().contains("code differs"));
    fixture.mapped[0x1209] = 4;
    put64(&mut fixture.mapped, 0x2280, fixture.base + 0x3000);
    assert!(fixture.report().unwrap_err().contains("executable code"));
    put64(&mut fixture.mapped, 0x2280, fixture.base + 0x1200);
    put64(&mut fixture.heap, 0x6018, 0);
    assert!(fixture.report().unwrap_err().contains("not a LotusWeapon"));
    put64(&mut fixture.heap, 0x6018, HEAP + 0x6000);
    assert!(fixture.report().unwrap_err().contains("ancestry"));
}

#[test]
fn equipment_rejects_changed_accessor_structure_and_disagreeing_offsets() {
    for (offset, value) in [(0x1250, 0x90), (0x125d, 0x90), (0x1255, 0x88), (0x1249, 5)] {
        let mut fixture = Fixture::new(0x140000000, 0, 0);
        fixture.executable[offset] = value;
        assert!(fixture.report().is_err());
    }
}

#[test]
fn equipment_rereads_header_entries_holders_identity_names_and_code() {
    for offset in [
        0x1000, 0x1008, 0x1010, 0x1180, 0x1188, 0x118c, 0x3000, 0x3008, 0x5020, 0x4008, 0x4010,
        0x6018, 0x7220,
    ] {
        let mut fixture = Fixture::new(0x140000000, 0, 0);
        let image = PeFile::from_bytes(&fixture.executable).unwrap();
        let layout = Layout::discover(image).unwrap();
        let (_, checks) = snapshot(&fixture.memory(), image, fixture.base, layout, WEAPON).unwrap();
        fixture.heap[offset] ^= 8;
        assert!(
            checks
                .verify(&fixture.memory(), fixture.base, layout)
                .is_err(),
            "mutation at {offset:x}"
        );
    }
    for offset in [0x2280, 0x2290, 0x1209, 0x1255] {
        let mut fixture = Fixture::new(0x140000000, 0, 0);
        let image = PeFile::from_bytes(&fixture.executable).unwrap();
        let layout = Layout::discover(image).unwrap();
        let (_, checks) = snapshot(&fixture.memory(), image, fixture.base, layout, WEAPON).unwrap();
        fixture.mapped[offset] ^= 8;
        assert!(
            checks
                .verify(&fixture.memory(), fixture.base, layout)
                .is_err()
        );
    }
}

#[test]
fn equipment_rejects_missing_capture_bytes_and_holder_mismatch() {
    let mut fixture = Fixture::new(0x140000000, 0, 0);
    put64(&mut fixture.heap, 0x4010, HEAP + 0x5040);
    assert!(fixture.report().unwrap_err().contains("backlink"));
    put64(&mut fixture.heap, 0x4010, HEAP + 0x5020);
    fixture.heap.truncate(0x3100);
    assert!(fixture.report().is_err());
}

#[test]
fn equipment_replays_opened_capture_and_checks_executable_identity() {
    use crate::inspect::{equipment, memory::Source};
    use sha2::{Digest, Sha256};

    let fixture = Fixture::new(0x180000000, 0x100, 0x20);
    let directory = std::env::temp_dir().join(format!(
        "wfinspect-equipment-{}-{}",
        std::process::id(),
        crate::inspect::unix_time_ms()
    ));
    std::fs::create_dir(&directory).unwrap();
    let executable = directory.join("Warframe.x64.exe");
    std::fs::write(&executable, &fixture.executable).unwrap();
    let memory = fixture.memory();
    std::fs::write(directory.join("maps.txt"), memory.maps()).unwrap();
    std::fs::write(
        directory.join("memory.bin"),
        [fixture.mapped.as_slice(), fixture.heap.as_slice()].concat(),
    )
    .unwrap();
    let manifest = json!({
        "schema": 1, "captured_at_unix_ms": 123, "duration_ms": 1,
        "bytes": fixture.mapped.len() + fixture.heap.len(), "truncated": false,
        "roots": [WEAPON], "blocks": [
            {"address": fixture.base, "length": fixture.mapped.len(), "file_offset": 0},
            {"address": HEAP, "length": fixture.heap.len(), "file_offset": fixture.mapped.len()}
        ],
        "source": {"kind": "live", "pid": 42, "identity": {"pid": 42, "executable": {
            "path": executable, "size": fixture.executable.len(),
            "sha256": format!("{:x}", Sha256::digest(&fixture.executable))
        }}}
    });
    let report = directory.join("memory.json");
    std::fs::write(&report, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let result = equipment::read(Source::Capture(directory.clone()), WEAPON).unwrap();
    assert_eq!(result["source"]["kind"], "capture");
    assert_eq!(result["equipment"]["vector"]["count"], 4);
    std::fs::write(&executable, b"different executable").unwrap();
    assert!(
        equipment::read(Source::Capture(directory.clone()), WEAPON)
            .unwrap_err()
            .contains("differs from source identity")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
