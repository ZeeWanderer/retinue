pub mod evaluate;
pub mod runtime;
mod transport;

#[cfg(test)]
#[path = "../../../tools/render-bench/support.rs"]
mod render_bench;

pub use transport::{Connection, Publisher};

use retinue_poe2::{process::Process, reader::Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Observed,
    #[default]
    Waiting,
    Error,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Observation {
    pub status: Status,
    pub process: Option<Process>,
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
}

impl Observation {
    pub fn from_result(result: Result<Option<(Process, Snapshot)>, String>) -> Self {
        match result {
            Ok(Some((process, snapshot))) => Self {
                status: Status::Observed,
                process: Some(process),
                snapshot: Some(snapshot),
                error: None,
            },
            Ok(None) => Self::default(),
            Err(error) => Self {
                status: Status::Error,
                error: Some(error),
                ..Self::default()
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sample {
    pub stream: String,
    pub sequence: u64,
    pub observed_at: u64,
    #[serde(flatten)]
    pub observation: Observation,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
