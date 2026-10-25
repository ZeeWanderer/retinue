use super::*;

impl View {
    pub(super) fn currency_icon(&self, p: &mut Canvas, id: &str, rect: Rect, s: f32) {
        let Some(icon) = self.currencies.get(id).filter(|i| i.size == rect.width) else {
            return;
        };
        p.draw_image(&icon.image, rect.x, rect.y, rect.width, rect.height);
        if let Some(tier) = &icon.source.tier {
            let size = 8.0 * s;
            let width = text_width(&self.ui_bold, tier, size).ceil() as u32;
            let x = rect.x + rect.width.saturating_sub(px(2, s) + width);
            let y = rect.y + rect.height.saturating_sub(px(7, s));
            let stroke = u(s);
            for (dx, dy) in [
                (-1_i32, -1_i32),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ] {
                p.draw_text_vertically_centered(
                    &self.ui_bold,
                    TextLine::new(
                        x.saturating_add_signed(dx * stroke as i32),
                        y.saturating_add_signed(dy * stroke as i32),
                        px(8, s),
                    ),
                    size,
                    tier,
                    rgba(0, 0, 0, 255),
                );
            }
            p.draw_text_vertically_centered(
                &self.ui_bold,
                TextLine::new(x, y, px(8, s)),
                size,
                tier,
                rgba(255, 255, 255, 255),
            );
        }
    }

    pub(super) fn price_icon(&self, id: &str, s: f32) -> bool {
        self.currencies.get(id).is_some_and(|i| i.size == px(20, s))
    }

    pub(super) fn listing_price(
        &self,
        p: &mut Canvas,
        rect: Rect,
        amount: f64,
        id: &str,
        color: [u8; 4],
        s: f32,
    ) {
        let size = px(20, s);
        let icon = Rect {
            x: rect.x + rect.width.saturating_sub(px(12, s) + size),
            y: rect.y + rect.height.saturating_sub(size) / 2,
            width: size,
            height: size,
        };
        let amount = elide(
            &self.ui_semibold,
            &format!("{} \u{00d7}", number(amount)),
            12.0 * s,
            rect.width.saturating_sub(px(48, s)),
        );
        let width = text_width(&self.ui_semibold, &amount, 12.0 * s).ceil() as u32;
        p.draw_text_vertically_centered(
            &self.ui_semibold,
            TextLine::new(icon.x.saturating_sub(width + px(4, s)), rect.y, rect.height),
            12.0 * s,
            &amount,
            color,
        );
        self.currency_icon(p, id, icon, s);
    }
}
