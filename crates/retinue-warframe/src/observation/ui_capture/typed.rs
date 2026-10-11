use super::*;

const MAX_TEXT_BYTES: usize = 4096;
const MAX_TEXT_OBJECTS: usize = 10_000;

#[derive(Debug, Serialize)]
pub(super) struct MovieCapture {
    path: String,
    record_address: u64,
    #[serde(flatten)]
    result: MovieResult,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum MovieResult {
    Retained {
        complete: bool,
        text_objects: usize,
        incomplete_texts: usize,
        truncated: bool,
    },
    Unavailable {
        reason: String,
    },
}

pub(super) struct TypedCapture {
    pub(super) snapshot: Snapshot,
    pub(super) registry: ui::BoundedScanMetrics,
    pub(super) movies: Vec<MovieCapture>,
    pub(super) manifest: MemoryManifest,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn collect(
    memory: ProcessMemory,
    layout: ScaleformLayout,
) -> Result<TypedCapture, String> {
    let memory = memory.record_reads(MAX_CAPTURE_BYTES, MAX_BLOCKS);
    let scan = ui::scan_registry_memory(&memory, layout)
        .map_err(|(stage, reason)| format!("{stage}: {reason}"))?;
    let mut movies = scan.snapshot.movies.iter().collect::<Vec<_>>();
    // Capture short-lived contextual movies before spending the budget on other UI.
    movies.sort_by_key(|movie| match movie.path.as_str() {
        "/Lotus/Interface/ProjectionRewardChoice.swf" => 0,
        "/Lotus/Interface/ThemedProjectionManager.swf" => 1,
        _ => 2,
    });
    let mut reports = Vec::with_capacity(movies.len());
    for movie in movies {
        memory.check_budget().map_err(|error| error.to_string())?;
        let result = movie
            .record_address
            .checked_sub(FLASH_RECORD_OFFSET)
            .ok_or_else(|| "invalid movie record".to_owned())
            .and_then(|flash| {
                ui::enumerate_text_objects(&memory, layout, flash, MAX_TEXT_BYTES, MAX_TEXT_OBJECTS)
            });
        let result = match result {
            Ok(report) => MovieResult::Retained {
                complete: !report.truncated
                    && report.objects.iter().all(|object| object.terminated),
                text_objects: report.objects.len(),
                incomplete_texts: report
                    .objects
                    .iter()
                    .filter(|object| !object.terminated)
                    .count(),
                truncated: report.truncated,
            },
            Err(reason) => MovieResult::Unavailable { reason },
        };
        reports.push(MovieCapture {
            path: movie.path.clone(),
            record_address: movie.record_address,
            result,
        });
    }
    memory.check_budget().map_err(|error| error.to_string())?;
    let regions = memory.regions().to_vec();
    let recorded = memory.into_recorded_reads()?;
    let blocks = manifest_blocks(recorded.blocks, &regions)?;
    let manifest = MemoryManifest {
        file: MEMORY_FILE.into(),
        typed_reads: true,
        block_size: 0,
        max_depth: 0,
        max_blocks: MAX_BLOCKS,
        max_blocks_per_root: 0,
        max_children_per_block: 0,
        bytes: recorded.bytes.len(),
        truncated: recorded.truncated,
        roots: scan
            .snapshot
            .movies
            .iter()
            .map(|movie| MovieRoot {
                path: movie.path.clone(),
                record_address: movie.record_address,
                path_address: movie.path_address,
            })
            .collect(),
        blocks,
    };
    Ok(TypedCapture {
        snapshot: scan.snapshot,
        registry: scan.metrics,
        movies: reports,
        manifest,
        bytes: recorded.bytes,
    })
}

fn manifest_blocks(
    blocks: Vec<CaptureBlock>,
    regions: &[Region],
) -> Result<Vec<MemoryBlock>, String> {
    let mut result = Vec::with_capacity(blocks.len());
    for block in blocks {
        let end = block.address + block.length as u64;
        let mut address = block.address;
        while address < end {
            let index = regions.partition_point(|region| region.end <= address);
            let region = regions
                .get(index)
                .filter(|region| region.contains(address))
                .ok_or("recorded read is outside process mappings")?;
            if result.len() == MAX_BLOCKS {
                return Err("recorded mapping fragments exceed capture limit".into());
            }
            let length = (region.end.min(end) - address) as usize;
            result.push(MemoryBlock {
                address,
                length,
                file_offset: block.file_offset + (address - block.address) as usize,
                depth: 0,
                permissions: region.permissions.clone(),
                region: region.path.clone(),
            });
            address += length as u64;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: u64 = 0x2_0000_0000;

    #[test]
    fn reads_crossing_mapping_boundaries_keep_both_fragments() {
        let regions = [
            Region {
                start: 0x1000,
                end: 0x2000,
                permissions: "r--p".into(),
                path: "image".into(),
            },
            Region {
                start: 0x2000,
                end: 0x3000,
                permissions: "rw-p".into(),
                path: "[heap]".into(),
            },
        ];
        let blocks = manifest_blocks(
            vec![CaptureBlock {
                address: 0x1ff0,
                length: 32,
                file_offset: 40,
            }],
            &regions,
        )
        .unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            (blocks[0].address, blocks[0].length, blocks[0].file_offset),
            (0x1ff0, 16, 40)
        );
        assert_eq!(
            (blocks[1].address, blocks[1].length, blocks[1].file_offset),
            (0x2000, 16, 56)
        );
        assert_eq!(blocks[1].permissions, "rw-p");
    }

    fn fixture(layout: ScaleformLayout) -> ProcessMemory {
        let mut bytes = vec![0; 0x10_000];
        let pointer = |bytes: &mut [u8], offset: usize, value: u64| {
            bytes[offset..offset + 8].copy_from_slice(&(IMAGE + value).to_le_bytes());
        };
        let integer = |bytes: &mut [u8], offset: usize, value: u32| {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        };
        pointer(&mut bytes, 0x1800, 0x1000);
        integer(&mut bytes, 0x1808, 32);
        integer(&mut bytes, 0x180c, 32);
        for (i, path) in [
            "/Lotus/Interface/ProjectionRewardChoice.swf",
            "/Lotus/Interface/Broken.swf",
        ]
        .into_iter()
        .enumerate()
        {
            let flash = 0x2000 + i * 0x200;
            let record = flash + 0xc0;
            let path_offset = 0x4000 + i * 0x200;
            pointer(&mut bytes, 0x1000 + i * 16, (0x1100 + i * 16) as u64);
            pointer(&mut bytes, 0x1008 + i * 16, layout.flash_instance_type_rva);
            pointer(&mut bytes, 0x1100 + i * 16, flash as u64);
            pointer(&mut bytes, flash, layout.flash_instance_vtable_rva);
            pointer(&mut bytes, flash + 8, layout.flash_instance_type_rva);
            for (offset, value) in [
                (8, 2560),
                (12, 1440),
                (0x20, 2560),
                (0x24, 1440),
                (0x18, 1_f32.to_bits()),
                (0x1c, 1_f32.to_bits()),
            ] {
                integer(&mut bytes, record + offset, value);
            }
            pointer(&mut bytes, record + 0x60, path_offset as u64);
            bytes[path_offset..path_offset + path.len()].copy_from_slice(path.as_bytes());
        }
        pointer(&mut bytes, 0x2088, 0x3000);
        pointer(&mut bytes, 0x3000, layout.root_vtable_rva);
        pointer(&mut bytes, 0x3010, layout.root_secondary_vtable_rva);
        pointer(&mut bytes, 0x3028, 0x3800);
        integer(&mut bytes, 0x3030, 8);
        integer(&mut bytes, 0x3034, 8);
        pointer(&mut bytes, 0x30a0, 0x2000);
        pointer(&mut bytes, 0x3800, 0x5000);
        pointer(&mut bytes, 0x5000, layout.container_vtable_rva);
        pointer(&mut bytes, 0x5010, layout.container_secondary_vtable_rva);
        pointer(&mut bytes, 0x5130, 0x5800);
        integer(&mut bytes, 0x5138, 16);
        integer(&mut bytes, 0x513c, 16);
        for (i, name) in ["Forma Blueprint", "Forma Blueprint x2"]
            .into_iter()
            .enumerate()
        {
            let object = 0x61c0 + i * 0x200;
            let text = 0x7ff8 + i * 0x100;
            pointer(&mut bytes, 0x5800 + i * 8, object as u64);
            pointer(&mut bytes, object, layout.text_vtable_rva);
            pointer(&mut bytes, object + 0x10, layout.text_secondary_vtable_rva);
            bytes[object + 0xc8..object + 0xd0].copy_from_slice(b"ItemName");
            for offset in [0x100, 0x104, 0x140, 0x144] {
                integer(&mut bytes, object + offset, 1);
            }
            pointer(&mut bytes, object + 0x190, text as u64);
            bytes[text..text + name.len()].copy_from_slice(name.as_bytes());
        }
        crate::game_observer::memory::from_capture(42,
            format!("{IMAGE:x}-{:x} r--p 0 00:00 0 /game/Warframe.x64.exe\n{:x}-{:x} rw-p 0 00:00 0 [heap]\n", IMAGE + 0x1000, IMAGE + 0x1000, IMAGE + bytes.len() as u64),
            vec![CaptureBlock { address: IMAGE, length: bytes.len(), file_offset: 0 }], bytes).unwrap()
    }

    #[test]
    fn saved_typed_capture_replays_names_without_unrelated_movie_root() {
        let mut layout = adapter::test_scaleform();
        layout.registry_vector_rva = 0x1800;
        let source = fixture(layout);
        let maps = source.maps().to_owned();
        let captured = collect(source, layout).unwrap();
        assert_eq!(captured.movies.len(), 2);
        assert!(matches!(
            captured.movies[0].result,
            MovieResult::Retained {
                complete: true,
                text_objects: 2,
                ..
            }
        ));
        assert!(matches!(
            captured.movies[1].result,
            MovieResult::Unavailable { .. }
        ));
        assert!(!captured.manifest.truncated);
        assert!(captured.bytes.len() < 10_000);
        let expected = captured.snapshot.clone();
        let frozen = Frozen {
            report: Report {
                schema: 5,
                captured_at_unix_ms: 123,
                duration_ms: 0,
                identity: ProcessIdentity {
                    pid: 42,
                    executable: crate::game_observer::ExecutableIdentity {
                        path: "/game/Warframe.x64.exe".into(),
                        size: 1,
                        modified_unix_ms: None,
                        sha256: "test-typed".into(),
                    },
                },
                scaleform: Some(layout),
                scaleform_error: None,
                snapshot: captured.snapshot,
                scan: CaptureScan::TypedReads {
                    registry: captured.registry,
                    movies: captured.movies,
                },
                text: None,
                memory: captured.manifest,
            },
            maps,
            bytes: captured.bytes,
        };
        let path = std::env::temp_dir().join(format!("wf-typed-ui-{}", std::process::id()));
        frozen.save(&Directory::create(&path).unwrap()).unwrap();
        let replay = replay_evidence(&path).unwrap();
        assert_eq!(replay.snapshot, expected);
        let ReplayProbe::Available { value: rewards } = replay.rewards else {
            panic!("reward replay failed");
        };
        assert_eq!(rewards.names, ["Forma Blueprint", "Forma Blueprint x2"]);
        fs::remove_dir_all(path).unwrap();
    }
}
