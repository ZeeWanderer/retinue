use fontdue::Font;
use retinue_overlay::painter::{Painter, RasterImage, TextLine, css_rgba, load_icon, text_width};

use crate::ducats::{Parent, Quote, Scene};
use crate::ui::Rect;

pub(in crate::overlay) struct Assets {
    platinum: RasterImage,
    owned: RasterImage,
    mastered: RasterImage,
    subsumed: RasterImage,
}

impl Assets {
    pub(super) fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            platinum: load_icon(include_bytes!("../../../assets/platinum.png"))?,
            owned: load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/owned.png"
            ))?,
            mastered: load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/mastered.png"
            ))?,
            subsumed: load_icon(include_bytes!(
                "../../../../../assets/warframe/ui/helminth.png"
            ))?,
        })
    }
}

fn row(scene: &Scene, dimensions: (u32, u32)) -> Option<(Rect, f32)> {
    let hover = &scene.hover;
    if hover.viewport != [dimensions.0, dimensions.1] {
        return None;
    }
    let s = hover.scale;
    if !s.is_finite() || !(0.25..=8.0).contains(&s) {
        return None;
    }
    let left = hover.price_label[2] + 8.0 * s;
    let right = hover.panel[2] - 40.0 * s;
    let top = hover.price_label[1];
    let bottom = hover.price_label[3];
    if left < 0.0
        || top < 0.0
        || right > dimensions.0 as f32
        || bottom > dimensions.1 as f32
        || right - left < 50.0 * s
        || bottom <= top
    {
        return None;
    }
    Some((
        Rect {
            x: left.ceil() as u32,
            y: top.ceil() as u32,
            width: (right - left).floor() as u32,
            height: (bottom - top).floor() as u32,
        },
        s,
    ))
}

pub(in crate::overlay) fn draw(
    painter: &mut Painter<'_>,
    font: &Font,
    assets: &Assets,
    scene: &Scene,
) {
    let Some((row, s)) = row(scene, (painter.width(), painter.height())) else {
        return;
    };
    let size = 19.0 * s;
    let icon = (18.0 * s).round() as u32;
    let gap = (6.0 * s).round() as u32;
    let mut x = row.x;
    if let Some(price) = price_text(scene) {
        let width = text_width(font, &price, size).ceil() as u32;
        if icon + gap + width <= row.width {
            let y = row.y + row.height.saturating_sub(icon) / 2;
            painter.draw_image_contained(&assets.platinum, x, y, icon, icon);
            painter.draw_text_vertically_centered(
                font,
                TextLine {
                    x: x + icon + gap,
                    y: row.y,
                    height: row.height,
                },
                size,
                &price,
                css_rgba(190, 169, 102, 255),
            );
            x += icon + gap + width + gap * 2;
        }
    }
    if let Some(parent) = &scene.parent {
        for (active, image, optical_size) in [
            (parent.owned, &assets.owned, 14.0),
            (parent.mastered, &assets.mastered, 18.0),
            (parent.subsumed, &assets.subsumed, 18.0),
        ] {
            if active && x + icon <= row.x + row.width {
                let drawn = (optical_size * s).round() as u32;
                let y = row.y + row.height.saturating_sub(drawn) / 2;
                painter.draw_image_contained(image, x + (icon - drawn) / 2, y, drawn, drawn);
                x += icon + gap;
            }
        }
    }
}

fn price_text(scene: &Scene) -> Option<String> {
    if !scene.listed {
        None
    } else if let Some(quote) = &scene.quote {
        quote.lowest_sell.map(|p| p.to_string())
    } else if scene.failed {
        Some("--".into())
    } else {
        Some("...".into())
    }
}

pub(in crate::overlay) fn fixture(dimensions: (u32, u32)) -> Scene {
    let s = dimensions.0 as f32 / 2560.0;
    Scene {
        hover: retinue_warframe::game_observer::ui::DucatHover {
            title: "AKJAGARA PRIME BARREL".into(),
            ducats: 100,
            viewport: [dimensions.0, dimensions.1],
            panel: [934.4, 264.0, 1494.4, 516.8].map(|v| v * s),
            price_label: [958.4, 344.0, 1078.4, 386.88].map(|v| v * s),
            scale: 1.6 * s,
        },
        parent: Some(Parent {
            name: "Akjagara Prime".into(),
            owned: true,
            mastered: true,
            subsumed: false,
        }),
        listed: true,
        quote: Some(Quote {
            lowest_sell: Some(8),
            quoted_at: 0,
            stale: false,
        }),
        failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_fits_beside_ducats_without_covering_native_control() {
        for size in [(1920, 1080), (2560, 1440), (2560, 1600), (3840, 2160)] {
            let scene = fixture(size);
            let (rect, scale) = row(&scene, size).unwrap();
            assert!(rect.x as f32 >= scene.hover.price_label[2]);
            assert!((rect.x + rect.width) as f32 <= scene.hover.panel[2] - 39.0 * scale);
        }
    }

    #[test]
    fn mismatched_window_size_is_not_guessed() {
        assert!(row(&fixture((2560, 1440)), (1920, 1080)).is_none());
    }

    #[test]
    fn absent_listing_hides_price_but_not_parent_status() {
        let mut scene = fixture((2560, 1440));
        scene.failed = true;
        assert_eq!(price_text(&scene).as_deref(), Some("8"));
        scene.failed = false;
        scene.listed = false;
        assert_eq!(price_text(&scene), None);
        assert!(scene.parent.as_ref().unwrap().owned);
        scene.listed = true;
        scene.quote.as_mut().unwrap().lowest_sell = None;
        assert_eq!(price_text(&scene), None);
        scene.quote = None;
        assert_eq!(price_text(&scene).as_deref(), Some("..."));
        scene.failed = true;
        assert_eq!(price_text(&scene).as_deref(), Some("--"));
        assert!(scene.parent.as_ref().unwrap().owned);
    }
}
