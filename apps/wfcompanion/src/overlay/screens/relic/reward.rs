use retinue_ui::composition::{Canvas, Key, Recording};
use std::rc::Rc;
use std::time::Duration;

use retinue_ui::painter::CachedFont;

use super::{Resources, View};
use crate::ui::Rect;
use retinue_ui::painter::{RasterImage, TextBox, TextLine, css_rgba, fit_text_size, text_width};

mod layout;

use layout::{CardSpec, Layout};

const LOADING_CYCLE: f32 = 1.923_076_9;
const LOADING_RADIUS: u32 = 24;
const LOADING_STROKE: u32 = 6;

pub(super) fn draw(
    painter: &mut Recording,
    resources: &Resources<'_>,
    scene: &crate::relic::Scene,
    view: View,
) -> Option<Rect> {
    let Resources {
        font,
        platinum_icon,
        ducat_icon,
        icons,
        ..
    } = resources;
    let View {
        scale,
        interaction_active,
        ..
    } = view;
    let card_specs = match scene {
        crate::relic::Scene::Rewards(rewards) => rewards
            .items
            .iter()
            .take(4)
            .map(|reward| {
                let platinum = reward
                    .lowest_sell
                    .map_or_else(|| "--".to_owned(), |value| value.to_string());
                let ducats = reward
                    .ducats
                    .map_or_else(|| "--".to_owned(), |value| value.to_string());
                CardSpec {
                    platinum_width: text_width(font, &platinum, 19.0).ceil() + 31.0,
                    ducats_width: text_width(font, &ducats, 19.0).ceil() + 33.0,
                    vaulted: reward.vaulted,
                    ..CardSpec::default()
                }
            })
            .collect(),
        crate::relic::Scene::Suggestions(_) => unreachable!(),
        _ => vec![CardSpec::default(); 4],
    };
    let layout = layout::compute(
        painter.width() / scale,
        painter.height() / scale,
        &card_specs,
    )
    .expect("static relic layout must be valid")
    .scaled(scale);
    let card_bounds = layout.cards;
    draw_relic_shell(painter, &layout, scale);
    match scene {
        crate::relic::Scene::Reading => {
            draw_loading_background(painter, layout.holder, scale);
            return Some(loading_pulse_bounds(layout.holder, scale));
        }
        crate::relic::Scene::Error(error) => {
            draw_message_panel(
                painter,
                font,
                card_bounds,
                600 * scale,
                "Relic reward scan failed",
                Some(&truncate(error, 72)),
                scale,
            );
        }
        crate::relic::Scene::Rewards(rewards) => {
            let best = rewards
                .items
                .iter()
                .filter_map(|reward| reward.lowest_sell)
                .max();
            for (index, (reward, card_layout)) in rewards
                .items
                .iter()
                .take(4)
                .zip(&layout.reward_cards)
                .enumerate()
            {
                let card = card_layout.card;
                painter.section(Key::new("relic.reward", index as u64), card);
                let selected = reward.lowest_sell.is_some() && reward.lowest_sell == best;
                if selected {
                    painter.fill_rounded_rect(
                        card.x,
                        card.y,
                        card.width,
                        card.height,
                        15 * scale,
                        css_rgba(212, 225, 255, 199),
                    );
                }
                let inset = if selected { 2 * scale } else { 0 };
                painter.fill_rounded_rect(
                    card.x + inset,
                    card.y + inset,
                    card.width - inset * 2,
                    card.height - inset * 2,
                    15 * scale,
                    if selected {
                        css_rgba(49, 58, 88, 255)
                    } else {
                        css_rgba(32, 40, 62, 255)
                    },
                );
                let name_size = fit_text_size(
                    font,
                    &reward.name,
                    18.0 * scale as f32,
                    12.0 * scale as f32,
                    card_layout.name.width.saturating_sub(8 * scale),
                );
                painter.draw_centered_text(
                    font,
                    TextBox::new(
                        card_layout.name.x,
                        card_layout.name.y,
                        card_layout.name.width,
                    ),
                    name_size,
                    &reward.name,
                    [255, 255, 255, 255],
                );
                if reward.slug.is_none() && reward.lowest_sell.is_none() && reward.ducats.is_none()
                {
                    painter.draw_centered_text(
                        font,
                        TextBox::new(
                            card_layout.prices.x,
                            card_layout.prices.y + 4 * scale,
                            card_layout.prices.width,
                        ),
                        14.0 * scale as f32,
                        "No market listing",
                        [165, 165, 165, 255],
                    );
                } else {
                    draw_primary_price(
                        painter,
                        font,
                        PrimaryPrice {
                            icon: platinum_icon,
                            bounds: card_layout.platinum,
                            scale,
                            icon_size: 28,
                            price: reward.lowest_sell,
                            color: css_rgba(255, 255, 255, 255),
                        },
                    );
                    if let Some(bounds) = card_layout.vaulted {
                        painter.fill_rounded_rect(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            12 * scale,
                            css_rgba(73, 79, 117, 255),
                        );
                        super::draw_vaulted_icon(
                            painter,
                            &icons.vaulted,
                            bounds.x + 7 * scale,
                            bounds.y + 4 * scale,
                            22 * scale,
                            22 * scale,
                            149,
                        );
                    }
                    draw_primary_price(
                        painter,
                        font,
                        PrimaryPrice {
                            icon: ducat_icon,
                            bounds: card_layout.ducats,
                            scale,
                            icon_size: 30,
                            price: reward.ducats,
                            color: css_rgba(255, 255, 255, 255),
                        },
                    );
                }
                draw_reward_ownership(painter, font, reward, card_layout.ownership, scale);
                draw_reward_parts(painter, resources, reward, card_layout.components, scale);
            }
            painter.section(Key::new("relic.footer", 0), layout.footer);
            draw_account_currency(
                painter,
                resources,
                layout.footer,
                scale,
                &rewards.account,
                interaction_active,
            );
        }
        crate::relic::Scene::Suggestions(_) => unreachable!(),
    }
    None
}

