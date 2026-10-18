use fontdue::Font;

use super::{Resources, View};
use crate::ui::{HitRegion, HitTarget, ScreenOutput};
use retinue_overlay::painter::{
    Painter, RasterImage, TextBox, TextLine, css_rgba, fit_text_size, text_width,
};

mod layout;

pub(super) fn draw(
    painter: &mut Painter<'_>,
    resources: &Resources<'_>,
    suggestions: &crate::relic::Suggestions,
    view: View,
) -> ScreenOutput {
    let Resources {
        font,
        platinum_icon,
        ducat_icon,
        icons,
        ..
    } = resources;
    let View {
        scale,
        suggestion_offset,
        interaction_active,
        close_hovered,
    } = view;
    let layout = layout::compute(
        painter.width() / scale,
        painter.height() / scale,
        suggestions.items.len(),
    )
    .scaled(scale);
    let shell = layout.shell;
    painter.fill_rounded_rect(
        shell.x,
        shell.y,
        shell.width,
        shell.height,
        20 * scale,
        css_rgba(255, 255, 255, 158),
    );
    let inner = shell.inset(2 * scale);
    painter.fill_rounded_rect(
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        18 * scale,
        css_rgba(23, 30, 48, 255),
    );
    painter.fill_rounded_rect(
        layout.header.x,
        layout.header.y,
        layout.header.width,
        layout.header.height,
        18 * scale,
        css_rgba(32, 40, 62, 255),
    );
    painter.fill_rounded_rect(
        layout.header.x,
        layout.header.y + 18 * scale,
        layout.header.width,
        layout.header.height.saturating_sub(18 * scale),
        0,
        css_rgba(32, 40, 62, 255),
    );
    painter.draw_centered_text(
        font,
        TextBox::new(
            layout.header.x,
            layout.header.y + 8 * scale,
            layout.header.width,
        ),
        21.0 * scale as f32,
        "Recommended relics",
        [255, 255, 255, 255],
    );
    painter.draw_image_contained(
        &icons.trace,
        layout.header.x + 14 * scale,
        layout.header.y + 10 * scale,
        24 * scale,
        24 * scale,
    );
    painter.draw_text_vertically_centered(
        font,
        TextLine::new(
            layout.header.x + 41 * scale,
            layout.header.y,
            layout.header.height,
        ),
        16.0 * scale as f32,
        &suggestions.trace_count.to_string(),
        [255, 255, 255, 255],
    );
    if close_hovered {
        painter.fill_rounded_rect(
            layout.close.x,
            layout.close.y,
            layout.close.width,
            layout.close.height,
            6 * scale,
            css_rgba(139, 171, 204, 107),
        );
    }
    painter.draw_image_contained(
        &icons.close,
        layout.close.x + 5 * scale,
        layout.close.y + 5 * scale,
        18 * scale,
        18 * scale,
    );

    let grid = layout.grid;
    painter.fill_rounded_rect(
        grid.x,
        grid.y,
        grid.width,
        grid.height,
        0,
        css_rgba(16, 22, 35, 255),
    );

    for (suggestion, card) in suggestions
        .items
        .iter()
        .skip(suggestion_offset)
        .take(4)
        .zip(layout.cards)
    {
        if suggestion.favorite {
            painter.fill_rounded_rect(
                card.x,
                card.y,
                card.width,
                card.height,
                15 * scale,
                css_rgba(252, 255, 69, 214),
            );
        }
        let outline = if suggestion.favorite { 2 * scale } else { 0 };
        painter.fill_rounded_rect(
            card.x + outline,
            card.y + outline,
            card.width.saturating_sub(outline * 2),
            card.height.saturating_sub(outline * 2),
            15 * scale,
            css_rgba(32, 40, 62, 255),
        );
        let count = format!("{}x", suggestion.amount_owned);
        let count_width = text_width(font, &count, 17.0 * scale as f32).ceil() as u32;
        let title_size = fit_text_size(
            font,
            &suggestion.name,
            19.0 * scale as f32,
            13.0 * scale as f32,
            card.width.saturating_sub(70 * scale),
        );
        let title_width = text_width(font, &suggestion.name, title_size).ceil() as u32;
        let vaulted_width = if suggestion.vaulted { 25 * scale } else { 0 };
        let gaps = if suggestion.vaulted {
            16 * scale
        } else {
            8 * scale
        };
        let top_width = vaulted_width + count_width + title_width + gaps;
        let mut top_x = card.x + card.width.saturating_sub(top_width) / 2;
        let top_y = card.y + 6 * scale;
        if suggestion.vaulted {
            super::draw_vaulted_icon(
                painter,
                &icons.vaulted,
                top_x,
                top_y,
                25 * scale,
                24 * scale,
                130,
            );
            top_x += 33 * scale;
        }
        painter.draw_text_vertically_centered(
            font,
            TextLine::new(top_x, top_y, 24 * scale),
            17.0 * scale as f32,
            &count,
            [255, 255, 255, 255],
        );
        top_x += count_width + 8 * scale;
        painter.draw_text_vertically_centered(
            font,
            TextLine::new(top_x, top_y, 24 * scale),
            title_size,
            &suggestion.name,
            [255, 255, 255, 255],
        );

        let price_size = 17.0 * scale as f32;
        let label_width = text_width(font, "E. profits:", price_size).ceil() as u32;
        let platinum = suggestion
            .expected_platinum
            .map_or_else(|| "--".to_owned(), |value| value.to_string());
        let ducats = suggestion.expected_ducats.to_string();
        let platinum_width = text_width(font, &platinum, price_size).ceil() as u32 + 23 * scale;
        let ducat_width = text_width(font, &ducats, price_size).ceil() as u32 + 23 * scale;
        let row_width = label_width + 10 * scale + platinum_width + 14 * scale + ducat_width;
        let row_x = card.x + card.width.saturating_sub(row_width) / 2;
        let row_y = card.y + card.height.saturating_sub(27 * scale);
        painter.draw_text_vertically_centered(
            font,
            TextLine::new(row_x, row_y, 20 * scale),
            price_size,
            "E. profits:",
            [255, 255, 255, 255],
        );
        draw_suggestion_price(
            painter,
            font,
            platinum_icon,
            row_x + label_width + 10 * scale,
            row_y,
            scale,
            &platinum,
        );
        draw_suggestion_price(
            painter,
            font,
            ducat_icon,
            row_x + label_width + 10 * scale + platinum_width + 14 * scale,
            row_y,
            scale,
            &ducats,
        );
    }
    draw_scrollbar(
        painter,
        grid,
        scale,
        suggestions.items.len(),
        suggestion_offset,
    );
    if suggestions.items.is_empty() {
        painter.draw_centered_text(
            font,
            TextBox::new(grid.x, grid.y + grid.height / 2 - 8 * scale, grid.width),
            16.0 * scale as f32,
            "No owned relics found for this era",
            [255, 255, 255, 255],
        );
    }
    painter.draw_centered_text(
        font,
        TextBox::new(
            layout.footer.x + 8 * scale,
            layout.footer.y + 6 * scale,
            layout.footer.width.saturating_sub(16 * scale),
        ),
        16.0 * scale as f32,
        if interaction_active {
            "Press Ctrl + Tab or click outside to return to the game"
        } else {
            "Press Ctrl + Tab to interact with the overlay"
        },
        [255, 255, 255, 255],
    );
    ScreenOutput {
        animation_bounds: None,
        hit_regions: vec![
            HitRegion {
                target: HitTarget::Content,
                bounds: layout.shell,
            },
            HitRegion {
                target: HitTarget::Close,
                bounds: layout.close,
            },
            HitRegion {
                target: HitTarget::Scroll,
                bounds: layout.grid,
            },
        ],
    }
}

