use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::{PreviewRequest, PreviewSource};

type Renderer = fn((u32, u32), &Path) -> Result<(), String>;
type FrameRenderer = fn((u32, u32), Duration) -> Result<image::RgbaImage, String>;

#[derive(Clone, Copy)]
struct Animation {
    duration: Duration,
    fps: u32,
    render_frame: FrameRenderer,
}

struct Preview {
    name: &'static str,
    render: Renderer,
    animation: Option<Animation>,
}

const PREVIEWS: &[Preview] = &[
    Preview {
        name: "ducat-tooltip-loading",
        render: crate::overlay::save_ducat_loading_preview,
        animation: None,
    },
    Preview {
        name: "ducat-tooltip-unlisted",
        render: crate::overlay::save_ducat_unlisted_preview,
        animation: None,
    },
    Preview {
        name: "ducat-tooltip",
        render: crate::overlay::save_ducat_preview,
        animation: None,
    },
    Preview {
        name: "relic-loading",
        render: crate::overlay::save_relic_loading_preview,
        animation: Some(Animation {
            duration: Duration::from_secs(2),
            fps: 10,
            render_frame: crate::overlay::render_relic_loading_preview,
        }),
    },
    Preview {
        name: "relic-rewards",
        render: crate::overlay::save_relic_preview,
        animation: None,
    },
    Preview {
        name: "relic-suggestions",
        render: crate::overlay::save_relic_suggestions_preview,
        animation: Some(Animation {
            duration: Duration::from_secs(4),
            fps: 5,
            render_frame: crate::overlay::render_relic_suggestions_preview,
        }),
    },
    Preview {
        name: "notification",
        render: crate::overlay::save_notification_preview,
        animation: None,
    },
];

pub(crate) fn names(animated_only: bool) -> impl Iterator<Item = &'static str> {
    PREVIEWS
        .iter()
        .filter(move |preview| !animated_only || preview.animation.is_some())
        .map(|preview| preview.name)
}

pub(crate) fn render(
    request: PreviewRequest,
    dimensions: (u32, u32),
) -> Result<Vec<PathBuf>, String> {
    match request {
        PreviewRequest::One {
            name,
            path,
            background,
            source,
        } => {
            let preview = find(&name)?;
            ensure_parent(&path)?;
            match source {
                Some(source) => render_live(source, dimensions, &path)?,
                None => (preview.render)(dimensions, &path)?,
            }
            if let Some(background) = background {
                composite_file(&path, &background, dimensions)?;
            }
            Ok(vec![path])
        }
        PreviewRequest::All { directory } => {
            fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "could not create preview directory {}: {error}",
                    directory.display()
                )
            })?;
            PREVIEWS
                .iter()
                .map(|preview| {
                    let path = directory.join(format!("{}.png", preview.name));
                    (preview.render)(dimensions, &path)?;
                    Ok(path)
                })
                .collect()
        }
        PreviewRequest::Animate {
            name,
            path,
            background,
        } => {
            let preview = find(&name)?;
            ensure_parent(&path)?;
            let background = background
                .as_deref()
                .map(|path| load_background(path, dimensions))
                .transpose()?;
            render_animation(preview, dimensions, background.as_ref(), &path)?;
            Ok(vec![path])
        }
        PreviewRequest::AnimateAll { directory } => {
            fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "could not create preview directory {}: {error}",
                    directory.display()
                )
            })?;
            PREVIEWS
                .iter()
                .filter(|preview| preview.animation.is_some())
                .map(|preview| {
                    let path = directory.join(format!("{}.webm", preview.name));
                    render_animation(preview, dimensions, None, &path)?;
                    Ok(path)
                })
                .collect()
        }
        PreviewRequest::List { .. } => Ok(Vec::new()),
    }
}

