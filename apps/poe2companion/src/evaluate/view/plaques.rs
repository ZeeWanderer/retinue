use super::*;

pub(super) enum Plaque {
    Value,
    Error,
}

pub(super) fn plaque(p: &mut Canvas, rect: Rect, kind: Plaque, s: f32) {
    let (border, background) = match kind {
        Plaque::Value => (rgba(49, 48, 42, 255), rgba(22, 21, 18, 255)),
        // The 80% source fill composites over the window, not over its border.
        Plaque::Error => (rgba(115, 113, 94, 255), rgba(39, 38, 34, 255)),
    };
    surface(p, rect, px(4, s), border, background, u(s));
}

pub(super) fn estimate_height(model: &Model) -> f32 {
    if model.predicting {
        64.0
    } else if model
        .estimate
        .as_ref()
        .is_some_and(|e| e.uncertainty.is_some())
    {
        63.0
    } else if model.estimate.is_some() {
        49.0
    } else {
        0.0
    }
}

fn reliability(confidence: f64) -> (&'static str, [u8; 4]) {
    let label = if confidence >= 90.0 {
        "Very High"
    } else if confidence >= 70.0 {
        "High"
    } else if confidence >= 50.0 {
        "Moderate"
    } else if confidence >= 30.0 {
        "Low"
    } else {
        "Very Low"
    };
    let color = if confidence >= 70.0 {
        rgba(47, 208, 59, 255)
    } else if confidence >= 50.0 {
        rgba(203, 201, 191, 255)
    } else if confidence >= 30.0 {
        rgba(253, 121, 7, 255)
    } else {
        rgba(232, 13, 13, 255)
    };
    (label, color)
}

