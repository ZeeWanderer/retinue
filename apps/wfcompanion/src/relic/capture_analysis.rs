use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use image::imageops::{FilterType, crop_imm, resize};
use image::{DynamicImage, GenericImageView, GrayImage, Luma};

use super::{Candidate, Context, Geometry, TESSERACT_ARGUMENTS, Trigger, capture};

pub(super) fn capture_trigger(trigger: &Trigger) -> Result<DynamicImage, String> {
    match trigger {
        Trigger::Rewards { .. } => capture::relic_window(),
        Trigger::Screenshot(path) => {
            image::open(path).map_err(|error| format!("could not read {}: {error}", path.display()))
        }
        Trigger::Suggestions { .. }
        | Trigger::CloseSuggestions
        | Trigger::DismissSuggestions { .. }
        | Trigger::ArmCapture(_)
        | Trigger::CancelCapture
        | Trigger::GameStopped
        | Trigger::IntakeGap
        | Trigger::SuggestionReady { .. }
        | Trigger::WorkFinished { .. } => {
            Err("trigger does not contain a reward capture".to_owned())
        }
    }
}

pub(super) fn suggestion_era(
    image: &DynamicImage,
    context: Option<&Context>,
) -> Result<String, String> {
    let x = image.width() * 25 / 1000;
    let y = image.height() * 65 / 1000;
    let width = (image.width() * 150 / 1000).min(image.width().saturating_sub(x));
    let height = (image.height() * 120 / 1000).min(image.height().saturating_sub(y));
    if width == 0 || height == 0 {
        return Err("relic suggestion crop is empty".to_owned());
    }
    let crop = crop_imm(image, x, y, width, height).to_image();
    parse_suggestion_era(&run_tesseract(preprocess(&crop), context)?)
        .ok_or_else(|| "could not identify relic era".to_owned())
}

pub(super) fn parse_suggestion_era(label: &str) -> Option<String> {
    let normalized = label
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    ["lith", "meso", "neo", "axi", "all"]
        .into_iter()
        .find(|era| normalized.contains(era))
        .map(str::to_owned)
}

pub(super) fn read_candidate(
    image: &DynamicImage,
    geometry: Geometry,
    count: usize,
    context: Option<&Context>,
) -> Result<Candidate, String> {
    let regions = reward_regions(image.width(), image.height(), geometry, count);
    let crops = regions
        .into_iter()
        .map(|(x, y, width, height)| crop_imm(image, x, y, width, height).to_image())
        .collect::<Vec<_>>();
    let labels = thread::scope(|scope| {
        let workers = crops
            .into_iter()
            .map(|crop| scope.spawn(move || run_tesseract(preprocess(&crop), context)))
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "tesseract worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    Ok(Candidate {
        geometry,
        count,
        labels,
    })
}

pub(super) fn reward_regions(
    screen_width: u32,
    screen_height: u32,
    geometry: Geometry,
    count: usize,
) -> Vec<(u32, u32, u32, u32)> {
    let (top, bottom, width_ratio, separation_ratio) = match geometry {
        Geometry::Normal => (0.38, 0.427, 0.121, 0.0053),
        Geometry::Legacy => (0.4027, 0.437, 0.1005, 0.0052),
    };
    let reference_width = 1920.0 * screen_height as f64 / 1080.0;
    let mut card_width = (reference_width * width_ratio).round().max(1.0);
    let separation = (reference_width * separation_ratio).round();
    let mut y = (screen_height as f64 * top).round().max(0.0);
    let mut bottom_y = (screen_height as f64 * bottom).round().max(y + 1.0);
    if screen_width == 2560 && screen_height == 1600 {
        card_width = (card_width * 0.9).round().max(1.0);
        y = (y * 1.04).round();
        bottom_y = (bottom_y * 1.013).round().max(y + 1.0);
    }
    let total_width = card_width * count as f64 + separation * count.saturating_sub(1) as f64;
    let start_x = (screen_width as f64 - total_width) / 2.0;
    let height = bottom_y - y;

    (0..count)
        .filter_map(|index| {
            let x = start_x + index as f64 * (card_width + separation);
            clamp_region(
                screen_width,
                screen_height,
                x.round() as i64,
                y as i64,
                card_width as i64,
                height as i64,
            )
        })
        .collect()
}

