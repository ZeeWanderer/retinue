use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::relic::{AssetRefresh, Scene};
use retinue_ui::painter::{RasterImage, load_scene_icon};
use retinue_ui::preparation::Preparation;
use retinue_ui::resources::{Resource, ResourceCache};

const DECODED_BUDGET: usize = 64 * 1024 * 1024;
const IMAGE_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct PendingRefreshes {
    entries: VecDeque<AssetRefresh>,
    bytes: usize,
}

impl PendingRefreshes {
    pub(super) fn insert(&mut self, refresh: AssetRefresh) {
        const BUDGET: usize = 128 * 1024;
        let bytes = refresh.bytes();
        if bytes > BUDGET {
            crate::incident::warn(
                "overlay.asset_refresh_rejected",
                "asset refresh exceeds budget",
            );
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|old| old.source == refresh.source && old.image_name == refresh.image_name)
        {
            self.bytes -= self.entries.remove(index).unwrap().bytes();
        }
        while self.entries.len() >= 128 || self.bytes > BUDGET - bytes {
            self.bytes -= self.entries.pop_front().unwrap().bytes();
        }
        self.entries.push_back(refresh);
        self.bytes += bytes;
    }

    pub(super) fn apply(&mut self, scene: &mut Scene) {
        self.entries.retain(|refresh| {
            if scene.apply_asset_refresh(refresh).is_some() {
                self.bytes -= refresh.bytes();
                false
            } else {
                true
            }
        });
    }
}

#[derive(Clone, Default)]
pub(super) struct SceneAssets {
    pub(super) images: BTreeMap<String, Resource<RasterImage>>,
    pub(super) issues: Vec<Value>,
}

pub(super) struct Preparer {
    cache: ResourceCache<[u8; 32], RasterImage>,
}

impl Default for Preparer {
    fn default() -> Self {
        Self {
            cache: ResourceCache::new(DECODED_BUDGET),
        }
    }
}

impl Preparer {
    pub(super) fn prepare(&mut self, scene: &Scene, current: impl Fn() -> bool) -> SceneAssets {
        self.prepare_with_budget(scene, DECODED_BUDGET, current)
    }

    fn prepare_with_budget(
        &mut self,
        scene: &Scene,
        budget: usize,
        current: impl Fn() -> bool,
    ) -> SceneAssets {
        let mut assets = SceneAssets::default();
        let Scene::Rewards(rewards) = scene else {
            self.cache.clear();
            return assets;
        };
        let requested = rewards
            .items
            .iter()
            .flat_map(|reward| {
                reward
                    .asset
                    .iter()
                    .chain(reward.parts.iter().filter_map(|part| part.asset.as_ref()))
            })
            .map(|asset| (&asset.digest, (source_key(&asset.digest), asset)))
            .collect::<BTreeMap<_, _>>();
        let sources: BTreeSet<_> = requested.values().map(|(key, _)| *key).collect();
        self.cache.retain(|key| sources.contains(key));
        let mut used = 0;
        let mut retained = BTreeSet::new();
        for (digest, (key, asset)) in requested {
            if !current() {
                break;
            }
            if digest == crate::assets::FORMA_ASSET.image.key {
                continue;
            }
            let remaining = budget.saturating_sub(used);
            let image = if let Some(image) = self.cache.get(&key) {
                if image.byte_len() <= remaining.min(IMAGE_BUDGET) {
                    Ok(image)
                } else {
                    Err("image exceeds decoded asset budget".to_owned())
                }
            } else {
                load_scene_icon(&asset.path, remaining.min(IMAGE_BUDGET))
                    .map(|image| self.cache.insert(key, image))
                    .map_err(|error| error.to_string())
            };
            match image {
                Ok(image) => {
                    used += image.byte_len();
                    retained.insert(key);
                    assets.images.insert(digest.clone(), image);
                }
                Err(error) => assets.issues.push(json!({
                    "kind": "asset_decode", "identity": asset.id,
                    "reason": error, "fallback": asset.image_name, "class": "companion"
                })),
            }
        }
        if current() {
            self.cache.retain(|key| retained.contains(key));
        }
        assets
    }
}

fn source_key(digest: &str) -> [u8; 32] {
    Sha256::digest(digest.as_bytes()).into()
}

pub(super) struct Loader {
    worker: Option<Preparation<Scene, SceneAssets>>,
}

impl Loader {
    pub(super) fn spawn(notify: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let mut preparer = Preparer::default();
        Ok(Self {
            worker: Some(Preparation::spawn(
                "wf-overlay-assets",
                move |scene, token| preparer.prepare(&scene, || token.is_current()),
                notify,
            )?),
        })
    }

    pub(super) fn request(&mut self, scene: Scene) {
        if let Some(worker) = &mut self.worker
            && let Err(error) = worker.request(scene)
        {
            self.failed(error);
        }
    }

