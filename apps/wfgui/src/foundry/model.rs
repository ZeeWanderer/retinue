use retinue_ui::thumbnails::{Key, Request, Source, parse_digest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct Asset {
    pub id: String,
    pub source: String,
    pub image_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Component {
    pub id: String,
    #[serde(rename = "full_name")]
    pub name: String,
    pub required: u64,
    pub owned: u64,
    pub blueprint_owned: u64,
    pub owned_relic: bool,
    pub relic_drop: bool,
    pub tradable: bool,
    pub market_name: Option<String>,
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub group: String,
    pub owned: bool,
    pub mastered: bool,
    pub is_prime: bool,
    pub vaulted: bool,
    pub pending: bool,
    pub subsumed: bool,
    pub ready_to_build: bool,
    pub used_for_crafting: bool,
    pub has_archon_shards: bool,
    pub has_incarnon: bool,
    pub mastery_requirement: u64,
    pub components: Vec<Component>,
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Snapshot {
    pub revision: u64,
    pub items: Vec<Card>,
    #[serde(default)]
    pub profile: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Descriptor {
    pub id: String,
    pub path: PathBuf,
    pub digest: String,
}

impl Descriptor {
    pub fn key(&self, size: u32) -> Option<Key> {
        Some(Key {
            digest: parse_digest(&self.digest)?,
            width: size,
            height: size,
            crop: None,
            tint: None,
        })
    }

    pub fn request(&self, size: u32) -> Option<Request> {
        Some(Request {
            key: self.key(size)?,
            source: Source::File(self.path.clone()),
            origin: None,
        })
    }
}

pub fn fixture() -> Snapshot {
    let items = (0..240)
        .map(|i| {
            let name = [
                "Ash Prime",
                "Ember Prime",
                "Frost",
                "Hildryn",
                "Mesa Prime",
                "Rhino Prime",
                "Saryn Prime",
                "Titania",
            ][i % 8];
            Card {
                id: format!("fixture-{i}"),
                name: if i < 8 {
                    name.into()
                } else {
                    format!("{name} {}", i / 8 + 1)
                },
                group: if i < 120 { "warframe" } else { "primary" }.into(),
                owned: i % 3 == 0,
                mastered: i % 2 == 0,
                is_prime: name.contains("Prime"),
                vaulted: i % 3 != 1,
                pending: i % 13 == 0,
                subsumed: i % 4 == 0,
                ready_to_build: i % 5 == 0,
                components: [
                    "Blueprint",
                    "Neuroptics",
                    "Chassis",
                    "Systems",
                    "Orokin Cell",
                ]
                .iter()
                .enumerate()
                .map(|(n, name)| Component {
                    id: format!("{i}-{n}"),
                    name: name.to_string(),
                    owned: if (i + n) % 3 == 0 { 0 } else { 3 },
                    required: 1,
                    blueprint_owned: u64::from(n == 2),
                    owned_relic: n == 1,
                    ..Default::default()
                })
                .collect(),
                ..Default::default()
            }
        })
        .collect();
    Snapshot {
        revision: 1,
        items,
        profile: serde_json::json!({"player_level":25}),
    }
}
