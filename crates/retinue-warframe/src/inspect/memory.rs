use serde::{Deserialize, Serialize};

use crate::game_observer::adapter::{self, AdapterSupport};
use crate::game_observer::identify_process;
use crate::game_observer::memory::ProcessMemory;
use crate::game_observer::ui::{
    self, BoundedScanMetrics, PointerReferences, RelicRewardText, RelicSelection, ScanMetrics,
    Snapshot, TextScan,
};
use crate::observation::ui_capture;

use retinue_native::inspect::memory as native;
pub use retinue_native::inspect::memory::{
    ByteScanReport, CaptureMetadata, Source, SourceMetadata,
};
use retinue_native::inspect::query::Selection;
#[derive(Debug, Serialize)]
pub struct UiStateReport {
    pub source: SourceMetadata,
    pub adapter: AdapterSupport,
    pub state: Probe<UiState>,
}

#[derive(Debug, Serialize)]
pub struct UiState {
    pub snapshot: Snapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BoundedScanMetrics>,
}

#[derive(Debug, Serialize)]
pub struct UiMoviesReport {
    pub source: SourceMetadata,
    pub snapshot: Snapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ScanMetrics>,
}

#[derive(Debug, Serialize)]
pub struct UiTextReport {
    pub source: SourceMetadata,
    pub text: TextScan,
}

#[derive(Debug, Serialize)]
pub struct UiPointerReport {
    pub source: SourceMetadata,
    pub pointers: PointerReferences,
}

#[derive(Debug, Serialize)]
pub struct UiRelicReport {
    pub source: SourceMetadata,
    pub adapter: AdapterSupport,
    pub selection: Probe<RelicSelection>,
    pub rewards: Probe<RelicRewardText>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Probe<T> {
    Available { value: T },
    Unavailable { reason: String },
}

pub(super) struct OpenedSource {
    pub memory: ProcessMemory,
    pub metadata: SourceMetadata,
    scaleform: Option<adapter::ScaleformLayout>,
    typed_capture: bool,
}

impl OpenedSource {
    pub(super) fn open(source: Source) -> Result<Self, String> {
        match source {
            Source::Live(pid) => {
                let memory = crate::game_observer::memory::open(pid)?;
                let identity = identify_process(pid)?;
                Ok(Self {
                    memory,
                    scaleform: None,
                    typed_capture: false,
                    metadata: SourceMetadata {
                        kind: "live".into(),
                        pid,
                        identity: Some(identity),
                        capture: None,
                    },
                })
            }
            Source::Capture(path) => {
                let generic = if path.is_dir() {
                    path.join("memory.json")
                } else {
                    path.clone()
                };
                if generic
                    .file_name()
                    .is_some_and(|name| name == "memory.json")
                    && generic.is_file()
                {
                    let source = native::OpenedSource::open(Source::Capture(generic))?;
                    return Ok(Self {
                        memory: crate::game_observer::memory::with_game_image(source.memory),
                        metadata: source.metadata,
                        scaleform: None,
                        typed_capture: false,
                    });
                }
                let evidence = ui_capture::load_evidence(&path)?;
                let metadata = SourceMetadata {
                    kind: "capture".into(),
                    pid: evidence.snapshot.pid,
                    identity: evidence.identity,
                    capture: Some(CaptureMetadata {
                        report: evidence.report,
                        schema: evidence.schema,
                        captured_at_unix_ms: evidence.captured_at_unix_ms,
                        memory: evidence.memory_summary,
                    }),
                };
                Ok(Self {
                    memory: evidence.memory,
                    metadata,
                    scaleform: evidence.scaleform,
                    typed_capture: evidence.typed_reads,
                })
            }
        }
    }

    fn layout(&self) -> Result<adapter::ScaleformLayout, String> {
        if self.metadata.kind == "capture" {
            return adapter::replay_scaleform(self.metadata.identity.as_ref(), self.scaleform);
        }
        let identity = self
            .metadata
            .identity
            .as_ref()
            .ok_or("capture has no executable identity")?;
        adapter::require_scaleform(identity)
    }