    pub(super) fn poll(&mut self) -> Option<SceneAssets> {
        match self.worker.as_mut()?.poll() {
            Ok(ready) => ready,
            Err(error) => {
                self.failed(error);
                None
            }
        }
    }

    fn failed(&mut self, error: impl std::fmt::Display) {
        crate::incident::warn("overlay.asset_worker_failed", error.to_string());
        self.worker = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_asset_refreshes_are_bounded_and_replace_by_identity() {
        let mut pending = PendingRefreshes::default();
        for index in 0..1000 {
            pending.insert(AssetRefresh {
                source: "market".into(),
                image_name: index.to_string(),
                path: String::new(),
                digest: "old".into(),
            });
        }
        assert_eq!(pending.entries.len(), 128);
        assert_eq!(pending.entries.front().unwrap().image_name, "872");
        pending.insert(AssetRefresh {
            source: "market".into(),
            image_name: "999".into(),
            path: String::new(),
            digest: "new".into(),
        });
        assert_eq!(pending.entries.len(), 128);
        assert_eq!(pending.entries.back().unwrap().digest, "new");
        pending.insert(AssetRefresh {
            source: "market".into(),
            image_name: "999".into(),
            path: "x".repeat(128 * 1024),
            digest: "too large".into(),
        });
        assert_eq!(pending.entries.back().unwrap().digest, "new");
        assert_eq!(
            pending.bytes,
            pending
                .entries
                .iter()
                .map(AssetRefresh::bytes)
                .sum::<usize>()
        );
        for index in 1000..1100 {
            pending.insert(AssetRefresh {
                source: "market".into(),
                image_name: index.to_string(),
                path: "x".repeat(8192),
                digest: "new".into(),
            });
        }
        assert!(pending.bytes <= 128 * 1024);
        assert!(pending.entries.len() < 128);
    }

    #[test]
    fn pending_refresh_is_applied_when_its_scene_arrives() {
        let mut scene = super::super::screens::mock_relic_scene();
        let Scene::Rewards(rewards) = &scene else {
            panic!()
        };
        let asset = rewards
            .items
            .iter()
            .flat_map(|item| &item.parts)
            .find_map(|part| part.asset.as_ref())
            .unwrap()
            .clone();
        let mut pending = PendingRefreshes::default();
        pending.insert(AssetRefresh {
            source: asset.source.clone(),
            image_name: asset.image_name.clone(),
            path: "/updated.png".into(),
            digest: "updated".into(),
        });
        pending.apply(&mut Scene::Reading);
        assert_eq!(pending.entries.len(), 1);
        pending.apply(&mut scene);
        assert!(pending.entries.is_empty());
        assert_eq!(pending.bytes, 0);
        let Scene::Rewards(rewards) = scene else {
            panic!()
        };
        assert!(
            rewards
                .items
                .iter()
                .flat_map(|item| &item.parts)
                .filter_map(|part| part.asset.as_ref())
                .any(|item| item.id == asset.id && item.digest == "updated")
        );
    }

    #[test]
    fn scene_cache_has_a_byte_budget_and_reuses_images() {
        let scene = super::super::screens::mock_relic_scene();
        let mut preparer = Preparer::default();
        let mut assets = preparer.prepare(&scene, || true);
        assert!(!assets.images.is_empty());
        let previous = assets.images.clone();
        assets = preparer.prepare(&scene, || true);
        assert!(
            assets
                .images
                .iter()
                .all(|(id, image)| std::ptr::eq(&**image, &*previous[id]))
        );
        assert!(preparer.cache.stats().hits > 0);
        assets = preparer.prepare_with_budget(&scene, 1024, || true);
        assert!(
            assets
                .images
                .values()
                .map(|image| image.byte_len())
                .sum::<usize>()
                <= 1024
        );
        assert!(!assets.issues.is_empty());
        assets = Preparer::default().prepare_with_budget(&scene, 1, || true);
        assert!(assets.images.is_empty());
        assert!(!assets.issues.is_empty());
    }

    #[test]
    fn changed_sources_are_decoded_again_and_old_handles_remain_valid() {
        let mut scene = super::super::screens::mock_relic_scene();
        let Scene::Rewards(rewards) = &mut scene else {
            unreachable!()
        };
        rewards.items.truncate(1);
        let mut preparer = Preparer::default();
        let previous = preparer.prepare(&scene, || true);
        let Scene::Rewards(rewards) = &mut scene else {
            unreachable!()
        };
        let asset = rewards
            .items
            .iter_mut()
            .flat_map(|item| &mut item.parts)
            .filter_map(|part| part.asset.as_mut())
            .find(|asset| previous.images.contains_key(&asset.digest))
            .unwrap();
        let old_digest = asset.digest.clone();
        let old_path = asset.path.clone();
        asset.digest = "replacement".into();
        let current = preparer.prepare(&scene, || true);
        let old = &previous.images[&old_digest];
        let new = &current.images["replacement"];
        assert!(!std::ptr::eq(&**old, &**new));
        assert_eq!(old.byte_len(), new.byte_len());
        assert!(preparer.cache.get(&source_key(&old_digest)).is_none());

        let Scene::Rewards(rewards) = &mut scene else {
            unreachable!()
        };
        for asset in rewards.items.iter_mut().flat_map(|item| {
            item.asset
                .iter_mut()
                .chain(item.parts.iter_mut().filter_map(|part| part.asset.as_mut()))
        }) {
            if asset.path == old_path {
                asset.path = "/missing-but-already-cached.png".into();
            }
        }
        let cached = preparer.prepare(&scene, || true);
        assert!(std::ptr::eq(
            &*cached.images["replacement"],
            &*current.images["replacement"]
        ));
        let empty = preparer.prepare(&Scene::Reading, || true);
        assert!(empty.images.is_empty());
        assert_eq!(preparer.cache.stats().cached_bytes, 0);
        assert!(preparer.cache.stats().retained_bytes > 0);
        drop((cached, current, previous));
        assert_eq!(preparer.cache.stats().live_bytes, 0);
    }

    #[test]
    fn cancelled_preparation_does_not_read_files() {
        let scene = super::super::screens::mock_relic_scene();
        let mut preparer = Preparer::default();
        let cancelled = preparer.prepare(&scene, || false);
        assert!(cancelled.images.is_empty());
        assert!(cancelled.issues.is_empty());
        assert_eq!(preparer.cache.stats().misses, 0);
    }

    #[test]
    fn failed_decode_is_reported_and_retried_after_source_recovers() {
        let mut scene = super::super::screens::mock_relic_scene();
        let Scene::Rewards(rewards) = &mut scene else {
            unreachable!()
        };
        rewards.items.truncate(1);
        let mut broken = scene.clone();
        let Scene::Rewards(rewards) = &mut broken else {
            unreachable!()
        };
        let asset = rewards
            .items
            .iter_mut()
            .flat_map(|item| &mut item.parts)
            .find_map(|part| part.asset.as_mut())
            .unwrap();
        let digest = asset.digest.clone();
        asset.path = "/missing-scene-image.png".into();
        let mut preparer = Preparer::default();
        let failed = preparer.prepare(&broken, || true);
        assert!(!failed.images.contains_key(&digest));
        assert!(!failed.issues.is_empty());
        let recovered = preparer.prepare(&scene, || true);
        assert!(recovered.images.contains_key(&digest));
        assert!(recovered.issues.is_empty());
    }

    #[test]
    fn loader_notifies_and_delivers_prepared_scene() {
        let (ready, results) = std::sync::mpsc::channel();
        let mut loader = Loader::spawn(move || {
            let _ = ready.send(());
        })
        .unwrap();
        loader.request(super::super::screens::mock_relic_scene());
        results
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!loader.poll().unwrap().images.is_empty());
        loader.request(Scene::Reading);
        results
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(loader.poll().unwrap().images.is_empty());
        assert!(loader.poll().is_none());
    }