impl View {
    pub(super) fn paint_estimate(&mut self, p: &mut Canvas, rect: Rect, model: &Model, s: f32) {
        plaque(p, rect, Plaque::Value, s);
        let title_width = text_width(&self.ui, "Estimated Value", 12.0 * s).ceil() as u32;
        let title = Rect {
            x: rect.x + (rect.width - title_width - px(16, s)) / 2,
            y: rect.y + px(7, s),
            width: title_width,
            height: px(12, s),
        };
        p.draw_text_vertically_centered(
            &self.ui,
            TextLine::new(title.x, title.y, title.height),
            12.0 * s,
            "Estimated Value",
            MUTED,
        );
        let info = Rect {
            x: title.x + title.width + px(4, s),
            width: px(12, s),
            ..title
        };
        self.artwork.icon(p, info, "estimate-help", false);
        self.hits.push(Hit {
            target: Target::EstimateInfo,
            rect: info,
        });
        let value = Rect {
            y: rect.y + px(23, s),
            height: px(if model.predicting { 20 } else { 19 }, s),
            ..rect
        };
        self.hits.push(Hit {
            target: Target::EstimateAmount,
            rect: value,
        });
        let detail = Rect {
            y: rect.y + px(if model.predicting { 46 } else { 44 }, s),
            height: px(12, s),
            ..rect
        };
        if model.predicting {
            p.fill_rounded_rect(
                value.x + (value.width - px(60, s)) / 2,
                value.y + px(4, s),
                px(60, s),
                px(12, s),
                px(3, s),
                rgba(49, 48, 42, 255),
            );
            let range_width = text_width(&self.ui, "Range:", 10.0 * s);
            let reliability_width = text_width(&self.ui, "Reliability:", 10.0 * s);
            let mut x = detail.x as f32
                + (detail.width as f32 - range_width - reliability_width - 72.0 * s) / 2.0;
            for (label, width, placeholder) in [
                ("Range:", range_width, 40),
                ("Reliability:", reliability_width, 20),
            ] {
                p.draw_text_vertically_centered(
                    &self.ui,
                    TextLine::new(x.round() as u32, detail.y, detail.height),
                    10.0 * s,
                    label,
                    MUTED,
                );
                x += width + 2.0 * s;
                p.fill_rounded_rect(
                    x.round() as u32,
                    detail.y + px(2, s),
                    px(placeholder, s),
                    px(7, s),
                    px(3, s),
                    rgba(49, 48, 42, 255),
                );
                x += (placeholder as f32 + 8.0) * s;
            }
            return;
        }
        let Some(estimate) = &model.estimate else {
            return;
        };
        let amount = number((estimate.amount * 100.0).round() / 100.0);
        if self.price_icon(&estimate.currency, s) {
            let amount_width = text_width(&self.ui_bold, &amount, 16.0 * s).ceil() as u32;
            let width = amount_width + px(37, s);
            let x = value.x + value.width.saturating_sub(width) / 2;
            self.artwork.icon(
                p,
                Rect {
                    x,
                    y: value.y + px(1, s),
                    width: px(9, s),
                    height: px(16, s),
                },
                "estimate-approximation",
                false,
            );
            p.draw_text_vertically_centered(
                &self.ui_bold,
                TextLine::new(x + px(13, s), value.y, value.height),
                16.0 * s,
                &amount,
                INK,
            );
            self.currency_icon(
                p,
                &estimate.currency,
                Rect {
                    x: x + px(17, s) + amount_width,
                    y: value.y,
                    width: px(20, s),
                    height: px(20, s),
                },
                s,
            );
        } else {
            self.plaque_line(
                p,
                value,
                &[
                    ("\u{2248}", 16.0, rgba(203, 201, 191, 255), true),
                    (&amount, 16.0, INK, true),
                    (&estimate.currency, 16.0, rgba(203, 201, 191, 255), false),
                ],
                4.0,
                s,
            );
        }
        if let Some(range) = &estimate.uncertainty {
            let (label, color) = reliability(range.confidence);
            let bounds = format!(
                "{}\u{2013}{}{}",
                number((range.low * 100.0).round() / 100.0),
                number((range.high * 100.0).round() / 100.0),
                crate::evaluate::currency::abbreviation(&estimate.currency)
            );
            self.plaque_line(
                p,
                detail,
                &[
                    ("Range:", 10.0, MUTED, false),
                    (&bounds, 10.0, rgba(203, 201, 191, 255), true),
                    ("  Reliability:", 10.0, MUTED, false),
                    (label, 10.0, color, true),
                ],
                2.0,
                s,
            );
        }
    }

    fn plaque_line(
        &self,
        p: &mut Canvas,
        rect: Rect,
        spans: &[(&str, f32, [u8; 4], bool)],
        gap: f32,
        s: f32,
    ) {
        let font = |bold| if bold { &self.ui_bold } else { &self.ui };
        let advance = |text: &str, size, bold| {
            if text == "\u{2248}" {
                // The reference's Lato lacks this glyph and uses its sans-serif fallback.
                size * s * (1124.0 / 2048.0)
            } else {
                text_width(font(bold), text, size * s)
            }
        };
        let width = spans
            .iter()
            .map(|(text, size, _, bold)| advance(text, *size, *bold))
            .sum::<f32>()
            + gap * s * spans.len().saturating_sub(1) as f32;
        let mut x = rect.x as f32 + (rect.width as f32 - width).max(0.0) / 2.0;
        for (text, size, color, bold) in spans {
            let width = advance(text, *size, *bold);
            if *text == "\u{2248}" {
                let height = (size * s).round() as u32;
                self.artwork.icon(
                    p,
                    Rect {
                        x: x.round() as u32,
                        y: rect.y + rect.height.saturating_sub(height) / 2,
                        width: width.ceil() as u32,
                        height,
                    },
                    "estimate-approximation",
                    false,
                );
            } else {
                p.draw_text_vertically_centered(
                    font(*bold),
                    TextLine::new(x.round() as u32, rect.y, rect.height),
                    size * s,
                    text,
                    *color,
                );
            }
            x += width + gap * s;
        }
    }
}