fn draw_loading_background(painter: &mut Canvas, bounds: Rect, scale: u32) {
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        18 * scale,
        css_rgba(28, 31, 32, 56),
    );
}

fn loading_pulse_bounds(bounds: Rect, scale: u32) -> Rect {
    let radius = (LOADING_RADIUS + LOADING_STROKE.div_ceil(2) + 1) * scale;
    let center_x = bounds.x + bounds.width / 2;
    let center_y = bounds.y + bounds.height / 2;
    Rect {
        x: center_x.saturating_sub(radius),
        y: center_y.saturating_sub(radius),
        width: (radius * 2).min(bounds.width),
        height: (radius * 2).min(bounds.height),
    }
}

pub(super) fn draw_loading_pulse(
    painter: &mut Canvas,
    bounds: Rect,
    scale: u32,
    elapsed: Duration,
) {
    let center_x = bounds.x + bounds.width / 2;
    let center_y = bounds.y + bounds.height / 2;
    let cycle = elapsed.as_secs_f32() / LOADING_CYCLE;
    for (offset, color) in [(0.0, (147, 219, 233)), (0.5, (104, 156, 197))] {
        let phase = (cycle + offset) % 1.0;
        let radius = (phase * LOADING_RADIUS as f32 * scale as f32).round() as u32;
        let alpha = ((1.0 - phase) * 255.0).round() as u8;
        painter.draw_ring(
            center_x,
            center_y,
            radius,
            LOADING_STROKE * scale,
            css_rgba(color.0, color.1, color.2, alpha),
        );
    }
}

fn draw_relic_shell(painter: &mut Canvas, layout: &Layout, scale: u32) {
    let bounds = layout.shell;
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        20 * scale,
        css_rgba(255, 255, 255, 51),
    );
    let inner = bounds.inset(2 * scale);
    painter.fill_rounded_rect(
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        18 * scale,
        css_rgba(16, 22, 35, 255),
    );
    fill_bottom_rounded(
        painter,
        layout.footer,
        18 * scale,
        css_rgba(23, 30, 48, 255),
    );
}

