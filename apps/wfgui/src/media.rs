use crate::foundry::model::{Asset, Descriptor};
use retinue_ui::{
    painter::RasterImage,
    resources::Resource,
    thumbnails::{Key, Request},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Media {
    descriptors: HashMap<String, Descriptor>,
    previous: HashMap<String, Descriptor>,
    images: HashMap<Key, Resource<RasterImage>>,
    pub errors: HashMap<String, String>,
    decode_errors: std::collections::BTreeMap<String, String>,
}

impl Media {
    pub fn assets(&mut self, descriptors: Vec<Descriptor>) {
        for descriptor in descriptors {
            self.errors.remove(&descriptor.id);
            if let Some(old) = self
                .descriptors
                .insert(descriptor.id.clone(), descriptor.clone())
                && old.digest != descriptor.digest
            {
                self.previous.insert(old.id.clone(), old);
            }
        }
    }
    pub fn request(&self, asset: &Asset, size: u32) -> Option<Request> {
        Some(
            self.descriptors
                .get(&asset.id)?
                .request(size)?
                .persistent(&asset.source, &asset.image_name),
        )
    }
    pub fn image_resource(&self, asset: &Asset, size: u32) -> Option<&Resource<RasterImage>> {
        self.descriptors
            .get(&asset.id)
            .and_then(|d| d.key(size))
            .and_then(|k| self.images.get(&k))
            .or_else(|| {
                self.previous
                    .get(&asset.id)
                    .and_then(|d| d.key(size))
                    .and_then(|k| self.images.get(&k))
            })
    }
    pub fn embedded_resource(&self, request: &Request) -> Option<&Resource<RasterImage>> {
        self.images.get(&request.key)
    }
    pub fn accept(&mut self, images: &[(Key, Resource<RasterImage>)], wanted: &[Request]) {
        let mut keys: HashSet<_> = wanted.iter().map(|r| r.key).collect();
        self.images
            .extend(images.iter().filter(|(key, _)| keys.contains(key)).cloned());
        for descriptor in self.previous.values() {
            if let Some(current) = self.descriptors.get(&descriptor.id) {
                for request in wanted {
                    if current.key(request.key.width) == Some(request.key)
                        && !self.images.contains_key(&request.key)
                        && let Some(key) = descriptor.key(request.key.width)
                    {
                        keys.insert(key);
                    }
                }
            }
        }
        self.images.retain(|key, _| keys.contains(key));
    }
    pub fn clear(&mut self) {
        self.images.clear();
    }
    pub fn image_health(
        &mut self,
        images: &[(Key, Resource<RasterImage>)],
        errors: &[(Key, String)],
    ) -> bool {
        if errors.is_empty() && self.decode_errors.is_empty() {
            return false;
        }
        let mut changed = false;
        for descriptor in self.descriptors.values() {
            let Some(digest) = retinue_ui::thumbnails::parse_digest(&descriptor.digest) else {
                continue;
            };
            if images.iter().any(|(key, _)| key.digest == digest) {
                changed |= self.decode_errors.remove(&descriptor.id).is_some();
            }
            if let Some((_, reason)) = errors.iter().find(|(key, _)| key.digest == digest) {
                let reason: String = reason.chars().take(1024).collect();
                if self.decode_errors.get(&descriptor.id) != Some(&reason) {
                    self.decode_errors.insert(descriptor.id.clone(), reason);
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn issues(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.decode_errors
                .iter()
                .map(|(id, reason)| {
                    serde_json::json!({
                        "kind":"asset_decode", "identity":id, "reason":reason,
                        "fallback":"placeholder", "class":"gui"
                    })
                })
                .collect(),
        )
    }
}

pub fn icon(name: &str) -> &'static [u8] {
    macro_rules! ui {
        ($n:literal) => {
            include_bytes!(concat!("../resources/ui/", $n, ".png"))
        };
    }
    macro_rules! cat {
        ($n:literal) => {
            include_bytes!(concat!("../resources/categories/", $n, ".png"))
        };
    }
    macro_rules! activity {
        ($n:literal) => {
            include_bytes!(concat!("../resources/activity/", $n, ".png"))
        };
    }
    match name {
        "search" => ui!("search"),
        "refresh" => ui!("refresh"),
        "filter" => ui!("filter"),
        "close" => ui!("window-close"),
        "mastery" => ui!("nav_mastery"),
        "market" => ui!("market"),
        "world" => ui!("world"),
        "favorite" => ui!("favorite"),
        "favorite_active" => ui!("favorite_active"),
        "mastered" => include_bytes!("../../../assets/warframe/ui/mastered.png"),
        "owned" => include_bytes!("../../../assets/warframe/ui/owned.png"),
        "vaulted" => include_bytes!("../../wfcompanion/assets/vaulted.png"),
        "unvaulted" => include_bytes!("../../wfcompanion/assets/unvaulted.png"),
        "platinum" => include_bytes!("../../wfcompanion/assets/platinum.png"),
        "ducats" => include_bytes!("../../wfcompanion/assets/ducats.png"),
        "fissure" | "traces" => ui!("fissure"),
        "parts" => cat!("all"),
        "sets" => cat!("sets"),
        "relics" => cat!("relics"),
        "mods" => cat!("mods"),
        "arcanes" => cat!("arcanes"),
        "misc" => cat!("misc"),
        "warframe" => cat!("warframe"),
        "primary" => cat!("primary"),
        "secondary" => cat!("secondary"),
        "melee" => cat!("melee"),
        "arch" => cat!("arch"),
        "companion" => cat!("companion"),
        "easy" => ui!("mastery_easy"),
        "summary_game" => ui!("summary_game"),
        "summary_star" => ui!("summary_star"),
        "summary_intrinsics" => ui!("summary_intrinsics"),
        "notification" => ui!("notification"),
        "normal" => ui!("normal"),
        "steel" => ui!("steel"),
        "all" => ui!("all"),
        "lith" => activity!("lith"),
        "meso" => activity!("meso"),
        "neo" => activity!("neo"),
        "axi" => activity!("axi"),
        "requiem" => activity!("requiem"),
        "omnia" => activity!("omnia"),
        "earth" | "cetus" => activity!("earth"),
        "vallis" => activity!("vallis"),
        "cambion" => activity!("cambion"),
        "resurgence" => activity!("resurgence"),
        _ => ui!("nav_mastery"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decode_health_reports_changes_and_recovery_only() {
        let mut media = Media::default();
        let descriptor = Descriptor {
            id: "wfcd:item".into(),
            path: "unused".into(),
            digest: "ab".repeat(32),
        };
        let key = descriptor.key(64).unwrap();
        media.assets(vec![descriptor]);
        let errors = vec![(key, "Image corrupt".into())];
        assert!(media.image_health(&[], &errors));
        assert!(!media.image_health(&[], &errors));
        assert_eq!(media.issues()[0]["identity"], "wfcd:item");
        let mut cache = retinue_ui::thumbnails::Thumbnails::new(1024 * 1024, 1024 * 1024);
        let request = Request::embedded(icon("mastery"), 8, None);
        let image = cache.prepare(&request).unwrap();
        assert!(media.image_health(&[(key, image)], &[]));
        assert_eq!(media.issues(), serde_json::json!([]));
        assert!(!media.image_health(&[], &[]));
    }
}
