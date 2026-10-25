use std::collections::BTreeMap;
use std::time::Duration;

use crate::overlay::assets::SceneAssets;
use crate::ui::ScreenOutput;
use retinue_ui::painter::CachedFont;
use retinue_ui::painter::{Painter, RasterImage, load_icon};
use retinue_ui::resources::Resource;

mod fixture;
mod reward;
mod suggestion;

struct SuggestionIcons {
    trace: RasterImage,
    vaulted: RasterImage,
    close: RasterImage,
}

impl SuggestionIcons {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            trace: load_icon(include_bytes!("../../../assets/trace.png"))?,
            vaulted: load_icon(include_bytes!("../../../assets/vaulted.png"))?,
            close: load_icon(include_bytes!("../../../assets/close.png"))?,
        })
    }
}

pub(super) struct Assets {
    platinum_icon: RasterImage,
    ducat_icon: RasterImage,
    icons: SuggestionIcons,
    permanent_images: BTreeMap<String, RasterImage>,
    pub(super) scene_assets: SceneAssets,
}

impl Assets {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            platinum_icon: load_icon(include_bytes!("../../../assets/platinum.png"))?,
            ducat_icon: load_icon(include_bytes!("../../../assets/ducats.png"))?,
            icons: SuggestionIcons::load()?,
            permanent_images: permanent_images()?,
            scene_assets: SceneAssets::default(),
        })
    }

    #[cfg(test)]
    fn cache_scene(&mut self, scene: &crate::relic::Scene) -> Vec<serde_json::Value> {
        self.scene_assets = crate::overlay::assets::Preparer::default().prepare(scene, || true);
        self.scene_assets.issues.clone()
    }
    fn resources<'a>(&'a self, font: &'a CachedFont) -> Resources<'a> {
        Resources {
            font,
            platinum_icon: &self.platinum_icon,
            ducat_icon: &self.ducat_icon,
            icons: &self.icons,
            permanent_images: &self.permanent_images,
            scene_images: &self.scene_assets.images,
        }
    }
}

struct Resources<'a> {
    font: &'a CachedFont,
    platinum_icon: &'a RasterImage,
    ducat_icon: &'a RasterImage,
    icons: &'a SuggestionIcons,
    permanent_images: &'a BTreeMap<String, RasterImage>,
    scene_images: &'a BTreeMap<String, Resource<RasterImage>>,
}

impl Resources<'_> {
    fn asset_image(&self, digest: &str) -> Option<&RasterImage> {
        self.permanent_images
            .get(digest)
            .or_else(|| self.scene_images.get(digest).map(|image| &**image))
    }
}

#[derive(Clone, Copy)]
pub(super) struct View {
    pub(super) scale: u32,
    pub(super) suggestion_offset: usize,
    pub(super) interaction_active: bool,
    pub(super) close_hovered: bool,
}

pub(super) fn draw_static_relic_scene(
    painter: &mut Painter<'_>,
    font: &CachedFont,
    assets: &Assets,
    scene: &crate::relic::Scene,
    view: View,
) -> ScreenOutput {
    let resources = assets.resources(font);
    match scene {
        crate::relic::Scene::Suggestions(suggestions) => {
            suggestion::draw(painter, &resources, suggestions, view)
        }
        _ => ScreenOutput {
            animation_bounds: reward::draw(painter, &resources, scene, view),
            hit_regions: Vec::new(),
        },
    }
}

fn permanent_images() -> Result<BTreeMap<String, RasterImage>, Box<dyn std::error::Error>> {
    let mut images = BTreeMap::new();
    images.insert(
        crate::assets::FORMA_ASSET.image.key.to_owned(),
        load_icon(crate::assets::FORMA_ASSET.image.bytes)?,
    );
    Ok(images)
}

pub(super) fn draw_loading_pulse(
    painter: &mut Painter<'_>,
    bounds: crate::ui::Rect,
    scale: u32,
    elapsed: Duration,
) {
    reward::draw_loading_pulse(painter, bounds, scale, elapsed);
}

fn draw_vaulted_icon(
    painter: &mut Painter<'_>,
    icon: &RasterImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    overscan_percent: u32,
) {
    let scaled_width = width * overscan_percent / 100;
    let scaled_height = height * overscan_percent / 100;
    painter.draw_image_contained(
        icon,
        x.saturating_sub((scaled_width - width) / 2),
        y.saturating_sub((scaled_height - height) / 2),
        scaled_width,
        scaled_height,
    );
}

pub(super) fn mock_relic_scene() -> crate::relic::Scene {
    fixture::scene()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_scene_has_four_reward_cards() {
        let crate::relic::Scene::Rewards(rewards) = fixture::scene() else {
            panic!("mock scene must contain rewards");
        };
        assert_eq!(rewards.items.len(), 4);
        assert_eq!(rewards.items[1].ducats, Some(100));
    }

    #[test]
    fn suggestion_fixture_has_scrollable_ranked_relics() {
        let crate::relic::Scene::Suggestions(suggestions) =
            crate::relic::suggestion_fixture().unwrap()
        else {
            panic!("fixture must contain suggestions");
        };
        assert_eq!(suggestions.items.len(), 8);
        assert!(
            suggestions.items[0].expected_platinum.unwrap()
                >= suggestions.items[1].expected_platinum.unwrap()
        );
    }

    #[test]
    fn scene_assets_are_loaded_from_preview_fixtures() {
        let mut assets = Assets::load().unwrap();
        assert_eq!(assets.permanent_images.len(), 1);
        assert!(
            assets
                .permanent_images
                .contains_key(crate::assets::FORMA_ASSET.image.key)
        );
        assert!(assets.cache_scene(&fixture::scene()).is_empty());
        for name in [
            "Blueprint",
            "Barrel",
            "Receiver",
            "Chassis",
            "Neuroptics",
            "Systems",
        ] {
            let asset = fixture::part_asset(name).unwrap();
            assert!(assets.scene_assets.images.contains_key(&asset.digest));
        }
    }

    #[test]
    fn scene_asset_failures_are_reported() {
        let mut scene = fixture::scene();
        let crate::relic::Scene::Rewards(rewards) = &mut scene else {
            panic!("mock scene must contain rewards");
        };
        let asset = rewards.items[0].parts[0].asset.as_mut().unwrap();
        asset.id = "missing:test".to_owned();
        asset.digest = "missing-digest".to_owned();
        asset.path = "/missing/wfcli-test.png".to_owned();

        let mut assets = Assets::load().unwrap();
        let issues = assets.cache_scene(&scene);
        assert!(
            issues
                .iter()
                .any(|issue| issue["identity"] == "missing:test")
        );
    }
}