fn fill_bottom_rounded(painter: &mut Canvas, bounds: Rect, radius: u32, color: [u8; 4]) {
    debug_assert_eq!(color[3], 255, "overlapping fills require an opaque color");
    let radius = radius.min(bounds.width / 2).min(bounds.height / 2);
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        radius,
        color,
    );
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height.saturating_sub(radius),
        0,
        color,
    );
}

fn fill_right_rounded(painter: &mut Canvas, bounds: Rect, radius: u32, color: [u8; 4]) {
    debug_assert_eq!(color[3], 255, "overlapping fills require an opaque color");
    let radius = radius.min(bounds.width / 2).min(bounds.height / 2);
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        radius,
        color,
    );
    painter.fill_rounded_rect(bounds.x, bounds.y, radius, bounds.height, 0, color);
}

fn fill_left_rounded(painter: &mut Canvas, bounds: Rect, radius: u32, color: [u8; 4]) {
    debug_assert_eq!(color[3], 255, "overlapping fills require an opaque color");
    let radius = radius.min(bounds.width / 2).min(bounds.height / 2);
    painter.fill_rounded_rect(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        radius,
        color,
    );
    painter.fill_rounded_rect(
        bounds.x + bounds.width.saturating_sub(radius),
        bounds.y,
        radius,
        bounds.height,
        0,
        color,
    );
}

fn draw_message_panel(
    painter: &mut Canvas,
    font: &Rc<CachedFont>,
    bounds: Rect,
    preferred_width: u32,
    title: &str,
    detail: Option<&str>,
    scale: u32,
) {
    let panel_width = preferred_width.min(bounds.width);
    let panel_height = 92 * scale;
    let x = bounds.x + (bounds.width - panel_width) / 2;
    painter.fill_rounded_rect(
        x,
        bounds.y,
        panel_width,
        panel_height,
        7 * scale,
        if detail.is_some() {
            css_rgba(18, 18, 28, 238)
        } else {
            css_rgba(12, 14, 18, 238)
        },
    );
    painter.draw_centered_text(
        font,
        TextBox::new(
            x,
            bounds.y + (if detail.is_some() { 16 } else { 25 }) * scale,
            panel_width,
        ),
        if detail.is_some() { 16.0 } else { 18.0 } * scale as f32,
        title,
        [255, 255, 255, 255],
    );
    if let Some(detail) = detail {
        painter.draw_centered_text(
            font,
            TextBox::new(x, bounds.y + 48 * scale, panel_width),
            13.0 * scale as f32,
            detail,
            [175, 175, 175, 255],
        );
    }
}

struct PrimaryPrice<'a> {
    icon: &'a Rc<RasterImage>,
    bounds: Rect,
    scale: u32,
    icon_size: u32,
    price: Option<u64>,
    color: [u8; 4],
}

fn draw_primary_price(painter: &mut Canvas, font: &Rc<CachedFont>, view: PrimaryPrice<'_>) {
    let value = view
        .price
        .map_or_else(|| "--".to_owned(), |value| value.to_string());
    let size = 19.0 * view.scale as f32;
    let icon_size = view.icon_size * view.scale;
    painter.draw_text_vertically_centered(
        font,
        TextLine::new(view.bounds.x, view.bounds.y, view.bounds.height),
        size,
        &value,
        view.color,
    );
    painter.draw_image_contained(
        view.icon,
        view.bounds.x + view.bounds.width.saturating_sub(icon_size),
        view.bounds.y + view.bounds.height.saturating_sub(icon_size) / 2,
        icon_size,
        icon_size,
    );
}