pub(super) fn detect_player_count(image: &DynamicImage, geometry: Geometry) -> Option<usize> {
    let counter = relic_counter_region(image.width(), image.height(), geometry)?;
    const PROBES: [(usize, [f64; 2]); 4] = [
        (4, [0.01, 0.91]),
        (3, [0.15, 0.78]),
        (2, [0.28, 0.665]),
        (1, [0.40, 0.535]),
    ];
    for (count, offsets) in PROBES {
        if offsets
            .into_iter()
            .filter_map(|offset| counter_probe(counter, offset))
            .any(|probe| has_reward_border(image, probe))
        {
            return Some(count);
        }
    }
    None
}

fn relic_counter_region(
    screen_width: u32,
    screen_height: u32,
    geometry: Geometry,
) -> Option<(u32, u32, u32, u32)> {
    let (counter_top, counter_bottom, width_ratio, separation_ratio) = match geometry {
        Geometry::Normal => (0.431, 0.458, 0.121, 0.0053),
        Geometry::Legacy => (0.441, 0.464, 0.1005, 0.0052),
    };
    let reference_width = 1920.0 * f64::from(screen_height) / 1080.0;
    let card_width = (reference_width * width_ratio) as i64;
    let separation = (reference_width * separation_ratio) as i64;
    let width = card_width * 4 + separation * 3;
    let x = i64::from(screen_width) / 2 - width / 2;
    let y = (f64::from(screen_height) * counter_top) as i64;
    let bottom = (f64::from(screen_height) * counter_bottom) as i64;
    let height = ((bottom - y) as f64 * 0.9) as i64;
    clamp_region(screen_width, screen_height, x, y, width, height)
}

fn counter_probe(counter: (u32, u32, u32, u32), offset: f64) -> Option<(u32, u32, u32, u32)> {
    let (x, y, width, height) = counter;
    let probe_x = x.saturating_add((f64::from(width) * offset) as u32);
    let probe_width = (f64::from(width) * 0.08) as u32;
    (probe_width > 0 && probe_x < x + width).then_some((
        probe_x,
        y,
        probe_width.min(x + width - probe_x),
        height,
    ))
}

fn has_reward_border(image: &DynamicImage, region: (u32, u32, u32, u32)) -> bool {
    let (x, y, width, height) = region;
    if width < 12 || height < 3 {
        return false;
    }
    let middle = y + height / 2;
    let start = x + width / 6;
    let end = x + width * 5 / 6;
    let mut previous = image.get_pixel(start, middle);
    let mut horizontal = 0_u32;
    for sample_x in start..end {
        let pixel = image.get_pixel(sample_x, middle);
        if color_distance(previous.0, pixel.0) >= 45.0 {
            break;
        }
        previous = pixel;
        horizontal += 1;
    }
    if horizontal as f64 / (f64::from(width) * 0.6) < 0.9 {
        return false;
    }

    let mut runs = Vec::with_capacity(width as usize);
    for sample_x in x..x + width {
        let mut previous = image.get_pixel(sample_x, middle);
        let mut run = 0_u32;
        for sample_y in (y..=middle).rev() {
            let pixel = image.get_pixel(sample_x, sample_y);
            if color_distance(previous.0, pixel.0) > 32.0 {
                break;
            }
            previous = pixel;
            run += 1;
        }
        for sample_y in middle + 1..y + height {
            let pixel = image.get_pixel(sample_x, sample_y);
            if color_distance(previous.0, pixel.0) > 32.0 {
                break;
            }
            previous = pixel;
            run += 1;
        }
        runs.push(f64::from(run));
    }

    if runs.iter().filter(|run| **run <= 1.0).count() > runs.len() / 3 {
        return false;
    }
    let average = runs.iter().sum::<f64>() / runs.len() as f64;
    let normalized_average = average / f64::from(height);
    if !(0.05..=0.27).contains(&normalized_average) {
        return false;
    }
    let mut sorted = runs.clone();
    sorted.sort_by(|left, right| right.total_cmp(left));
    let variance = sorted
        .iter()
        .skip(3)
        .map(|run| (run - average).powi(2))
        .sum::<f64>()
        / sorted.len().saturating_sub(3).max(1) as f64;
    5.0 * variance.sqrt() / f64::from(height) <= 0.36
}

