use crate::style;
use retinue_ui::{
    Rect,
    composition::Canvas,
    painter::{CachedFont, TextLine, wrap_text},
};
use std::rc::Rc;

pub struct Layout {
    pub bounds: Rect,
    pub rows: Vec<Rect>,
    title: Vec<String>,
    details: Vec<String>,
    header_height: u32,
    scale: f32,
}

impl Layout {
    pub fn new(
        anchor: Rect,
        viewport: Rect,
        title: &str,
        details: &[String],
        count: usize,
        font: &CachedFont,
        scale: f32,
    ) -> Self {
        let px = |n: f32| (n * scale).round() as u32;
        let width = px(320.0).min(viewport.width);
        let title: Vec<_> = wrap_text(
            font,
            title,
            15.0 * scale,
            width.saturating_sub(px(24.0)) as f32,
        )
        .into_iter()
        .take(3)
        .map(str::to_owned)
        .collect();
        let details: Vec<_> = details
            .iter()
            .flat_map(|s| wrap_text(font, s, 12.0 * scale, width.saturating_sub(px(24.0)) as f32))
            .take(3)
            .map(str::to_owned)
            .collect();
        let header_height =
            px(18.0) + title.len() as u32 * px(21.0) + details.len() as u32 * px(18.0);
        let height = (header_height + count as u32 * px(32.0) + px(8.0)).min(viewport.height);
        let x = anchor
            .x
            .clamp(viewport.x, viewport.x + viewport.width - width);
        let below = anchor.y.saturating_add(anchor.height);
        let y = if below.saturating_add(height) <= viewport.y + viewport.height {
            below
        } else {
            anchor.y.saturating_sub(height)
        }
        .clamp(viewport.y, viewport.y + viewport.height - height);
        let bounds = Rect {
            x,
            y,
            width,
            height,
        };
        let header_height =
            header_height.min(height.saturating_sub(px(24.0) * count as u32 + px(8.0)));
        let row_height = height.saturating_sub(header_height + px(8.0)) / count.max(1) as u32;
        let rows = (0..count)
            .map(|i| Rect {
                x: x + px(5.0),
                y: y + header_height + i as u32 * row_height,
                width: width.saturating_sub(px(10.0)),
                height: row_height,
            })
            .collect();
        Self {
            bounds,
            rows,
            title,
            details,
            header_height,
            scale,
        }
    }

    pub fn paint(
        &self,
        canvas: &mut Canvas,
        font: &Rc<CachedFont>,
        items: &[(&str, bool)],
        hover: Option<usize>,
        focused: Option<usize>,
    ) {
        let px = |n: f32| (n * self.scale).round() as u32;
        frame(canvas, self.bounds, self.scale);
        canvas.clip(self.bounds, |p| {
            let r = self.bounds;
            let mut y = r.y + px(8.0);
            p.clip(
                Rect {
                    height: self.header_height.saturating_sub(px(5.0)),
                    ..r
                },
                |p| {
                    for (lines, size, step, color) in [
                        (&self.title, 15.0, 21.0, style::TEXT),
                        (&self.details, 12.0, 18.0, style::MUTED),
                    ] {
                        for line in lines {
                            p.draw_text_vertically_centered(
                                font,
                                TextLine::new(r.x + px(12.0), y, px(step)),
                                size * self.scale,
                                line,
                                color,
                            );
                            y += px(step);
                        }
                    }
                },
            );
            p.fill(
                Rect {
                    x: r.x + px(8.0),
                    y: r.y + self.header_height.saturating_sub(px(4.0)),
                    width: r.width.saturating_sub(px(16.0)),
                    height: px(1.0).max(1),
                },
                0,
                style::MENU_EDGE,
            );
            for (index, (r, (label, enabled))) in self.rows.iter().zip(items).enumerate() {
                row(
                    p,
                    *r,
                    font,
                    label,
                    *enabled,
                    hover == Some(index),
                    self.scale,
                );
                if focused == Some(index) {
                    style::focus_ring(p, *r, self.scale);
                }
            }
        });
    }
}

pub fn frame(p: &mut Canvas, r: Rect, scale: f32) {
    p.fill(r, (8.0 * scale).round() as u32, style::POPOVER);
    p.outline(
        r,
        (8.0 * scale).round() as u32,
        scale.round().max(1.0) as u32,
        style::MENU_EDGE,
    );
}

pub fn row(
    p: &mut Canvas,
    r: Rect,
    font: &Rc<CachedFont>,
    text: &str,
    enabled: bool,
    selected: bool,
    scale: f32,
) {
    if selected && enabled {
        p.fill(r, (4.0 * scale).round() as u32, style::MENU_HOVER);
    }
    p.clip(r, |p| {
        p.draw_text_vertically_centered(
            font,
            TextLine::new(r.x + (9.0 * scale).round() as u32, r.y, r.height),
            14.0 * scale,
            text,
            if enabled { style::TEXT } else { style::MUTED },
        )
    });
}