fn draw_reward_ownership(
    painter: &mut Canvas,
    font: &Rc<CachedFont>,
    reward: &crate::relic::Reward,
    bounds: Rect,
    scale: u32,
) {
    let status = match reward.crafted {
        Some(true) => "Crafted",
        Some(false) => "Not crafted",
        None => "-",
    };
    let status_background = match reward.crafted {
        Some(true) => css_rgba(51, 82, 66, 255),
        Some(false) => css_rgba(83, 65, 117, 255),
        None => css_rgba(57, 62, 78, 255),
    };
    let status_width = bounds.width * 5 / 7;
    let count_width = bounds.width.saturating_sub(status_width);
    let radius = bounds.height / 2;
    fill_left_rounded(
        painter,
        Rect {
            x: bounds.x,
            y: bounds.y,
            width: status_width,
            height: bounds.height,
        },
        radius,
        status_background,
    );
    fill_right_rounded(
        painter,
        Rect {
            x: bounds.x + status_width,
            y: bounds.y,
            width: count_width,
            height: bounds.height,
        },
        radius,
        css_rgba(23, 29, 45, 255),
    );
    painter.draw_centered_text(
        font,
        TextBox::new(bounds.x, bounds.y + 4 * scale, status_width),
        16.0 * scale as f32,
        status,
        [255, 255, 255, 255],
    );
    let owned = format!("{}/{}", reward.count_owned, reward.total_to_own);
    painter.draw_centered_text(
        font,
        TextBox::new(bounds.x + status_width, bounds.y + 4 * scale, count_width),
        16.0 * scale as f32,
        &owned,
        [255, 255, 255, 255],
    );
}

fn draw_reward_parts(
    painter: &mut Canvas,
    resources: &Resources<'_>,
    reward: &crate::relic::Reward,
    bounds: Rect,
    scale: u32,
) {
    let Resources {
        font,
        platinum_icon,
        ..
    } = resources;
    let parts = reward.parts.iter().take(5).collect::<Vec<_>>();
    if parts.is_empty() {
        return;
    }
    let has_set_price = reward.set_price.is_some_and(|price| price > 0);
    let price_height = if has_set_price { 22 * scale } else { 0 };
    let connector_height = if has_set_price { 16 * scale } else { 0 };
    let tile_size = (42 * scale).min(
        bounds
            .height
            .saturating_sub(connector_height + price_height),
    );
    let gap = 4 * scale;
    let row_width = tile_size
        .saturating_mul(parts.len() as u32)
        .saturating_add(gap.saturating_mul(parts.len().saturating_sub(1) as u32));
    let mut x = bounds.x + bounds.width.saturating_sub(row_width) / 2;
    let group_height = tile_size + connector_height + price_height;
    let y = bounds.y + bounds.height.saturating_sub(group_height) / 2;

    for part in parts {
        let complete = part.owned >= part.required;
        let background = if complete {
            css_rgba(49, 99, 48, 181)
        } else {
            css_rgba(255, 255, 255, 38)
        };
        painter.fill_rounded_rect(x, y, tile_size, tile_size, tile_size / 2, background);
        if let Some(image) = part
            .asset
            .as_ref()
            .and_then(|asset| resources.asset_image(&asset.digest))
        {
            painter.draw_image_contained_circle(
                image,
                x + 2 * scale,
                y + 2 * scale,
                tile_size.saturating_sub(4 * scale),
            );
        }
        if part.current {
            painter.draw_ring(
                x + tile_size / 2,
                y + tile_size / 2,
                tile_size.saturating_sub(2 * scale) / 2,
                2 * scale,
                css_rgba(220, 203, 0, 255),
            );
        }
        let count = part.owned.to_string();
        let count_width = text_width(font, &count, 16.0 * scale as f32).ceil() as u32;
        painter.fill_rounded_rect(
            x + tile_size.saturating_sub(count_width + 8 * scale),
            y + tile_size.saturating_sub(21 * scale),
            count_width + 8 * scale,
            21 * scale,
            5 * scale,
            css_rgba(0, 0, 0, 186),
        );
        painter.draw_text(
            font,
            x + tile_size.saturating_sub(count_width + 4 * scale),
            y + tile_size.saturating_sub(20 * scale),
            16.0 * scale as f32,
            &count,
            [255, 255, 255, 255],
        );
        x += tile_size + gap;
    }

    if let Some(set_price) = reward.set_price.filter(|price| *price > 0) {
        let line = css_rgba(255, 255, 255, 71);
        let bracket_top = y + tile_size + 3 * scale;
        painter.fill_rounded_rect(
            bounds.x + bounds.width.saturating_sub(row_width) / 2,
            bracket_top,
            2 * scale,
            8 * scale,
            0,
            line,
        );
        painter.fill_rounded_rect(
            bounds.x + (bounds.width + row_width).saturating_sub(2 * scale) / 2,
            bracket_top,
            2 * scale,
            8 * scale,
            0,
            line,
        );
        painter.fill_rounded_rect(
            bounds.x + bounds.width.saturating_sub(row_width) / 2,
            bracket_top + 6 * scale,
            row_width,
            2 * scale,
            0,
            line,
        );
        painter.fill_rounded_rect(
            bounds.x + bounds.width / 2,
            bracket_top + 8 * scale,
            2 * scale,
            4 * scale,
            0,
            line,
        );
        let label_size = 17.0 * scale as f32;
        let value = set_price.to_string();
        let value_width = text_width(font, &value, label_size).ceil() as u32;
        let icon_size = 22 * scale;
        let total_width = value_width + 3 * scale + icon_size;
        let price_x = bounds.x + bounds.width.saturating_sub(total_width) / 2;
        let price_y = y + tile_size + connector_height;
        painter.draw_text_vertically_centered(
            font,
            TextLine::new(price_x, price_y, price_height),
            label_size,
            &value,
            [255, 255, 255, 255],
        );
        painter.draw_image_contained(
            *platinum_icon,
            price_x + value_width + 3 * scale,
            price_y + price_height.saturating_sub(icon_size) / 2,
            icon_size,
            icon_size,
        );
    }
}