fn color_distance(left: [u8; 4], right: [u8; 4]) -> f64 {
    let red = f64::from(left[0]) - f64::from(right[0]);
    let green = f64::from(left[1]) - f64::from(right[1]);
    let blue = f64::from(left[2]) - f64::from(right[2]);
    (red * red + green * green + blue * blue).sqrt()
}

fn clamp_region(
    screen_width: u32,
    screen_height: u32,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
) -> Option<(u32, u32, u32, u32)> {
    let left = x.clamp(0, i64::from(screen_width));
    let top = y.clamp(0, i64::from(screen_height));
    let right = (x + width).clamp(0, i64::from(screen_width));
    let bottom = (y + height).clamp(0, i64::from(screen_height));
    (right > left && bottom > top).then_some((
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

fn preprocess(source: &image::RgbaImage) -> GrayImage {
    let enlarged = resize(
        source,
        source.width().saturating_mul(3),
        source.height().saturating_mul(3),
        FilterType::Lanczos3,
    );
    let border = 12;
    let mut output = GrayImage::from_pixel(
        enlarged.width() + border * 2,
        enlarged.height() + border * 2,
        Luma([255]),
    );
    for (x, y, pixel) in enlarged.enumerate_pixels() {
        let [red, green, blue, _] = pixel.0;
        let luma = (u16::from(red) * 54 + u16::from(green) * 183 + u16::from(blue) * 19) / 256;
        output.put_pixel(
            x + border,
            y + border,
            Luma([if luma >= 135 { 0 } else { 255 }]),
        );
    }
    output
}

fn run_tesseract(image: GrayImage, context: Option<&Context>) -> Result<String, String> {
    if let Some(context) = context {
        context.check()?;
    }
    let path = capture::temporary_png("ocr")?;
    image
        .save(&path)
        .map_err(|error| format!("could not write OCR image: {error}"))?;
    let tesseract = crate::external::resolve(
        "WFCOMPANION_TESSERACT",
        "tesseract",
        option_env!("WFCOMPANION_BUILD_TESSERACT"),
        &[
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin/tesseract"),
            PathBuf::from("/usr/local/bin/tesseract"),
            PathBuf::from("/usr/bin/tesseract"),
        ],
    );
    let mut command = Command::new(tesseract);
    command.arg(&path).args(TESSERACT_ARGUMENTS);
    let output = retinue_client::process::output(command, Duration::from_secs(10), || {
        context.is_none_or(Context::is_current)
    })
    .map_err(|error| format!("could not run tesseract: {error}"));
    let _ = fs::remove_file(path);
    let output = output?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("tesseract exited with {}", output.status)
        } else {
            format!("tesseract: {message}")
        });
    }
    Ok(clean_label(&String::from_utf8_lossy(&output.stdout)))
}

pub(super) fn clean_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|character| match character {
            '|' | '~' | '"' | '=' | '?' | '!' | '\\' | '/' | '1'..='9' | '.' => ' ',
            _ => character,
        })
        .collect();
    let lines: Vec<String> = cleaned
        .lines()
        .map(|line| {
            line.split_whitespace()
                .filter_map(clean_token)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.is_empty())
        .collect();
    let has_substantial_line = lines.iter().any(|line| {
        line.chars()
            .filter(|character| character.is_alphanumeric())
            .count()
            >= 6
    });
    lines
        .into_iter()
        .filter(|line| {
            !has_substantial_line
                || line
                    .chars()
                    .filter(|character| character.is_alphanumeric())
                    .count()
                    > 2
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_owned()
}

fn clean_token(token: &str) -> Option<String> {
    let trimmed =
        token.trim_matches(|character: char| !character.is_alphanumeric() && character != '&');
    if trimmed.is_empty() || matches!(trimmed, "F" | "FF" | "L" | "LL") {
        return None;
    }
    let fixed = if trimmed.eq_ignore_ascii_case("Fanq") {
        "Fang"
    } else {
        trimmed
    };
    let alphanumeric = fixed
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    (alphanumeric > 2 || fixed.eq_ignore_ascii_case("bo") || fixed == "&").then(|| fixed.to_owned())
}
