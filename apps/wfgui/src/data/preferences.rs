use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Values {
    pub ui_scale: u16,
    pub favorites: BTreeSet<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            ui_scale: 100,
            favorites: BTreeSet::new(),
            extra: BTreeMap::new(),
        }
    }
}