fn draw_scrollbar(
    painter: &mut Painter<'_>,
    grid: crate::ui::Rect,
    scale: u32,
    item_count: usize,
    suggestion_offset: usize,
) {
    let rows = item_count.div_ceil(2);
    if rows <= 2 {
        return;
    }

    let track_height = grid.height.saturating_sub(10 * scale);
    let track_x = grid.x + grid.width.saturating_sub(5 * scale);
    let track_y = grid.y + 5 * scale;
    painter.fill_rounded_rect(
        track_x,
        track_y,
        3 * scale,
        track_height,
        2 * scale,
        css_rgba(88, 94, 107, 180),
    );

    let thumb_height = ((track_height as usize * 2) / rows)
        .max((12 * scale) as usize)
        .min(track_height as usize) as u32;
    let max_row = rows - 2;
    let row = (suggestion_offset / 2).min(max_row);
    let thumb_y = track_y
        + track_height
            .saturating_sub(thumb_height)
            .saturating_mul(row as u32)
            / max_row as u32;
    painter.fill_rounded_rect(
        track_x,
        thumb_y,
        3 * scale,
        thumb_height,
        2 * scale,
        css_rgba(255, 255, 255, 230),
    );
}

fn draw_suggestion_price(
    painter: &mut Painter<'_>,
    font: &Font,
    icon: &RasterImage,
    x: u32,
    y: u32,
    scale: u32,
    value: &str,
) {
    let size = 17.0 * scale as f32;
    let value_width = text_width(font, value, size).ceil() as u32;
    painter.draw_text_vertically_centered(
        font,
        TextLine::new(x, y, 20 * scale),
        size,
        value,
        [255, 255, 255, 255],
    );
    painter.draw_image_contained(icon, x + value_width + 3 * scale, y, 20 * scale, 20 * scale);
}