fn render_live(source: PreviewSource, dimensions: (u32, u32), path: &Path) -> Result<(), String> {
    let (ui, _events) = crate::runtime::presentation::channel();
    let (relic, _triggers) = crate::relic::channel();
    let (diagnostics, _) = crate::runtime::diagnostics::channel();
    let connection = crate::daemon::spawn(ui, relic, diagnostics, "preview", Default::default())?;
    let outbound = connection.outbound();
    let result = (|| {
        let scene = match source {
            PreviewSource::RewardScreenshot(path) => {
                crate::relic::reward_preview_scene(&path, &outbound)?
            }
            PreviewSource::RelicInventory(era) => {
                crate::relic::suggestion_preview_scene(era, &outbound)?
            }
        };
        crate::overlay::save_relic_scene_preview(dimensions, &scene, path)
    })();
    result
}

fn render_animation(
    preview: &Preview,
    dimensions: (u32, u32),
    background: Option<&image::RgbaImage>,
    path: &Path,
) -> Result<(), String> {
    let animation = preview
        .animation
        .ok_or_else(|| format!("overlay preview has no animation: {}", preview.name))?;
    let executable = std::env::var_os("WFCOMPANION_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let video_size = format!("{}x{}", dimensions.0, dimensions.1);
    let mut child = Command::new(executable)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &video_size,
            "-framerate",
            &animation.fps.to_string(),
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
            "-deadline",
            "good",
            "-cpu-used",
            "4",
            "-metadata:s:v:0",
            "alpha_mode=1",
            "-f",
            "webm",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run ffmpeg: {error}"))?;

    let frame_count = (animation.duration.as_secs_f64() * f64::from(animation.fps))
        .ceil()
        .max(1.0) as u32;
    let write_result = {
        let stdin = child.stdin.as_mut().expect("piped ffmpeg stdin");
        (0..frame_count).try_for_each(|frame| {
            let elapsed = Duration::from_secs_f64(f64::from(frame) / f64::from(animation.fps));
            let mut image = (animation.render_frame)(dimensions, elapsed)?;
            if let Some(background) = background {
                image = composite(background, &image);
            }
            stdin
                .write_all(image.as_raw())
                .map_err(|error| format!("could not stream preview frame to ffmpeg: {error}"))
        })
    };
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for ffmpeg: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg exited with {}: {detail}", output.status));
    }
    write_result
}

fn composite_file(
    overlay_path: &Path,
    background_path: &Path,
    dimensions: (u32, u32),
) -> Result<(), String> {
    let background = load_background(background_path, dimensions)?;
    let overlay = image::open(overlay_path)
        .map_err(|error| format!("could not read preview {}: {error}", overlay_path.display()))?
        .into_rgba8();
    composite(&background, &overlay)
        .save(overlay_path)
        .map_err(|error| {
            format!(
                "could not write preview {}: {error}",
                overlay_path.display()
            )
        })
}

fn load_background(path: &Path, dimensions: (u32, u32)) -> Result<image::RgbaImage, String> {
    let image = image::open(path)
        .map_err(|error| {
            format!(
                "could not read preview background {}: {error}",
                path.display()
            )
        })?
        .into_rgba8();
    if image.dimensions() != dimensions {
        return Err(format!(
            "preview background {} is {}x{}, expected {}x{}",
            path.display(),
            image.width(),
            image.height(),
            dimensions.0,
            dimensions.1
        ));
    }
    Ok(image)
}

fn composite(background: &image::RgbaImage, overlay: &image::RgbaImage) -> image::RgbaImage {
    let mut result = background.clone();
    image::imageops::overlay(&mut result, overlay, 0, 0);
    for (pixel, background) in result.pixels_mut().zip(background.pixels()) {
        if background[3] == u8::MAX {
            pixel[3] = u8::MAX;
        }
    }
    result
}

