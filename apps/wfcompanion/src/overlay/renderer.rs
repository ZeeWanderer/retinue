use std::num::NonZeroU32;
use std::path::Path;
use std::time::Duration;

use image::ImageFormat;

use crate::ui::{Rect, ScreenOutput};
use retinue_ui::painter::{
    CachedFont, GlyphCache, Painter, load_overlay_font, render_thread_budget, straight_rgba,
};

use super::scene::{RelicView, Scene};
use super::screens::{self, Assets, mock_relic_scene};

pub(super) const LOADING_FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);

pub(super) struct Renderer {
    font: CachedFont,
    assets: Assets,
    frame: Option<Frame>,
    render_budget: NonZeroU32,
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
    #[cfg(test)]
    render_threads: u32,
}

pub(super) enum SurfaceScene<'a> {
    Empty,
    Relic(Duration),
    Ducats(&'a crate::ducats::Scene),
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

    fn restore(&self, target: &mut [u8], bounds: Rect) {
        if bounds.width == self.key.width && bounds.height == self.key.height {
            target.copy_from_slice(&self.pixels);
            return;
        }
        let stride = self.key.width as usize * 4;
        let row_bytes = bounds.width as usize * 4;
        let x = bounds.x as usize * 4;
        for row in bounds.y as usize..(bounds.y + bounds.height) as usize {
            let start = row * stride + x;
            target[start..start + row_bytes]
                .copy_from_slice(&self.pixels[start..start + row_bytes]);
        }
    }
}

impl Renderer {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            font: load_overlay_font(&GlyphCache::new(2 * 1024 * 1024))?,
            assets: Assets::load()?,
            frame: None,
            render_budget: render_thread_budget(2),
        })
    }

    pub(super) fn cache_scene_assets(
        &mut self,
        scene: &crate::relic::Scene,
    ) -> Vec<serde_json::Value> {
        let assets = super::assets::Preparer::default().prepare(scene, || true);
        let issues = assets.issues.clone();
        self.install_scene_assets(assets);
        issues
    }

    pub(super) fn install_scene_assets(&mut self, assets: super::assets::SceneAssets) {
        self.assets.set_relic_assets(assets);
        self.frame = None;
    }

    pub(super) fn prepare_frame(&mut self, key: FrameKey) -> Result<bool, String> {
        ensure_frame(
            &mut self.frame,
            key,
            &self.font,
            &self.assets,
            self.render_budget,
        )
    }

    pub(super) fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    pub(super) fn paint_surface(
        &self,
        pixels: &mut [u8],
        size: (u32, u32),
        repair: Rect,
        scene: SurfaceScene<'_>,
        status: Option<screens::StatusView<'_>>,
    ) -> Result<(), String> {
        let length = size
            .0
            .checked_mul(size.1)
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok());
        if length != Some(pixels.len()) {
            return Err("surface dimensions do not match buffer".to_owned());
        }
        let repair = repair.intersection(Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        });
        if repair.is_empty() {
            return Ok(());
        }
        let animation = if let SurfaceScene::Relic(elapsed) = scene {
            let cache = self
                .frame()
                .filter(|frame| (frame.key.width, frame.key.height) == size)
                .ok_or_else(|| "surface has no matching relic frame".to_owned())?;
            cache.restore(pixels, repair);
            cache
                .output
                .animation_bounds
                .filter(|bounds| !bounds.intersection(repair).is_empty())
                .map(|_| elapsed)
        } else {
            None
        };
        if matches!(scene, SurfaceScene::Relic(_)) && animation.is_none() && status.is_none() {
            return Ok(());
        }

        let mut painter =
            Painter::new(pixels, size.0, size.1).map_err(|error| error.to_string())?;
        painter.clip_to_rect(repair);
        if !matches!(scene, SurfaceScene::Relic(_)) {
            painter.clear();
        }
        if let SurfaceScene::Ducats(scene) = scene {
            self.draw_ducats(&mut painter, scene);
        }
        if let Some(elapsed) = animation {
            self.draw_animation(&mut painter, elapsed);
        }
        if let Some(view) = status {
            self.draw_status(&mut painter, view);
        }
        painter.finish().map_err(|error| error.to_string())
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

    pub(super) fn draw_ducats(&self, painter: &mut Painter<'_>, scene: &crate::ducats::Scene) {
        screens::ducats::draw(painter, &self.font, &self.assets.ducats, scene);
    }
}