    fn typed_snapshot(
        &self,
        layout: adapter::ScaleformLayout,
    ) -> Result<(Snapshot, Option<BoundedScanMetrics>), String> {
        if self.metadata.kind == "capture" && !self.typed_capture {
            return ui::scan_memory(&self.memory).map(|scan| (scan.snapshot, None));
        }
        ui::scan_registry_memory(&self.memory, layout)
            .map(|scan| (scan.snapshot, Some(scan.metrics)))
            .map_err(|(stage, reason)| format!("{stage}: {reason}"))
    }
}

pub fn open_memory(source: Source) -> Result<native::OpenedSource, String> {
    let source = OpenedSource::open(source)?;
    Ok(native::OpenedSource {
        memory: source.memory,
        metadata: source.metadata,
    })
}

pub(super) fn find_bytes(
    source: &OpenedSource,
    pattern: &[u8],
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    native::find_bytes(
        &source.memory,
        &source.metadata,
        pattern,
        selection,
        limit,
        max_bytes,
    )
}

impl<T> Probe<T> {
    fn from_result(result: Result<T, String>) -> Self {
        match result {
            Ok(value) => Self::Available { value },
            Err(reason) => Self::Unavailable { reason },
        }
    }
}

pub fn ui_state(source: Source) -> Result<UiStateReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout();
    let adapter = adapter::layout_support(&layout);
    let state = layout
        .and_then(|layout| source.typed_snapshot(layout))
        .map(|(snapshot, metrics)| UiState { snapshot, metrics });
    Ok(UiStateReport {
        source: source.metadata,
        adapter,
        state: Probe::from_result(state),
    })
}

pub fn ui_movies(source: Source) -> Result<UiMoviesReport, String> {
    let source = OpenedSource::open(source)?;
    let scan = ui::scan_memory(&source.memory)?;
    let (snapshot, metrics) = (scan.snapshot, Some(scan.metrics));
    Ok(UiMoviesReport {
        source: source.metadata,
        snapshot,
        metrics,
    })
}

pub fn ui_find(source: Source, terms: &[String]) -> Result<UiTextReport, String> {
    let source = OpenedSource::open(source)?;
    let text = ui::scan_text_memory(&source.memory, terms)?;
    Ok(UiTextReport {
        source: source.metadata,
        text,
    })
}

#[derive(Serialize)]
pub struct UiObjectsReport {
    pub source: SourceMetadata,
    pub movies: Vec<MovieObjects>,
}

#[derive(Serialize)]
pub struct MovieObjects {
    pub movie: ui::Movie,
    pub objects: Probe<ui::ObjectReport>,
}

pub fn ui_objects(
    source: Source,
    movie: &str,
    max_text_bytes: usize,
    limit: usize,
) -> Result<UiObjectsReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout()?;
    let (snapshot, _) = source.typed_snapshot(layout)?;
    let mut movies = Vec::new();
    for found in snapshot
        .movies
        .into_iter()
        .filter(|found| found.path.contains(movie))
    {
        let objects = found
            .record_address
            .checked_sub(0xc0)
            .ok_or("invalid movie address")
            .map_err(str::to_owned)
            .and_then(|flash| {
                ui::enumerate_text_objects(&source.memory, layout, flash, max_text_bytes, limit)
            });
        movies.push(MovieObjects {
            movie: found,
            objects: Probe::from_result(objects),
        });
    }
    if movies.is_empty() {
        return Err(format!("no movie matched {movie:?} in available memory"));
    }
    Ok(UiObjectsReport {
        source: source.metadata,
        movies,
    })
}

#[derive(Serialize)]
pub struct UiTreeReport {
    pub source: SourceMetadata,
    pub movies: Vec<MovieTree>,
}

#[derive(Serialize)]
pub struct MovieTree {
    pub movie: ui::Movie,
    #[serde(flatten)]
    pub tree: Probe<ui::DisplayTree>,
}

#[derive(Deserialize, Serialize)]
struct DisplayBindings {
    schema: u8,
    executable_sha256: String,
    acquisition_consistent: bool,
    display: adapter::ScaleformLayout,
    geometry: ui::geometry::layout::Layout,
}

impl DisplayBindings {
    fn validate(&self, hash: &str) -> Result<(), String> {
        if self.schema != 1 || self.executable_sha256 != hash {
            return Err("display bindings do not match capture identity".into());
        }
        self.display.validate()?;
        self.geometry.validate()
    }
}

