use retinue_render::{
    thumbnails::Key, thumbnails::Request, thumbnails::Source, thumbnails::parse_digest,
};
use serde::Deserialize;
use std::path::PathBuf;

pub use retinue_protocol::warframe::Asset;

pub use retinue_protocol::warframe::Component;

pub use retinue_protocol::warframe::FoundryCard as Card;

pub use retinue_protocol::warframe::FoundrySnapshot as Snapshot;

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
                    full_name: name.to_string(),
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
        profile: Some(retinue_protocol::warframe::Profile {
            player_level: Some(25),
            ..Default::default()
        }),
    }
}
