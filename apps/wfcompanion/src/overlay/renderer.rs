use std::path::Path;
use std::time::Duration;

use fontdue::Font;
use image::ImageFormat;

use crate::ui::ScreenOutput;
use retinue_overlay::painter::{Painter, load_overlay_font, straight_rgba};

use super::scene::{RelicView, Scene};
use super::screens::{self, Assets, mock_relic_scene};

pub(super) const LOADING_FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);

pub(super) struct Renderer {
    font: Font,
    assets: Assets,
    frame: Option<Frame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FrameKey {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale: u32,
    pub(super) scene: Scene,
}

pub(super) struct Frame {
    pub(super) key: FrameKey,
    pub(super) pixels: Vec<u8>,
    pub(super) output: ScreenOutput,
}

impl Frame {
    pub(super) fn contains_surface_point(
        &self,
        target: crate::ui::HitTarget,
        position: (f64, f64),
    ) -> bool {
        let scale = f64::from(self.key.scale);
        self.output
            .contains(target, (position.0 * scale, position.1 * scale))
    }
}

impl Renderer {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            font: load_overlay_font()?,
            assets: Assets::load()?,
            frame: None,
        })
    }

    pub(super) fn cache_scene_assets(
        &mut self,
        scene: &crate::relic::Scene,
    ) -> Vec<serde_json::Value> {
        let mut assets = super::assets::SceneAssets::default();
        assets.prepare(scene, || true);
        let issues = assets.issues.clone();
        self.install_scene_assets(assets);
        issues
    }

    pub(super) fn install_scene_assets(&mut self, assets: super::assets::SceneAssets) {
        self.assets.set_relic_assets(assets);
        self.frame = None;
    }

    pub(super) fn prepare_frame(&mut self, key: FrameKey) -> Result<bool, String> {
        ensure_frame(&mut self.frame, key, &self.font, &self.assets)
    }

    pub(super) fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    pub(super) fn draw_animation(&self, painter: &mut Painter<'_>, elapsed: Duration) {
        if let Some(frame) = self.frame.as_ref() {
            screens::draw_animation(
                painter,
                &frame.key.scene,
                &frame.output,
                frame.key.scale,
                elapsed,
            );
        }
    }

    pub(super) fn draw_status(&self, painter: &mut Painter<'_>, view: screens::StatusView<'_>) {
        screens::draw_status(painter, &self.font, view);
    }
}

pub(crate) fn save_relic_preview(dimensions: (u32, u32), path: &Path) -> Result<(), String> {
    save_preview_image(
        render_relic_scene_preview(dimensions, &mock_relic_scene(), Duration::ZERO)?,
        path,
    )
}

pub(crate) fn save_relic_scene_preview(
    dimensions: (u32, u32),
    scene: &crate::relic::Scene,
    path: &Path,
) -> Result<(), String> {
    save_preview_image(
        render_relic_scene_preview(dimensions, scene, Duration::ZERO)?,
        path,
    )
}

pub(crate) fn save_relic_loading_preview(
    dimensions: (u32, u32),
    path: &Path,
) -> Result<(), String> {
    save_preview_image(
        render_relic_loading_preview(dimensions, Duration::from_millis(750))?,
        path,
    )
}

pub(crate) fn save_relic_suggestions_preview(
    dimensions: (u32, u32),
    path: &Path,
) -> Result<(), String> {
    let scene = crate::relic::suggestion_fixture()?;
    save_preview_image(
        render_relic_scene_preview(dimensions, &scene, Duration::ZERO)?,
        path,
    )
}

pub(crate) fn save_notification_preview(dimensions: (u32, u32), path: &Path) -> Result<(), String> {
    save_preview_image(render_notification_preview(dimensions)?, path)
}

pub(crate) fn render_relic_loading_preview(
    dimensions: (u32, u32),
    elapsed: Duration,
) -> Result<image::RgbaImage, String> {
    render_relic_scene_preview(dimensions, &crate::relic::Scene::Reading, elapsed)
}

pub(crate) fn render_relic_suggestions_preview(
    dimensions: (u32, u32),
    elapsed: Duration,
) -> Result<image::RgbaImage, String> {
    let scene = crate::relic::suggestion_fixture()?;
    let item_count = match &scene {
        crate::relic::Scene::Suggestions(suggestions) => suggestions.items.len(),
        _ => 0,
    };
    let offset = suggestion_preview_offset(item_count, elapsed);
    render_relic_scene_preview_with_view(
        dimensions,
        &scene,
        elapsed,
        RelicView {
            suggestion_offset: offset,
            interaction_active: true,
            close_hovered: false,
        },
    )
}

fn render_relic_scene_preview(
    dimensions: (u32, u32),
    scene: &crate::relic::Scene,
    elapsed: Duration,
) -> Result<image::RgbaImage, String> {
    render_relic_scene_preview_with_view(
        dimensions,
        scene,
        elapsed,
        RelicView {
            suggestion_offset: 0,
            interaction_active: false,
            close_hovered: false,
        },
    )
}

