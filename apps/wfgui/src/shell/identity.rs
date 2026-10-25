use super::*;

impl Shell {
    pub(super) fn identity_bounds(&self) -> Rect {
        let openness =
            ((self.geometry.left.width as f32 / self.scale() - 52.0) / 90.0).clamp(0.0, 1.0);
        Rect {
            x: self.geometry.left.x + self.px(3.0),
            y: self.geometry.left.y + self.px(8.0),
            width: self.geometry.left.width.saturating_sub(self.px(6.0)),
            height: self.px(49.0 + 31.0 * openness),
        }
    }

    pub(super) fn paint_identity(&self, p: &mut Painter<'_>) {
        let bounds = self.identity_bounds();
        let bounds = Rect {
            x: bounds.x - self.geometry.left.x,
            y: bounds.y - self.geometry.left.y,
            ..bounds
        };
        let openness =
            ((self.geometry.left.width as f32 / self.scale() - 52.0) / 90.0).clamp(0.0, 1.0);
        let emblem_size = self.px(36.0 + 14.0 * openness);
        let emblem = Rect {
            x: bounds.x + (bounds.width - emblem_size) / 2,
            y: bounds.y + self.px(5.0),
            width: emblem_size,
            height: emblem_size,
        };
        let panel = Rect {
            height: self.px(49.0 + 31.0 * openness),
            ..bounds
        };
        fill(
            p,
            panel,
            self.px(8.0),
            if self.hover == Some(Control::Player) {
                rgb(0x232b42)
            } else {
                rgb(0x171e30)
            },
        );
        if let Some(image) = self.foundry.rank_image() {
            p.draw_image_contained(image, emblem.x, emblem.y, emblem.width, emblem.height);
        } else {
            self.icon(p, Icon::Mastery, emblem);
        }
        let profile = &self.foundry.snapshot.profile;
        if let Some(rank) = profile["player_level"].as_u64() {
            let text = rank.to_string();
            let font = 13.0 + 4.0 * openness;
            let width = text_width(&self.settings.regular, &text, font * self.scale()).round()
                as u32
                + self.px(8.0);
            let badge = Rect {
                x: emblem.x + emblem.width - width - self.px(2.0),
                y: emblem.y + emblem.height - self.px(22.0),
                width,
                height: self.px(22.0),
            };
            fill(p, badge, self.px(6.0), [0, 0, 0, 199]);
            self.text(
                p,
                &text,
                badge.x + self.px(4.0),
                badge.y,
                badge.height,
                font,
                TEXT,
            );
        }
        if openness > 0.0 {
            let name = profile["player_name"].as_str().unwrap_or("Player");
            let mut label = name.to_owned();
            let limit = bounds.width.saturating_sub(self.px(8.0));
            if text_width(&self.settings.regular, &label, 13.0 * self.scale()) > limit as f32 {
                while !label.is_empty()
                    && text_width(
                        &self.settings.regular,
                        &format!("{label}..."),
                        13.0 * self.scale(),
                    ) > limit as f32
                {
                    label.pop();
                }
                label.push_str("...");
            }
            let width =
                text_width(&self.settings.regular, &label, 13.0 * self.scale()).round() as u32;
            p.viewport(panel, (0, 0), |p| {
                self.text(
                    p,
                    &label,
                    bounds.x + bounds.width.saturating_sub(width) / 2,
                    emblem.y + emblem.height + self.px(2.0),
                    self.px(20.0),
                    13.0,
                    [TEXT[0], TEXT[1], TEXT[2], (openness * 255.0).round() as u8],
                )
            });
        }
        if self
            .focus_visible
            .shows(self.focus == Some(Focus::Chrome(Control::Player)), false)
        {
            focus_ring(p, panel, self.scale());
        }
    }
}