fn draw_account_currency(
    painter: &mut Canvas,
    resources: &Resources<'_>,
    bounds: Rect,
    scale: u32,
    account: &crate::relic::Account,
    interaction_active: bool,
) {
    let Resources {
        font,
        platinum_icon,
        ducat_icon,
        ..
    } = resources;
    let brand = "wfcompanion";
    painter.draw_text_vertically_centered(
        font,
        TextLine::new(bounds.x + 16 * scale, bounds.y, bounds.height),
        18.0 * scale as f32,
        brand,
        [255, 255, 255, 255],
    );
    let hint = if interaction_active {
        "Press Ctrl + Tab or click outside to return to the game"
    } else {
        "Press Ctrl + Tab to interact with the overlay"
    };
    let hint_width = text_width(font, hint, 18.0 * scale as f32).ceil() as u32 + 24 * scale;
    let hint_x = bounds.x + bounds.width.saturating_sub(hint_width) / 2;
    painter.fill_rounded_rect(
        hint_x,
        bounds.y + bounds.height.saturating_sub(27 * scale) / 2,
        hint_width,
        27 * scale,
        14 * scale,
        css_rgba(23, 30, 48, 255),
    );
    painter.draw_centered_text(
        font,
        TextBox::new(hint_x, bounds.y + 14 * scale, hint_width),
        18.0 * scale as f32,
        hint,
        [255, 255, 255, 255],
    );

    let platinum = account
        .platinum
        .map_or_else(|| "--".to_owned(), |value| value.to_string());
    let ducats = account
        .ducats
        .map_or_else(|| "--".to_owned(), |value| value.to_string());
    let platinum_width =
        text_width(font, &platinum, 18.0 * scale as f32).ceil() as u32 + 31 * scale;
    let ducat_width = text_width(font, &ducats, 18.0 * scale as f32).ceil() as u32 + 31 * scale;
    let right = bounds.x + bounds.width.saturating_sub(10 * scale);
    draw_primary_price(
        painter,
        font,
        PrimaryPrice {
            icon: platinum_icon,
            bounds: Rect {
                x: right.saturating_sub(ducat_width + 12 * scale + platinum_width),
                y: bounds.y,
                width: platinum_width,
                height: bounds.height,
            },
            scale,
            icon_size: 22,
            price: account.platinum,
            color: [255, 255, 255, 255],
        },
    );
    draw_primary_price(
        painter,
        font,
        PrimaryPrice {
            icon: ducat_icon,
            bounds: Rect {
                x: right.saturating_sub(ducat_width),
                y: bounds.y,
                width: ducat_width,
                height: bounds.height,
            },
            scale,
            icon_size: 23,
            price: account.ducats,
            color: [255, 255, 255, 255],
        },
    );
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut value: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    value.push_str("...");
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_ui::{composition::Surface, painter::Painter};

    fn render(recording: Recording, pixels: &mut [u8]) {
        let mut painter = Painter::new(pixels, 120, 80).unwrap();
        let repair = painter.visible_bounds();
        Surface::default()
            .draw(recording, &mut painter, repair, |_, _| {})
            .unwrap();
        painter.finish().unwrap();
    }

    #[test]
    fn loading_layers_stay_inside_relic_holder() {
        let mut canvas = vec![0; 120 * 80 * 4];
        let bounds = Rect {
            x: 20,
            y: 10,
            width: 80,
            height: 60,
        };
        let mut painter = Recording::new((120, 80));
        draw_loading_background(&mut painter, bounds, 1);
        draw_loading_pulse(
            &mut painter,
            loading_pulse_bounds(bounds, 1),
            1,
            Duration::from_millis(750),
        );
        render(painter, &mut canvas);

        assert_eq!(pixel(&canvas, 120, 19, 40), [0, 0, 0, 0]);
        assert!(pixel(&canvas, 120, 20, 40)[3] > 0);
        assert!(pixel(&canvas, 120, 60, 40)[3] > 0);
        assert_eq!(pixel(&canvas, 120, 100, 40), [0, 0, 0, 0]);
        assert_eq!(pixel(&canvas, 120, 20, 10), [0, 0, 0, 0]);
        assert_eq!(pixel(&canvas, 120, 99, 69), [0, 0, 0, 0]);
    }

    #[test]
    fn loading_damage_only_covers_the_pulse() {
        let holder = Rect {
            x: 20,
            y: 10,
            width: 800,
            height: 200,
        };

        assert_eq!(
            loading_pulse_bounds(holder, 1),
            Rect {
                x: 392,
                y: 82,
                width: 56,
                height: 56,
            }
        );
    }

    #[test]
    fn loading_pulse_never_writes_outside_its_damage() {
        let damage = Rect {
            x: 32,
            y: 12,
            width: 56,
            height: 56,
        };
        for elapsed in [0, 250, 750, 1_250, 1_900] {
            let mut canvas = vec![0; 120 * 80 * 4];
            let mut painter = Recording::new((120, 80));
            draw_loading_pulse(&mut painter, damage, 1, Duration::from_millis(elapsed));
            render(painter, &mut canvas);

            for y in 0..80 {
                for x in 0..120 {
                    let outside = x < damage.x
                        || x >= damage.x + damage.width
                        || y < damage.y
                        || y >= damage.y + damage.height;
                    if outside {
                        assert_eq!(pixel(&canvas, 120, x, y), [0; 4]);
                    }
                }
            }
        }
    }

    #[test]
    fn relic_footer_keeps_both_bottom_corners_inside_shell() {
        let mut canvas = vec![0; 120 * 80 * 4];
        let color = css_rgba(23, 30, 48, 255);
        let mut painter = Recording::new((120, 80));
        fill_bottom_rounded(
            &mut painter,
            Rect {
                x: 20,
                y: 20,
                width: 80,
                height: 40,
            },
            10,
            color,
        );
        render(painter, &mut canvas);

        assert_eq!(pixel(&canvas, 120, 20, 20), color);
        assert_eq!(pixel(&canvas, 120, 99, 20), color);
        assert!(pixel(&canvas, 120, 20, 59)[3] < 32);
        assert!(pixel(&canvas, 120, 99, 59)[3] < 32);
        assert_eq!(pixel(&canvas, 120, 60, 59), color);
    }

    fn pixel(canvas: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        canvas[offset..offset + 4].try_into().unwrap()
    }
}
