use std::io::Read;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::memory::{OpenedSource, Source};
use crate::game_observer::equipment;

const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

pub fn read(source: Source, weapon: u64) -> Result<Value, String> {
    let source = OpenedSource::open(source)?;
    let identity = source
        .metadata
        .identity
        .as_ref()
        .ok_or("equipment source has no executable identity")?;
    if identity.pid != source.metadata.pid || identity.pid != source.memory.pid() {
        return Err("equipment source process identity disagrees".into());
    }
    let file = std::fs::File::open(&identity.executable.path).map_err(|e| {
        format!(
            "equipment requires its identified executable {}: {e}",
            identity.executable.path.display()
        )
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_EXECUTABLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("could not read equipment executable: {e}"))?;
    if bytes.len() as u64 > MAX_EXECUTABLE_BYTES {
        return Err("equipment executable exceeds 256 MiB limit".into());
    }
    if bytes.len() as u64 != identity.executable.size
        || format!("{:x}", Sha256::digest(&bytes)) != identity.executable.sha256
    {
        return Err("equipment executable differs from source identity".into());
    }
    let equipment = equipment::inspect(&source.memory, &bytes, weapon)?;
    Ok(json!({"source": source.metadata, "equipment": equipment}))
}
