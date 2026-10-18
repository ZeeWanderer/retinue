use serde::{Deserialize, Serialize};

use crate::{daemon, incident, inventory};
use retinue_client::reload::{Checkpoint, Config, Event};

pub(crate) const CONFIG: Config = Config {
    name: "wfcompanion-reload",
    enable_env: "WFCOMPANION_DEV_RELOAD",
    handoff_env: "WFCOMPANION_RELOAD_FD",
    schema: 1,
    ready: "wfcompanion-reload/1\n",
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct State {
    pub inventory: Option<inventory::Checkpoint>,
    pub publications: daemon::Replay,
}

impl Checkpoint for State {
    fn validate(&self) -> Result<(), String> {
        self.publications.validate()
    }
}

pub(crate) type Handoff = retinue_client::reload::Handoff<State>;

pub(super) fn report(event: Event) {
    match event {
        Event::Watching(installed) => {
            incident::info("reload.watching", installed.display().to_string());
        }
        Event::Pending(reason) => {
            incident::info(
                "reload.pending",
                format!("{reason}; waiting for idle services"),
            );
        }
        Event::WatchFailed(error) => incident::warn("reload.watch_failed", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_keeps_wf_publication_validation() {
        assert!(State::default().validate().is_ok());
        let mut saved = serde_json::to_value(State::default()).unwrap();
        saved["publications"] = serde_json::json!([["other", "source", {}]]);
        let state: State = serde_json::from_value(saved).unwrap();
        assert!(
            state
                .validate()
                .unwrap_err()
                .contains("unknown publication source")
        );
    }
}