    #[test]
    #[ignore = "manual cached-asset decode benchmark"]
    fn benchmark_scene_assets() {
        let mut scene = super::super::screens::mock_relic_scene();
        if let Ok(directory) = std::env::var("WFCOMPANION_BENCH_ASSET_DIR") {
            let paths = std::fs::read_dir(directory)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    matches!(
                        path.extension().and_then(|value| value.to_str()),
                        Some("png" | "webp")
                    )
                });
            let Scene::Rewards(rewards) = &mut scene else {
                unreachable!()
            };
            for (asset, path) in rewards
                .items
                .iter_mut()
                .flat_map(|reward| {
                    reward.asset.iter_mut().chain(
                        reward
                            .parts
                            .iter_mut()
                            .filter_map(|part| part.asset.as_mut()),
                    )
                })
                .zip(paths)
            {
                asset.path = path.to_string_lossy().into_owned();
                asset.digest = asset.path.clone();
            }
        }
        let mut preparer = Preparer::default();
        for pass in ["decode", "reuse"] {
            let started = std::time::Instant::now();
            let assets = preparer.prepare(&scene, || true);
            eprintln!(
                "scene_assets pass={pass} elapsed_us={} images={} bytes={} errors={}",
                started.elapsed().as_micros(),
                assets.images.len(),
                assets
                    .images
                    .values()
                    .map(|image| image.byte_len())
                    .sum::<usize>(),
                assets.issues.len()
            );
        }
    }
}