pub(crate) fn save_ducat_preview(dimensions: (u32, u32), path: &Path) -> Result<(), String> {
    save_ducat_scene(dimensions, &screens::ducats::fixture(dimensions), path)
}

pub(crate) fn save_ducat_loading_preview(
    dimensions: (u32, u32),
    path: &Path,
) -> Result<(), String> {
    let mut scene = screens::ducats::fixture(dimensions);
    scene.quote = None;
    save_ducat_scene(dimensions, &scene, path)
}

pub(crate) fn save_ducat_unlisted_preview(
    dimensions: (u32, u32),
    path: &Path,
) -> Result<(), String> {
    let mut scene = screens::ducats::fixture(dimensions);
    scene.quote = None;
    scene.listed = false;
    save_ducat_scene(dimensions, &scene, path)
}

fn save_ducat_scene(
    dimensions: (u32, u32),
    scene: &crate::ducats::Scene,
    path: &Path,
) -> Result<(), String> {
    let (width, height) = dimensions;
    let mut pixels = vec![0; (width * height * 4) as usize];
    let renderer = Renderer::load().map_err(|e| e.to_string())?;
    renderer.paint_surface(
        &mut pixels,
        dimensions,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        SurfaceScene::Ducats(scene),
        None,
    )?;
    save_preview_image(straight_rgba(&pixels, width, height)?, path)
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
    let mut renderer =
        Renderer::load().map_err(|error| format!("could not load overlay renderer: {error}"))?;
    let _ = renderer.cache_scene_assets(scene);
    renderer.prepare_frame(FrameKey {
        width,
        height,
        scale: 1,
        scene: Scene::Relic {
            content: scene.clone(),
            view,
        },
    })?;
    let mut overlay = vec![0; renderer.frame().unwrap().pixels.len()];
    renderer.paint_surface(
        &mut overlay,
        dimensions,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        SurfaceScene::Relic(elapsed),
        None,
    )?;
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
    renderer.paint_surface(
        &mut surface,
        (screens::STATUS_WIDTH, screens::STATUS_HEIGHT),
        Rect {
            x: 0,
            y: 0,
            width: screens::STATUS_WIDTH,
            height: screens::STATUS_HEIGHT,
        },
        SurfaceScene::Empty,
        Some(screens::StatusView {
            scale: 1,
            origin: (0, 0),
            daemon: concat!("retinue-daemon ", env!("WFCLI_VERSION"), " - connected"),
            player: "Warframe running (pid 12345)",
            detail: "wfcli companion hud hide",
        }),
    )?;

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
    font: &CachedFont,
    assets: &Assets,
    threads: NonZeroU32,
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
    let mut painter = Painter::with_threads(&mut pixels, key.width, key.height, threads)
        .map_err(|error| format!("could not create relic frame painter: {error}"))?;
    painter.clear();
    let output = screens::draw_static(&mut painter, font, assets, &key.scene, key.scale);
    #[cfg(test)]
    let render_threads = painter.thread_count();
    painter
        .finish()
        .map_err(|error| format!("could not render relic frame: {error}"))?;
    *cache = Some(Frame {
        key,
        pixels,
        output,
        #[cfg(test)]
        render_threads,
    });
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_ui::damage::{Damage, RepaintTracker};

    fn reading_key(size: (u32, u32), scale: u32) -> FrameKey {
        FrameKey {
            width: size.0,
            height: size.1,
            scale,
            scene: Scene::Relic {
                content: crate::relic::Scene::Reading,
                view: RelicView {
                    suggestion_offset: 0,
                    interaction_active: false,
                    close_hovered: false,
                },
            },
        }
    }

    fn overlapping_status(scale: u32, pulse: Rect) -> screens::StatusView<'static> {
        screens::StatusView {
            scale,
            origin: (pulse.x / scale, pulse.y / scale),
            daemon: "DEV | retinue-daemon connected",
            player: "Warframe running",
            detail: "relic rewards",
        }
    }

    #[test]
    fn frame_rect_copy_leaves_other_pixels_untouched() {
        let frame = Frame {
            key: reading_key((8, 6), 1),
            pixels: vec![7; 8 * 6 * 4],
            output: ScreenOutput::default(),
            render_threads: 1,
        };
        let mut target = vec![3; frame.pixels.len()];
        frame.restore(
            &mut target,
            Rect {
                x: 2,
                y: 1,
                width: 3,
                height: 2,
            },
        );
        for y in 0..6 {
            for x in 0..8 {
                let value = if (2..5).contains(&x) && (1..3).contains(&y) {
                    7
                } else {
                    3
                };
                assert_eq!(pixel(&target, 8, x, y), [value; 4]);
            }
        }
        frame.restore(&mut target, Damage::Full.bounds((8, 6)).unwrap());
        assert_eq!(target, frame.pixels);
    }

    #[test]
    fn loading_repairs_match_full_paint_with_rotating_buffers_and_hud() {
        for (size, scale) in [((1920, 1080), 1), ((2560, 1440), 1), ((2560, 1440), 2)] {
            let mut renderer = Renderer::load().unwrap();
            let key = reading_key(size, scale);
            renderer.prepare_frame(key.clone()).unwrap();
            let pulse = renderer.frame().unwrap().output.animation_bounds.unwrap();
            let mut buffers = vec![vec![0x71; (size.0 * size.1 * 4) as usize]; 3];
            let mut repaint = RepaintTracker::default();
            for _ in &buffers {
                repaint.add_buffer();
            }
            let mut partial_repairs = 0;
            for step in 0..16 {
                let show_hud = (2..10).contains(&step);
                if step == 2 || step == 10 {
                    repaint.invalidate(Damage::Full);
                }
                if step == 6 {
                    renderer.install_scene_assets(super::super::assets::SceneAssets::default());
                    assert!(renderer.prepare_frame(key.clone()).unwrap());
                    repaint.invalidate(Damage::Full);
                }
                repaint.invalidate(Damage::rect(pulse));
                let index = [0, 1, 0, 2, 1][step % 5];
                let plan = repaint.plan(index, size).unwrap();
                let elapsed = Duration::from_millis(step as u64 * 137);
                let before = buffers[index].clone();
                renderer
                    .paint_surface(
                        &mut buffers[index],
                        size,
                        plan.repair,
                        SurfaceScene::Relic(elapsed),
                        show_hud.then(|| overlapping_status(scale, pulse)),
                    )
                    .unwrap();
                repaint.submitted(index);

                let mut expected = renderer.frame().unwrap().pixels.clone();
                let mut painter = Painter::new(&mut expected, size.0, size.1).unwrap();
                renderer.draw_animation(&mut painter, elapsed);
                if show_hud {
                    renderer.draw_status(&mut painter, overlapping_status(scale, pulse));
                }
                painter.finish().unwrap();
                assert!(
                    buffers[index]
                        .iter()
                        .zip(&expected)
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "full/partial mismatch: size={size:?} scale={scale} step={step}"
                );
                if plan.repair == pulse {
                    partial_repairs += 1;
                    for y in 0..size.1 {
                        for x in 0..size.0 {
                            if !pulse.contains(f64::from(x), f64::from(y)) {
                                assert_eq!(
                                    pixel(&buffers[index], size.0, x, y),
                                    pixel(&before, size.0, x, y),
                                );
                            }
                        }
                    }
                }
            }
            assert!(partial_repairs >= 3);
        }
    }

    #[test]
    fn surface_scene_transitions_erase_old_content() {
        let size = (1920, 1080);
        let full = Damage::Full.bounds(size).unwrap();
        let mut renderer = Renderer::load().unwrap();
        let mut key = reading_key(size, 1);
        let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
        let mut repaint = RepaintTracker::default();
        repaint.add_buffer();
        let ducats = screens::ducats::fixture(size);
        for step in 0..4 {
            let scene = match step {
                0 => SurfaceScene::Ducats(&ducats),
                1 | 2 => {
                    if step == 2 {
                        let Scene::Relic { content, .. } = &mut key.scene;
                        *content = mock_relic_scene();
                    }
                    assert!(renderer.prepare_frame(key.clone()).unwrap());
                    SurfaceScene::Relic(Duration::from_millis(750))
                }
                _ => SurfaceScene::Empty,
            };
            repaint.invalidate(Damage::Full);
            let plan = repaint.plan(0, size).unwrap();
            assert_eq!(plan.repair, full);
            renderer
                .paint_surface(&mut pixels, size, plan.repair, scene, None)
                .unwrap();
            repaint.submitted(0);
            match step {
                0 => assert!(pixels.iter().any(|byte| *byte != 0)),
                2 => assert!(pixels == renderer.frame().unwrap().pixels),
                3 => assert!(pixels.iter().all(|byte| *byte == 0)),
                _ => {}
            }
        }
    }

    #[test]
    fn surface_rejects_mismatched_frames_before_copying() {
        let mut renderer = Renderer::load().unwrap();
        let size = (1920, 1080);
        let full = Damage::Full.bounds(size).unwrap();
        let mut pixels = vec![0x71; (size.0 * size.1 * 4) as usize];
        assert!(
            renderer
                .paint_surface(
                    &mut pixels,
                    size,
                    full,
                    SurfaceScene::Relic(Duration::ZERO),
                    None
                )
                .is_err()
        );
        renderer.prepare_frame(reading_key((1280, 720), 1)).unwrap();
        assert!(
            renderer
                .paint_surface(
                    &mut pixels,
                    size,
                    full,
                    SurfaceScene::Relic(Duration::ZERO),
                    None
                )
                .is_err()
        );
        assert!(
            renderer
                .paint_surface(&mut pixels[..4], size, full, SurfaceScene::Empty, None)
                .is_err()
        );
        assert!(pixels.iter().all(|byte| *byte == 0x71));
    }

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
        assert!(!renderer.prepare_frame(key.clone()).unwrap());
        assert!(renderer.frame().unwrap().output.animation_bounds.is_none());
        let misses = renderer.font.cache_stats().misses;
        assert!(misses > 0);
        renderer.frame = None;
        assert!(renderer.prepare_frame(key).unwrap());
        assert_eq!(renderer.font.cache_stats().misses, misses);

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
    fn completed_frames_need_no_transient_surface_paint() {
        let mut renderer = Renderer::load().unwrap();
        for content in [
            mock_relic_scene(),
            crate::relic::suggestion_fixture().unwrap(),
        ] {
            renderer
                .prepare_frame(FrameKey {
                    width: 1920,
                    height: 1080,
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
            let frame = renderer.frame().unwrap();
            assert!(frame.output.animation_bounds.is_none());
            let mut surface = frame.pixels.clone();
            let mut painter = Painter::new(&mut surface, 1920, 1080).unwrap();
            renderer.draw_animation(&mut painter, Duration::from_millis(750));
            painter.finish().unwrap();
            assert!(
                surface == frame.pixels,
                "completed frame gained transient pixels"
            );
        }
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
    fn queued_thread_counts_match_relic_frames() {
        for (width, height) in [(1920, 1080), (2560, 1440)] {
            let mut reference = None;
            for threads in [1, 2, 3, 4, 6, 8] {
                let mut renderer = Renderer::load().unwrap();
                renderer.render_budget = NonZeroU32::new(threads).unwrap();
                renderer
                    .prepare_frame(FrameKey {
                        width,
                        height,
                        scale: 1,
                        scene: Scene::Relic {
                            content: mock_relic_scene(),
                            view: RelicView {
                                suggestion_offset: 0,
                                interaction_active: false,
                                close_hovered: false,
                            },
                        },
                    })
                    .unwrap();
                let pixels = &renderer.frame().unwrap().pixels;
                if let Some(reference) = &reference {
                    assert!(
                        pixels == reference,
                        "pixel mismatch: size {width}x{height}, threads {threads}"
                    );
                } else {
                    reference = Some(pixels.clone());
                }
            }
        }
    }

    #[test]
    #[ignore = "make bench-overlays"]
    fn overlay_benchmark() {
        let mut cases = Vec::new();
        for size in [(1920, 1080), (2560, 1440), (3840, 2160)] {
            if crate::render_bench::selected("loading-buffer-cycle") {
                cases.push(crate::render_bench::measure(
                    "loading-buffer-cycle",
                    size,
                    1.0,
                    || {
                        let mut renderer = Renderer::load().unwrap();
                        renderer.render_budget = crate::render_bench::render_threads();
                        let buffers = vec![vec![0; (size.0 * size.1 * 4) as usize]; 3];
                        let mut repaint = RepaintTracker::default();
                        for _ in &buffers {
                            repaint.add_buffer();
                        }
                        (renderer, buffers, repaint)
                    },
                    |(renderer, buffers, repaint), index| {
                        if renderer.prepare_frame(reading_key(size, 1)).unwrap() {
                            repaint.invalidate(Damage::Full);
                        }
                        repaint.invalidate(Damage::rect(
                            renderer.frame().unwrap().output.animation_bounds.unwrap(),
                        ));
                        let buffer_index = index % 3;
                        let plan = repaint.plan(buffer_index, size).unwrap();
                        renderer
                            .paint_surface(
                                &mut buffers[buffer_index],
                                size,
                                plan.repair,
                                SurfaceScene::Relic(Duration::from_millis(index as u64 * 33)),
                                None,
                            )
                            .unwrap();
                        repaint.submitted(buffer_index);
                    },
                    |(renderer, buffers, _)| {
                        serde_json::json!({
                            "canvas_bytes": buffers.iter().map(Vec::len).sum::<usize>(),
                            "buffer_count": buffers.len(),
                            "static_frame_bytes": renderer.frame().unwrap().pixels.len(),
                            "render_threads": renderer.frame().unwrap().render_threads,
                            "assets": "embedded UI; no daemon assets",
                            "scale_kind": "buffer scale; layout uses output dimensions"
                        })
                    },
                ));
            }
            for (name, changing) in [("static-frame-copy", false), ("scene-change", true)] {
                if !crate::render_bench::selected(name) {
                    continue;
                }
                cases.push(crate::render_bench::measure(
                    name,
                    size,
                    1.0,
                    || {
                        let mut renderer = Renderer::load().unwrap();
                        renderer.render_budget = crate::render_bench::render_threads();
                        (
                            renderer,
                            vec![0; (size.0 * size.1 * 4) as usize],
                            mock_relic_scene(),
                        )
                    },
                    |(renderer, pixels, content), index| {
                        renderer
                            .prepare_frame(FrameKey {
                                width: size.0,
                                height: size.1,
                                scale: 1,
                                scene: Scene::Relic {
                                    content: content.clone(),
                                    view: RelicView {
                                        suggestion_offset: if changing { index % 4 } else { 0 },
                                        interaction_active: false,
                                        close_hovered: false,
                                    },
                                },
                            })
                            .unwrap();
                        renderer
                            .paint_surface(
                                pixels,
                                size,
                                Damage::Full.bounds(size).unwrap(),
                                SurfaceScene::Relic(Duration::ZERO),
                                None,
                            )
                            .unwrap();
                    },
                    |(renderer, pixels, _)| {
                        let glyphs = renderer.font.cache_stats();
                        serde_json::json!({
                            "canvas_bytes": pixels.len(),
                            "render_threads": renderer.frame().unwrap().render_threads,
                            "static_frame_bytes": renderer.frame().unwrap().pixels.len(),
                            "glyph_cache": {
                                "entries": glyphs.entries,
                                "cached_bytes": glyphs.cached_bytes,
                                "live_bytes": glyphs.live_bytes,
                                "hits": glyphs.hits,
                                "misses": glyphs.misses,
                                "evictions": glyphs.evictions,
                            },
                            "assets": "embedded UI and mock scene; no daemon assets",
                            "scale_kind": "buffer scale; layout uses output dimensions"
                        })
                    },
                ));
            }
        }
        crate::render_bench::report("warframe", cases);
    }

    fn pixel(canvas: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        canvas[offset..offset + 4].try_into().unwrap()
    }
}