fn render_relic_scene_preview_with_view(
    dimensions: (u32, u32),
    scene: &crate::relic::Scene,
    elapsed: Duration,
    view: RelicView,
) -> Result<image::RgbaImage, String> {
    let (width, height) = dimensions;
    let mut overlay = vec![0; (width * height * 4) as usize];
    let mut renderer =
        Renderer::load().map_err(|error| format!("could not load overlay renderer: {error}"))?;
    let _ = renderer.cache_scene_assets(scene);
    let mut painter = Painter::new(&mut overlay, width, height)
        .map_err(|error| format!("could not create overlay painter: {error}"))?;
    let scene = Scene::Relic {
        content: scene.clone(),
        view,
    };
    let output = screens::draw_static(&mut painter, &renderer.font, &renderer.assets, &scene, 1);
    screens::draw_animation(&mut painter, &scene, &output, 1, elapsed);
    painter
        .finish()
        .map_err(|error| format!("could not render overlay: {error}"))?;

    straight_rgba(&overlay, width, height)
}

fn suggestion_preview_offset(item_count: usize, elapsed: Duration) -> usize {
    const STEPS: [usize; 4] = [0, 2, 4, 2];
    let step = (elapsed.as_secs() as usize) % STEPS.len();
    STEPS[step].min(super::scene::max_suggestion_offset(item_count))
}

fn render_notification_preview(dimensions: (u32, u32)) -> Result<image::RgbaImage, String> {
    let mut surface = vec![0; (screens::STATUS_WIDTH * screens::STATUS_HEIGHT * 4) as usize];
    let renderer =
        Renderer::load().map_err(|error| format!("could not load overlay renderer: {error}"))?;
    let mut painter = Painter::new(&mut surface, screens::STATUS_WIDTH, screens::STATUS_HEIGHT)
        .map_err(|error| format!("could not create overlay painter: {error}"))?;
    painter.clear();
    renderer.draw_status(
        &mut painter,
        screens::StatusView {
            scale: 1,
            origin: (0, 0),
            daemon: concat!("retinue-daemon ", env!("WFCLI_VERSION"), " - connected"),
            player: "Warframe running (pid 12345)",
            detail: "wfcli companion hud hide",
        },
    );
    painter
        .finish()
        .map_err(|error| format!("could not render overlay: {error}"))?;

    let (width, height) = dimensions;
    let mut output = vec![0; (width * height * 4) as usize];
    blit_surface(
        SurfaceMut {
            pixels: &mut output,
            width,
            height,
        },
        Surface {
            pixels: &surface,
            width: screens::STATUS_WIDTH,
            height: screens::STATUS_HEIGHT,
        },
        (screens::STATUS_INSET, screens::STATUS_INSET),
    );
    straight_rgba(&output, width, height)
}

fn save_preview_image(output: image::RgbaImage, path: &Path) -> Result<(), String> {
    output
        .save_with_format(path, ImageFormat::Png)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(())
}

struct Surface<'a> {
    pixels: &'a [u8],
    width: u32,
    height: u32,
}

struct SurfaceMut<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
}

fn blit_surface(target: SurfaceMut<'_>, source: Surface<'_>, origin: (u32, u32)) {
    let copy_width = source.width.min(target.width.saturating_sub(origin.0));
    let copy_height = source.height.min(target.height.saturating_sub(origin.1));
    for row in 0..copy_height {
        let source_start = (row * source.width * 4) as usize;
        let target_start = (((origin.1 + row) * target.width + origin.0) * 4) as usize;
        let bytes = (copy_width * 4) as usize;
        target.pixels[target_start..target_start + bytes]
            .copy_from_slice(&source.pixels[source_start..source_start + bytes]);
    }
}

