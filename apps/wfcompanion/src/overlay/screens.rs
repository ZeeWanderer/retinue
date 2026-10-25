pub(super) mod ducats;
mod relic;
mod status;

use std::time::Duration;

use super::assets::SceneAssets;
use retinue_ui::painter::CachedFont;

use super::scene::Scene;
use crate::ui::ScreenOutput;
use retinue_ui::painter::Painter;

pub(super) use status::{
    HEIGHT as STATUS_HEIGHT, INSET as STATUS_INSET, View as StatusView, WIDTH as STATUS_WIDTH,
};

pub(super) struct Assets {
    relic: relic::Assets,
    pub(super) ducats: ducats::Assets,
}

impl Assets {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            relic: relic::Assets::load()?,
            ducats: ducats::Assets::load()?,
        })
    }

    pub(super) fn set_relic_assets(&mut self, assets: SceneAssets) {
        self.relic.scene_assets = assets;
    }
}

pub(super) fn mock_relic_scene() -> crate::relic::Scene {
    relic::mock_relic_scene()
}

pub(super) fn draw_static(
    painter: &mut Painter<'_>,
    font: &CachedFont,
    assets: &Assets,
    scene: &Scene,
    scale: u32,
) -> ScreenOutput {
    match scene {
        Scene::Relic { content, view } => relic::draw_static_relic_scene(
            painter,
            font,
            &assets.relic,
            content,
            relic::View {
                scale,
                suggestion_offset: view.suggestion_offset,
                interaction_active: view.interaction_active,
                close_hovered: view.close_hovered,
            },
        ),
    }
}

pub(super) fn draw_animation(
    painter: &mut Painter<'_>,
    scene: &Scene,
    output: &ScreenOutput,
    scale: u32,
    elapsed: Duration,
) {
    match scene {
        Scene::Relic { .. } => {
            if let Some(bounds) = output.animation_bounds {
                relic::draw_loading_pulse(painter, bounds, scale, elapsed);
            }
        }
    }
}

pub(super) fn draw_status(painter: &mut Painter<'_>, font: &CachedFont, view: StatusView<'_>) {
    status::draw(painter, font, view);
}
