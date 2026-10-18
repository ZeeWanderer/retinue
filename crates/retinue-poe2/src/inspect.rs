use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use retinue_native::inspect::{
    capture,
    memory::{OpenedSource, Source, SourceMetadata},
    unix_time_ms,
};
use retinue_native::work::{Budget, Limits, Usage};
use serde::{Deserialize, Serialize};

use crate::{
    layout::Layout,
    process::Process,
    reader::{self, Snapshot},
};

#[derive(Serialize)]
pub struct Report {
    pub process: Process,
    pub layout: Layout,
    pub snapshot: Result<Snapshot, String>,
    pub usage: Usage,
    pub capture: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct Manifest {
    schema: u32,
    process: Process,
    layout: Layout,
}

pub fn snapshot(pid: Option<u32>, output: Option<&Path>) -> Result<Report, String> {
    let process = Process::discover(pid)?.ok_or("PoE2 is not running")?;
    let layout = Layout::read(&process.executable)?;
    let memory = process.open()?;
    let memory = if output.is_some() {
        memory.record_reads(1024 * 1024, 16384)
    } else {
        memory
    };
    let budget = budget();
    let memory = memory.with_budget(budget.clone());
    let started = Instant::now();
    let captured_at = unix_time_ms();
    let snapshot = reader::read(&memory, process.image_base, &layout);
    process.validate()?;
    if let Some(directory) = output {
        capture::write_recorded(
            memory,
            SourceMetadata {
                kind: "live".into(),
                pid: process.pid,
                identity: None,
                capture: None,
            },
            directory,
            captured_at,
            started.elapsed().as_millis(),
        )?;
        let manifest = Manifest {
            schema: 1,
            process: process.clone(),
            layout: layout.clone(),
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("poe2.json"))
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    Ok(Report {
        process,
        layout,
        snapshot,
        usage: budget.usage(),
        capture: output.map(Path::to_owned),
    })
}

pub fn replay(directory: &Path) -> Result<Report, String> {
    let path = directory.join("poe2.json");
    if path.metadata().map_err(|e| e.to_string())?.len() > 65536 {
        return Err("PoE2 capture manifest exceeds limit".into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest.schema != 1 {
        return Err("unsupported PoE2 capture".into());
    }
    manifest.layout.validate()?;
    let source = OpenedSource::open(Source::Capture(directory.into()))?;
    if source.metadata.pid != manifest.process.pid {
        return Err("PoE2 capture process identity mismatch".into());
    }
    let budget = budget();
    let memory = source.memory.with_budget(budget.clone());
    let snapshot = reader::read(&memory, manifest.process.image_base, &manifest.layout);
    Ok(Report {
        process: manifest.process,
        layout: manifest.layout,
        snapshot,
        usage: budget.usage(),
        capture: Some(directory.into()),
    })
}

pub fn budget() -> Budget {
    Budget::new(Limits {
        duration: Duration::from_millis(500),
        read_bytes: 1024 * 1024,
        write_bytes: 0,
    })
}

pub struct Observer {
    pid: Option<u32>,
    current: Option<(Process, Result<Layout, String>)>,
}

impl Observer {
    pub fn new(pid: Option<u32>) -> Self {
        Self { pid, current: None }
    }

    pub fn poll(&mut self) -> Result<Option<(Process, Snapshot)>, String> {
        if self
            .current
            .as_ref()
            .is_some_and(|(process, _)| process.validate().is_err())
        {
            self.current = None;
        }
        if self.current.is_none() {
            let Some(process) = Process::discover(self.pid)? else {
                return Ok(None);
            };
            let layout = Layout::read(&process.executable);
            self.current = Some((process, layout));
        }
        let (process, layout) = self.current.as_ref().unwrap();
        let layout = layout.as_ref().map_err(Clone::clone)?;
        let memory = process.open()?.with_budget(budget());
        let snapshot = reader::read(&memory, process.image_base, layout)?;
        process.validate()?;
        Ok(Some((process.clone(), snapshot)))
    }
}