pub(crate) fn current_dimensions() -> Result<(u32, u32), String> {
    if let Some(value) = std::env::var_os("WFCOMPANION_PREVIEW_SIZE") {
        return parse_dimensions(&value.to_string_lossy());
    }

    let executable =
        std::env::var_os("WFCOMPANION_KSCREEN_DOCTOR").unwrap_or_else(|| "kscreen-doctor".into());
    let output = Command::new(executable)
        .arg("-j")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("could not run kscreen-doctor: {error}"))?;
    if !output.status.success() {
        return Err(format!("kscreen-doctor exited with {}", output.status));
    }
    dimensions_from_kscreen(&output.stdout)
}

fn dimensions_from_kscreen(json: &[u8]) -> Result<(u32, u32), String> {
    let value: serde_json::Value = serde_json::from_slice(json)
        .map_err(|error| format!("could not parse kscreen-doctor output: {error}"))?;
    let outputs = value
        .get("outputs")
        .and_then(serde_json::Value::as_array)
        .ok_or("kscreen-doctor output has no outputs")?;
    let output = outputs
        .iter()
        .filter(|output| output.get("enabled").and_then(serde_json::Value::as_bool) == Some(true))
        .min_by_key(|output| {
            output
                .get("priority")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(u64::MAX)
        })
        .ok_or("kscreen-doctor reports no enabled output")?;
    let size = output.get("size").ok_or("enabled output has no size")?;
    dimensions_from_values(size.get("width"), size.get("height"))
}

fn parse_dimensions(value: &str) -> Result<(u32, u32), String> {
    let Some((width, height)) = value.split_once(['x', 'X']) else {
        return Err(format!(
            "invalid preview size: {value}; expected WIDTHxHEIGHT"
        ));
    };
    let width = width
        .parse::<u64>()
        .map(serde_json::Value::from)
        .map_err(|_| "invalid preview width")?;
    let height = height
        .parse::<u64>()
        .map(serde_json::Value::from)
        .map_err(|_| "invalid preview height")?;
    dimensions_from_values(Some(&width), Some(&height))
}

fn dimensions_from_values(
    width: Option<&serde_json::Value>,
    height: Option<&serde_json::Value>,
) -> Result<(u32, u32), String> {
    let width = width
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("invalid preview width")?;
    let height = height
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("invalid preview height")?;
    Ok((width, height))
}

fn find(name: &str) -> Result<&'static Preview, String> {
    PREVIEWS
        .iter()
        .find(|preview| preview.name == name)
        .ok_or_else(|| format!("unknown overlay preview: {name}"))
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create preview directory {}: {error}",
            parent.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_overlay_previews() {
        assert_eq!(
            names(false).collect::<Vec<_>>(),
            [
                "ducat-tooltip-loading",
                "ducat-tooltip-unlisted",
                "ducat-tooltip",
                "relic-loading",
                "relic-rewards",
                "relic-suggestions",
                "notification"
            ]
        );
        assert_eq!(
            names(true).collect::<Vec<_>>(),
            ["relic-loading", "relic-suggestions"]
        );
        assert!(find("relic-loading").unwrap().animation.is_some());
        assert!(find("relic-suggestions").unwrap().animation.is_some());
        assert!(find("notification").unwrap().animation.is_none());
        assert!(find("missing").is_err());
    }

    #[test]
    fn parses_override_and_primary_output_dimensions() {
        assert_eq!(parse_dimensions("2560x1440"), Ok((2560, 1440)));
        assert!(parse_dimensions("invalid").is_err());
        let json = br#"{
            "outputs": [
                {"enabled": true, "priority": 2, "size": {"width": 1920, "height": 1080}},
                {"enabled": true, "priority": 1, "size": {"width": 2560, "height": 1440}}
            ]
        }"#;
        assert_eq!(dimensions_from_kscreen(json), Ok((2560, 1440)));
    }

    #[test]
    fn composites_overlay_over_background() {
        let background = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 255, 255]));
        let overlay = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 128]));

        let result = composite(&background, &overlay);
        assert_eq!(result.get_pixel(0, 0)[3], 255);
        assert_ne!(result.get_pixel(0, 0), background.get_pixel(0, 0));
    }
}