fn ensure_frame(
    cache: &mut Option<Frame>,
    key: FrameKey,
    font: &Font,
    assets: &Assets,
) -> Result<bool, String> {
    if cache.as_ref().is_some_and(|cached| cached.key == key) {
        return Ok(false);
    }

    let length = key
        .width
        .checked_mul(key.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| "relic frame dimensions overflow".to_owned())?;
    let mut pixels = vec![0; length];
    let mut painter = Painter::new(&mut pixels, key.width, key.height)
        .map_err(|error| format!("could not create relic frame painter: {error}"))?;
    painter.clear();
    let output = screens::draw_static(&mut painter, font, assets, &key.scene, key.scale);
    painter
        .finish()
        .map_err(|error| format!("could not render relic frame: {error}"))?;
    *cache = Some(Frame {
        key,
        pixels,
        output,
    });
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn suggestion_preview_scrolls_complete_rows() {
        assert_eq!(suggestion_preview_offset(8, Duration::ZERO), 0);
        assert_eq!(suggestion_preview_offset(8, Duration::from_secs(1)), 2);
        assert_eq!(suggestion_preview_offset(8, Duration::from_secs(2)), 4);
        assert_eq!(suggestion_preview_offset(8, Duration::from_secs(3)), 2);
        assert_eq!(suggestion_preview_offset(5, Duration::from_secs(2)), 2);
    }

    #[test]
    fn static_relic_frame_is_reused_until_scene_changes() {
        let mut renderer = Renderer::load().unwrap();
        let key = FrameKey {
            width: 2560,
            height: 1440,
            scale: 1,
            scene: Scene::Relic {
                content: mock_relic_scene(),
                view: RelicView {
                    suggestion_offset: 0,
                    interaction_active: false,
                    close_hovered: false,
                },
            },
        };

        assert!(renderer.prepare_frame(key.clone()).unwrap());
        assert!(!renderer.prepare_frame(key).unwrap());
        assert!(renderer.frame().unwrap().output.animation_bounds.is_none());

        let reading = FrameKey {
            width: 2560,
            height: 1440,
            scale: 1,
            scene: Scene::Relic {
                content: crate::relic::Scene::Reading,
                view: RelicView {
                    suggestion_offset: 0,
                    interaction_active: false,
                    close_hovered: false,
                },
            },
        };
        assert!(renderer.prepare_frame(reading).unwrap());
        assert!(renderer.frame().unwrap().output.animation_bounds.is_some());
    }

    #[test]
    fn suggestion_frame_exposes_rendered_hit_regions() {
        let mut renderer = Renderer::load().unwrap();
        let content = crate::relic::suggestion_fixture().unwrap();
        renderer
            .prepare_frame(FrameKey {
                width: 2560,
                height: 1440,
                scale: 1,
                scene: Scene::Relic {
                    content,
                    view: RelicView {
                        suggestion_offset: 0,
                        interaction_active: true,
                        close_hovered: false,
                    },
                },
            })
            .unwrap();

        let output = &renderer.frame().unwrap().output;
        assert!(output.contains(crate::ui::HitTarget::Content, (2060.0, 20.0)));
        assert!(output.contains(crate::ui::HitTarget::Close, (2503.0, 30.0)));
        assert!(output.contains(crate::ui::HitTarget::Scroll, (2062.0, 66.0)));
        assert!(!output.contains(crate::ui::HitTarget::Content, (2059.0, 20.0)));
    }

    #[test]
    fn scaled_suggestion_frame_accepts_surface_coordinates() {
        let mut renderer = Renderer::load().unwrap();
        for scale in [1, 2] {
            renderer
                .prepare_frame(FrameKey {
                    width: 1280 * scale,
                    height: 720 * scale,
                    scale,
                    scene: Scene::Relic {
                        content: crate::relic::suggestion_fixture().unwrap(),
                        view: RelicView {
                            suggestion_offset: 0,
                            interaction_active: true,
                            close_hovered: false,
                        },
                    },
                })
                .unwrap();
            let frame = renderer.frame().unwrap();
            for region in &frame.output.hit_regions {
                let bounds = region.bounds;
                let divisor = f64::from(scale);
                let center = (
                    (f64::from(bounds.x) + f64::from(bounds.width) / 2.0) / divisor,
                    (f64::from(bounds.y) + f64::from(bounds.height) / 2.0) / divisor,
                );
                assert!(
                    frame.contains_surface_point(region.target, center),
                    "scale={scale}"
                );
                let outside = (f64::from(bounds.x + bounds.width) / divisor, center.1);
                assert!(!frame.contains_surface_point(region.target, outside));
            }
        }
    }

    #[test]
    fn status_panel_can_be_composited_over_contextual_scene() {
        let mut canvas = vec![0; 600 * 180 * 4];
        let renderer = Renderer::load().unwrap();
        let mut painter = Painter::new(&mut canvas, 600, 180).unwrap();
        painter.clear();
        renderer.draw_status(
            &mut painter,
            screens::StatusView {
                scale: 1,
                origin: (24, 24),
                daemon: "DEV | retinue-daemon connected",
                player: "Warframe running (pid 12345)",
                detail: "DBWIN on | dbg 8 | relic rewards",
            },
        );
        painter.finish().unwrap();

        assert_eq!(pixel(&canvas, 600, 12, 12), [0, 0, 0, 0]);
        assert!(pixel(&canvas, 600, 40, 40)[3] > 0);
        assert_eq!(pixel(&canvas, 600, 580, 160), [0, 0, 0, 0]);
    }

    #[test]
    #[ignore = "manual renderer benchmark"]
    fn benchmark_relic_scene_renderer() {
        const FRAMES: u32 = 10;
        let mut renderer = Renderer::load().unwrap();
        let started = Instant::now();
        for frame in 0..FRAMES {
            renderer
                .prepare_frame(FrameKey {
                    width: 2560,
                    height: 1440,
                    scale: 1,
                    scene: Scene::Relic {
                        content: mock_relic_scene(),
                        view: RelicView {
                            suggestion_offset: frame as usize,
                            interaction_active: false,
                            close_hovered: false,
                        },
                    },
                })
                .unwrap();
        }
        let elapsed = started.elapsed();
        eprintln!(
            "renderer benchmark: {FRAMES} frames in {elapsed:?}, {:.3} ms/frame",
            elapsed.as_secs_f64() * 1000.0 / f64::from(FRAMES)
        );
    }

    fn pixel(canvas: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        canvas[offset..offset + 4].try_into().unwrap()
    }
}