pub fn ui_tree(
    source: Source,
    movie: &str,
    limit: usize,
    save: Option<&std::path::Path>,
) -> Result<UiTreeReport, String> {
    if movie.is_empty() || !(1..=10000).contains(&limit) {
        return Err("supply a movie substring and node limit 1..10000".into());
    }
    let (mut source, mut bindings) = open_display(source)?;
    if save.is_some() {
        source.memory = source.memory.record_reads(32 * 1024 * 1024, 65536);
    }
    let captured_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = std::time::Instant::now();
    let snapshot = ui::scan_registry_memory(&source.memory, bindings.display)
        .map_err(|(stage, reason)| format!("{stage}: {reason}"))?
        .snapshot;
    let mut movies = Vec::new();
    for found in snapshot
        .movies
        .iter()
        .filter(|found| found.path.contains(movie))
    {
        movies.push(MovieTree {
            movie: found.clone(),
            tree: Probe::from_result(ui::geometry::read_tree(
                &source.memory,
                bindings.display,
                &bindings.geometry,
                found,
                limit,
            )),
        });
    }
    if movies.is_empty() {
        return Err(format!("no movie matched {movie:?}"));
    }
    for tree in &mut movies {
        if let Probe::Available { value } = &mut tree.tree {
            value.consistent &= bindings.acquisition_consistent;
        }
    }
    bindings.acquisition_consistent = movies
        .iter()
        .all(|tree| matches!(&tree.tree, Probe::Available { value } if value.consistent));
    let metadata = source.metadata.clone();
    if let Some(directory) = save {
        save_display(
            source,
            &bindings,
            directory,
            captured_at,
            started.elapsed().as_millis(),
        )?;
    }
    Ok(UiTreeReport {
        source: metadata,
        movies,
    })
}

#[derive(Serialize)]
pub struct UiDucatReport {
    pub source: SourceMetadata,
    pub state: Probe<ui::DucatState>,
}

pub fn ui_ducats(source: Source, save: Option<&std::path::Path>) -> Result<UiDucatReport, String> {
    let (mut source, mut bindings) = open_display(source)?;
    if save.is_some() {
        source.memory = source.memory.record_reads(32 * 1024 * 1024, 65536);
    }
    let captured_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = std::time::Instant::now();
    let result = if bindings.acquisition_consistent {
        ui::ducats::Cache::default().read(&source.memory, bindings.display, &bindings.geometry)
    } else {
        Err("display capture was inconsistent at acquisition".into())
    };
    bindings.acquisition_consistent &= result.is_ok();
    let metadata = source.metadata.clone();
    if let Some(directory) = save {
        save_display(
            source,
            &bindings,
            directory,
            captured_at,
            started.elapsed().as_millis(),
        )?;
    }
    Ok(UiDucatReport {
        source: metadata,
        state: Probe::from_result(result),
    })
}

