use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Usage {
    pub cache_root: PathBuf,
    pub bytes: u64,
    pub objects: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub source: Result<Usage, String>,
    pub rendered: Result<Usage, String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            source: Err("Connecting...".into()),
            rendered: Err("Measuring...".into()),
        }
    }
}

impl Snapshot {
    pub fn fixture() -> Self {
        Self {
            source: Ok(Usage {
                cache_root: "/home/player/.cache/retinue/games/warframe/assets".into(),
                bytes: 173_015_040,
                objects: 2401,
            }),
            rendered: Ok(Usage {
                cache_root: "/home/player/.cache/retinue/games/warframe/gui/derivatives".into(),
                bytes: 37_748_736,
                objects: 920,
            }),
        }
    }
}