fn open_display(source: Source) -> Result<(OpenedSource, DisplayBindings), String> {
    let source = OpenedSource::open(source)?;
    let identity = source
        .metadata
        .identity
        .as_ref()
        .ok_or("display geometry needs an executable identity")?;
    let recorded = source
        .metadata
        .capture
        .as_ref()
        .and_then(|capture| capture.report.parent())
        .map(|directory| directory.join("display-bindings.json"))
        .filter(|path| path.is_file());
    let bindings = if let Some(path) = recorded {
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 {
            return Err("display bindings file too large".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        if bytes.len() > 64 * 1024 {
            return Err("display bindings file too large".into());
        }
        let bindings: DisplayBindings =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        bindings.validate(&identity.executable.sha256)?;
        bindings
    } else {
        let (display, geometry) = ui::geometry::bindings(identity)?;
        DisplayBindings {
            schema: 1,
            executable_sha256: identity.executable.sha256.clone(),
            acquisition_consistent: true,
            display,
            geometry,
        }
    };
    Ok((source, bindings))
}

fn save_display(
    source: OpenedSource,
    bindings: &DisplayBindings,
    directory: &std::path::Path,
    captured_at: u128,
    elapsed: u128,
) -> Result<(), String> {
    retinue_native::inspect::capture::write_recorded(
        source.memory,
        source.metadata,
        directory,
        captured_at,
        elapsed,
    )?;
    std::fs::write(
        directory.join("display-bindings.json"),
        serde_json::to_vec_pretty(bindings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn ui_refs(
    source: Source,
    target_start: u64,
    target_end: u64,
) -> Result<UiPointerReport, String> {
    let source = OpenedSource::open(source)?;
    let pointers = ui::scan_pointers_memory(&source.memory, target_start, target_end)?;
    Ok(UiPointerReport {
        source: source.metadata,
        pointers,
    })
}

pub fn ui_relic(source: Source) -> Result<UiRelicReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout();
    let adapter = adapter::layout_support(&layout);
    let snapshot = layout.and_then(|layout| {
        source
            .typed_snapshot(layout)
            .map(|(snapshot, _metrics)| (layout, snapshot))
    });
    let (selection, rewards) = match snapshot {
        Ok((layout, snapshot)) => (
            Probe::from_result(ui::relic_selection_memory(
                &source.memory,
                layout,
                &snapshot,
            )),
            Probe::from_result(ui::relic_rewards_memory(&source.memory, layout, &snapshot)),
        ),
        Err(reason) => (
            Probe::Unavailable {
                reason: reason.clone(),
            },
            Probe::Unavailable { reason },
        ),
    };
    Ok(UiRelicReport {
        source: source.metadata,
        adapter,
        selection,
        rewards,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_native::inspect::query::WalkOptions;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn display_capture_bindings_reject_wrong_identity_and_unsafe_fields() {
        let display = adapter::test_scaleform();
        let mut bindings = DisplayBindings {
            schema: 1,
            executable_sha256: "test".into(),
            acquisition_consistent: true,
            display,
            geometry: ui::geometry::layout::Layout {
                matrix: 0x78,
                parent_control: 0xb8,
                parent: 0xc0,
                name: 0xc8,
                alpha: 0xf0,
                flags: 0x127,
                leaf_definitions: [(display.text_vtable_rva, 0x130)].into_iter().collect(),
                definition_rects: [(0x8000, 0x50)].into_iter().collect(),
            },
        };
        assert!(bindings.validate("test").is_ok());
        assert!(bindings.validate("other").is_err());
        bindings.geometry.matrix = usize::MAX;
        assert!(bindings.validate("test").is_err());
        bindings.geometry.matrix = 0x78;
        bindings
            .geometry
            .leaf_definitions
            .insert(0x1000, usize::MAX);
        assert!(bindings.validate("test").is_err());
    }

    fn capture() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "wfinspect-memory-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let base = 0x200000000_u64;
        let mut bytes = vec![0_u8; 256];
        bytes[0x40..0x47].copy_from_slice(b"NEO ERA");
        bytes[0x10..0x18].copy_from_slice(&(base + 0x40).to_le_bytes());
        std::fs::write(directory.join("scaleform-memory.bin"), &bytes).unwrap();
        std::fs::write(
            directory.join("maps.txt"),
            "140000000-141000000 r-xp 0 00:00 1 /game/Warframe.x64.exe\n\
             200000000-200001000 rw-p 0 00:00 0 [heap]\n",
        )
        .unwrap();
        let report = serde_json::json!({
            "schema": 2,
            "captured_at_unix_ms": 123,
            "snapshot": {"pid": 42, "movies": []},
            "memory": {
                "file": "scaleform-memory.bin",
                "block_size": 512,
                "max_depth": 4,
                "max_blocks": 16,
                "max_blocks_per_root": 4,
                "max_children_per_block": 4,
                "bytes": bytes.len(),
                "truncated": false,
                "roots": [],
                "blocks": [{
                    "address": base,
                    "length": bytes.len(),
                    "file_offset": 0,
                    "depth": 0,
                    "permissions": "rw-p",
                    "region": "[heap]"
                }]
            }
        });
        std::fs::write(
            directory.join("ui.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn capture_supports_same_research_queries_as_live_memory() {
        let directory = capture();
        let source = || Source::Capture(directory.clone());
        let base = 0x200000000_u64;

        assert_eq!(
            native::read(open_memory(source()).unwrap(), base + 0x40, 7).unwrap(),
            b"NEO ERA"
        );
        assert_eq!(
            native::scan(open_memory(source()).unwrap(), b"NEO ERA")
                .unwrap()
                .matches
                .len(),
            1
        );
        let text = ui_find(source(), &["NEO ERA".to_owned()]).unwrap();
        assert_eq!(text.text.terms[0].matches[0].address, base + 0x40);
        let refs = ui_refs(source(), base + 0x40, base + 0x47).unwrap();
        assert_eq!(refs.pointers.references[0].address, base + 0x10);
        let path = native::path(
            open_memory(source()).unwrap(),
            base,
            Some(base + 0x40..base + 0x47),
            &WalkOptions::default(),
        )
        .unwrap();
        assert_eq!(path.path.hops.len(), 1);
        assert_eq!(ui_movies(source()).unwrap().snapshot.movies.len(), 0);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
