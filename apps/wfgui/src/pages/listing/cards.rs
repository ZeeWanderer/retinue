use super::*;
use crate::settings::rgb;

impl Pages {
    pub(super) fn inventory_card(&mut self, index: usize, row: &Value, r: Rect) {
        use widgets::Weight;
        let s = self.ui.scale;
        let (x, y, w, h) = (
            r.x as f32 / s,
            r.y as f32 / s,
            r.width as f32 / s,
            r.height as f32 / s,
        );
        let quantity = number(row, "quantity");
        let is_set = text(row, "group") == "sets";
        self.ui.fill(r, 20.0, card_color(&self.ui, index));
        let image = self.ui.rect(x + 10.0, y + 10.0, 86.0, h - 44.0);
        self.ui.first_region("inventory.card.image", image);
        self.ui.image(image, asset(&row["asset"]).as_ref());
        if number(row, "ducats") > 0.0 {
            let value = format!("{:.0}", number(row, "ducats"));
            let width = self.ui.measure(&value, 16.0, false);
            let left = x + 8.0 + (90.0 - width - 26.0) / 2.0;
            self.ui.aligned(
                self.ui.rect(left, y + h - 32.0, width + 1.0, 24.0),
                value,
                16.0,
                TEXT,
                Weight::Medium,
                0.0,
            );
            self.ui.icon(
                self.ui.rect(left + width + 4.0, y + h - 31.0, 22.0, 22.0),
                "ducats",
            );
        }
        let right = x + 103.0;
        let rw = w - 111.0;
        let title = self.ui.rect(right, y + 8.0, rw, 27.0);
        self.ui.first_region("inventory.card.title", title);
        self.ui
            .aligned(title, text(row, "name"), 17.0, TEXT, Weight::Medium, 0.5);
        self.ui.hit(
            format!("list:item:{index}:menu"),
            title,
            text(row, "name"),
            true,
        );
        let quote = self
            .quotes
            .get(text(row, "market_name"))
            .cloned()
            .unwrap_or_default();
        let action_width = if is_set {
            114.0_f32.min(rw / 3.0)
        } else {
            (rw - 11.0) / 2.0
        };
        let status_x = if is_set {
            right + rw - action_width
        } else {
            right
        };
        let status_w = if is_set { action_width } else { rw };
        let status_y = y + 37.0;
        let status_h = h - 80.0;
        let amount = format!("x{quantity:.0}");
        let aw = self.ui.measure(&amount, 14.0, false);
        let vaulted = row["vaulted"].is_boolean()
            && (row["is_prime"] == true || text(row, "group") == "relics" || is_set);
        let mastered = row["mastered"] == true;
        let label = if row["vaulted"] == true {
            "Vaulted"
        } else {
            "Unvaulted"
        };
        let vw = if vaulted {
            24.0 + if is_set {
                0.0
            } else {
                self.ui.measure(label, 14.0, false) + 4.0
            }
        } else {
            0.0
        };
        let group = aw + if vaulted { 8.0 + vw } else { 0.0 } + if mastered { 28.0 } else { 0.0 };
        let left = status_x + (status_w - group) / 2.0;
        self.ui.text(
            self.ui.rect(left, status_y, aw + 1.0, status_h),
            amount,
            14.0,
            TEXT,
        );
        if vaulted {
            self.ui.icon(
                self.ui.rect(
                    left + aw + 8.0,
                    status_y + (status_h - 24.0) / 2.0,
                    24.0,
                    24.0,
                ),
                if row["vaulted"] == true {
                    "vaulted"
                } else {
                    "unvaulted"
                },
            );
            if !is_set {
                self.ui.text(
                    self.ui
                        .rect(left + aw + 36.0, status_y, vw - 28.0 + 1.0, status_h),
                    label,
                    14.0,
                    TEXT,
                );
            }
        }
        if mastered {
            self.ui.icon(
                self.ui.rect(
                    left + group - 20.0,
                    status_y + (status_h - 20.0) / 2.0,
                    20.0,
                    20.0,
                ),
                "owned",
            );
        }
        if is_set {
            let parts = entries(&row["components"]);
            let count = parts.len().min(6);
            let columns = count.clamp(1, 3);
            let area = rw - action_width - 6.0;
            let body_h = h - 45.0;
            let gh = if count > 3 { 76.0 } else { 35.0 };
            for (i, part) in parts.iter().take(6).enumerate() {
                let row_count = (count - i / columns * columns).min(columns);
                let group = row_count as f32 * 35.0 + (row_count - 1) as f32 * 6.0;
                let cx = right + (area - group) / 2.0 + (i % columns) as f32 * 41.0;
                let cy = y + 37.0 + (body_h - gh) / 2.0 + (i / columns) as f32 * 41.0;
                let bounds = self.ui.rect(cx, cy, 35.0, 35.0);
                self.ui.first_region("inventory.card.component", bounds);
                component(&mut self.ui, part, bounds, true);
                self.ui.hit(
                    format!("list:item:{index}:component:{i}"),
                    bounds,
                    text(part, "name"),
                    true,
                );
            }
        }
        if text(row, "market_name").is_empty() {
            return;
        }
        let sale = self.ui.rect(
            if is_set { status_x } else { right },
            y + h - 43.0,
            action_width,
            35.0,
        );
        // A missing set can still be priced; it cannot be listed for sale.
        quote_pill(
            &mut self.ui,
            format!("list:item:{index}:buy"),
            sale,
            if is_set { "SELL" } else { "WTS" },
            &price(quote.sell, quote.resolved),
            if quantity > 0.0 {
                crate::style::SELL_QUOTE
            } else {
                crate::style::UNAVAILABLE_QUOTE
            },
        );
        if !is_set {
            let buy = self.ui.rect(
                right + action_width + 11.0,
                y + h - 43.0,
                action_width,
                35.0,
            );
            quote_pill(
                &mut self.ui,
                format!("list:item:{index}:sell"),
                buy,
                "WTB",
                &price(quote.buy, quote.resolved),
                crate::style::BUY_QUOTE,
            );
        }
    }
    pub(super) fn mastery_card(&mut self, index: usize, row: &Value, r: Rect) {
        use widgets::Weight;
        let s = self.ui.scale;
        let (x, y, w, h) = (
            r.x as f32 / s,
            r.y as f32 / s,
            r.width as f32 / s,
            r.height as f32 / s,
        );
        self.ui.fill(r, 12.0, card_color(&self.ui, index));
        let image = self.ui.rect(x + 8.0, y + (h - 72.0) / 2.0, 72.0, 72.0);
        self.ui.first_region("mastery.card.image", image);
        self.ui.image(image, asset(&row["asset"]).as_ref());
        let middle_x = x + 84.0;
        let middle_w = (w - 84.0 - 117.0).max(1.0);
        let title_width = self
            .ui
            .measure(text(row, "name"), 17.0, false)
            .min((middle_w - 25.0).max(1.0));
        let title_x = middle_x + (middle_w - title_width - 25.0) / 2.0;
        let star = self.ui.rect(title_x, y + 14.0, 20.0, 20.0);
        self.ui.icon(
            star,
            if self.preferences.is_favorite(text(row, "id")) {
                "favorite_active"
            } else {
                "favorite"
            },
        );
        self.ui.hit(
            format!("list:item:{index}:favorite"),
            star,
            "Favorite",
            true,
        );
        let title = self.ui.rect(title_x + 25.0, y + 10.0, title_width, 27.0);
        self.ui.first_region("mastery.card.title", title);
        self.ui.text(title, text(row, "name"), 17.0, TEXT);
        self.ui.hit(
            format!("list:item:{index}:menu"),
            title,
            text(row, "name"),
            true,
        );
        let xp = self
            .ui
            .rect(x + 8.0, y + (h + 72.0) / 2.0 - 21.0, 72.0, 21.0);
        self.ui.fill(xp, 5.0, [0, 0, 0, 138]);
        self.ui.aligned(
            xp,
            format!("+{:.0} XP", number(row, "potential_xp")),
            14.0,
            TEXT,
            Weight::Regular,
            0.5,
        );
        if row["owned"] == true {
            self.ui.aligned(
                self.ui.rect(middle_x, y + 41.0, middle_w, 24.0),
                format!(
                    "Level {:.0}/{:.0}",
                    number(row, "rank"),
                    number(row, "max_rank")
                ),
                15.0,
                TEXT,
                Weight::Light,
                0.5,
            );
            let group_width = 24.0 + self.ui.measure("Owned", 14.0, false);
            let right = x + w - 117.0 + (109.0 - group_width) / 2.0;
            self.ui.icon(
                self.ui.rect(right, y + (h - 20.0) / 2.0, 20.0, 20.0),
                "owned",
            );
            self.ui.aligned(
                self.ui
                    .rect(right + 24.0, y + (h - 24.0) / 2.0, group_width - 24.0, 24.0),
                "Owned",
                14.0,
                crate::style::OWNED_TEXT,
                Weight::Light,
                0.0,
            );
        } else {
            let parts = entries(&row["components"]);
            let count = parts.len().min(6);
            let columns = count.clamp(1, 3);
            for (i, part) in parts.iter().take(6).enumerate() {
                let row_count = (count - i / columns * columns).min(columns);
                let width = row_count as f32 * 30.0 + (row_count - 1) as f32 * 4.0;
                let cx = x + w - 117.0 + (109.0 - width) / 2.0 + (i % columns) as f32 * 34.0;
                let cy = y
                    + (h - if count > 3 { 64.0 } else { 30.0 }) / 2.0
                    + (i / columns) as f32 * 34.0;
                let rect = self.ui.rect(cx, cy, 30.0, 30.0);
                self.ui.first_region("mastery.card.component", rect);
                component(&mut self.ui, part, rect, false);
                self.ui.hit(
                    format!("list:item:{index}:component:{i}"),
                    rect,
                    text(part, "full_name"),
                    true,
                );
            }
            let chance = number(row, "relic_probability") * 100.0;
            let chance = if chance == 0.0 {
                "0%".into()
            } else if chance < 1.0 {
                format!("{chance:.2}%")
            } else {
                format!("{chance:.1}%")
            };
            let price = price(cost(row, &self.quotes), self.quote_pending.is_empty());
            let cw = self.ui.measure(&chance, 16.0, false);
            let pw = self.ui.measure(&price, 16.0, false);
            let group = cw + pw + 2.0 * 28.0 + 4.0 + 27.0;
            let left = middle_x + (middle_w - group) / 2.0;
            let cy = y + 41.0;
            self.ui
                .text(self.ui.rect(left, cy, cw + 1.0, 28.0), chance, 16.0, TEXT);
            self.ui
                .icon(self.ui.rect(left + cw + 2.0, cy, 28.0, 28.0), "axi");
            self.ui.text(
                self.ui.rect(left + cw + 57.0, cy, pw + 1.0, 28.0),
                price,
                16.0,
                TEXT,
            );
            self.ui.icon(
                self.ui.rect(left + cw + pw + 59.0, cy, 28.0, 28.0),
                "platinum",
            );
        }
    }
    pub(super) fn relic_card(&mut self, index: usize, row: &Value, r: Rect) {
        use widgets::Weight;
        let s = self.ui.scale;
        let (x, y, w, h) = (
            r.x as f32 / s,
            r.y as f32 / s,
            r.width as f32 / s,
            r.height as f32 / s,
        );
        self.ui.fill(r, 12.0, card_color(&self.ui, index));
        let middle = (w - 12.0 - 80.0 - 127.0 - 20.0).clamp(0.0, 220.0);
        let left = x + (w - 80.0 - middle - 127.0 - 20.0) / 2.0;
        let image = self.ui.rect(left + 4.0, y + 8.0, 72.0, h - 16.0);
        self.ui.first_region("relic.card.image", image);
        self.ui.image(image, asset(&row["asset"]).as_ref());
        self.ui.aligned(
            self.ui.rect(left + 18.0, y + h - 25.0, 44.0, 20.0),
            format!("x{:.0}", number(row, "amount_owned")),
            15.0,
            TEXT,
            Weight::Medium,
            0.5,
        );
        if row["vaulted"] == true {
            self.ui
                .icon(self.ui.rect(left + 2.0, y + 8.0, 23.0, 23.0), "vaulted");
        }
        let refinements = entries(&row["refinements"])
            .iter()
            .filter(|r| !self.relics.owned || number(r, "amount_owned") > 0.0)
            .take(4)
            .collect::<Vec<_>>();
        let rh = if refinements.len() >= 4 { 16.0 } else { 18.0 };
        let font = if refinements.len() >= 4 { 12.0 } else { 13.33 };
        let top = y + (h - 23.0 - refinements.len() as f32 * rh) / 2.0;
        let mid_x = left + 90.0;
        let values = mid_x + middle - 68.0;
        let title = self.ui.rect(mid_x, top, (middle - 74.0).max(1.0), 23.0);
        self.ui.first_region("relic.card.title", title);
        self.ui
            .aligned(title, text(row, "name"), 19.0, TEXT, Weight::Medium, 0.0);
        self.ui.icon(
            self.ui.rect(values + 8.0, top + 2.0, 19.0, 19.0),
            "platinum",
        );
        self.ui
            .icon(self.ui.rect(values + 42.0, top + 2.0, 19.0, 19.0), "ducats");
        for (i, refinement) in refinements.iter().enumerate() {
            let cy = top + 23.0 + i as f32 * rh;
            let label = self.ui.rect(mid_x, cy, (middle - 74.0).max(1.0), rh);
            self.ui.first_region("relic.card.refinement", label);
            self.ui.text(
                label,
                format!(
                    "{:.0}x {}",
                    number(refinement, "amount_owned"),
                    text(refinement, "refinement")
                ),
                font,
                TEXT,
            );
            self.ui.aligned(
                self.ui.rect(values, cy, 34.0, rh),
                price(
                    refinement["expected_platinum"].as_f64(),
                    self.quote_pending.is_empty(),
                ),
                font,
                TEXT,
                Weight::Regular,
                0.5,
            );
            self.ui.aligned(
                self.ui.rect(values + 34.0, cy, 34.0, rh),
                format!("{:.0}", number(refinement, "expected_ducats")),
                font,
                TEXT,
                Weight::Regular,
                0.5,
            );
        }
        let query = self.relics.search.to_lowercase();
        for (i, reward) in entries(&row["rewards"]).iter().take(6).enumerate() {
            let cx = mid_x + middle + 14.0 + (i % 3) as f32 * 41.0;
            let cy = y + (h - 78.0) / 2.0 + (i / 3) as f32 * 41.0;
            let rect = self.ui.rect(cx, cy, 37.0, 37.0);
            self.ui.first_region("relic.card.reward", rect);
            self.ui.component(
                rect,
                asset(&reward["asset"]).as_ref(),
                number(reward, "owned") > 0.0,
            );
            if !query.is_empty() && text(reward, "name").to_lowercase().contains(&query) {
                self.ui.outline(rect, 18.5, crate::style::SEARCH_HIGHLIGHT);
            }
            self.ui.hit(
                format!("list:item:{index}:reward:{i}"),
                rect,
                format!(
                    "{} - {:.2}% - {:.0} ducats",
                    text(reward, "name"),
                    number(reward, "chance"),
                    number(reward, "ducats")
                ),
                !text(reward, "slug").is_empty(),
            );
        }
    }
}

fn card_color(ui: &Ui, index: usize) -> [u8; 4] {
    let hovered = ui
        .hover
        .as_deref()
        .and_then(|key| key.strip_prefix("list:item:"))
        .and_then(|key| key.split(':').next())
        .and_then(|value| value.parse::<usize>().ok());
    if hovered == Some(index) {
        crate::style::CARD_HOVER
    } else {
        widgets::CARD
    }
}

fn quote_pill(ui: &mut Ui, key: String, r: Rect, label: &str, price: &str, color: [u8; 4]) {
    let s = ui.scale;
    let (x, y, w, h) = (
        r.x as f32 / s,
        r.y as f32 / s,
        r.width as f32 / s,
        r.height as f32 / s,
    );
    ui.fill(r, h / 2.0, color);
    ui.text(ui.rect(x + 10.0, y, w - 20.0, h), label, 14.0, TEXT);
    let pw = ui.measure(price, 14.0, false);
    ui.text(
        ui.rect(x + w - 29.0 - pw, y, pw + 1.0, h),
        price,
        14.0,
        TEXT,
    );
    ui.icon(
        ui.rect(x + w - 26.0, y + (h - 16.0) / 2.0, 16.0, 16.0),
        "platinum",
    );
    ui.hit(key, r, format!("View {label} listings"), true);
}

fn component(ui: &mut Ui, part: &Value, r: Rect, show_count: bool) {
    let owned = number(part, "owned");
    let required = number(part, "required");
    ui.component(
        r,
        asset(&part["asset"]).as_ref(),
        number(part, "blueprint_owned") > 0.0 || (owned >= required && required > 0.0),
    );
    if !show_count {
        return;
    }
    let value = compact_count(owned);
    let width =
        retinue_ui::painter::text_width(&ui.regular, &value, 12.5 * ui.scale) + ui.px(8.0) as f32;
    let count = Rect {
        x: (r.x + r.width + ui.px(3.0)).saturating_sub(width.round() as u32),
        y: r.y + r.height + ui.px(2.0) - ui.px(18.0),
        width: width.ceil() as u32 + 1,
        height: ui.px(18.0),
    };
    ui.fill(count, 9.0, rgb(0x141b2a));
    ui.text(
        Rect {
            x: count.x + ui.px(4.0),
            width: count.width.saturating_sub(ui.px(8.0)),
            ..count
        },
        value,
        12.5,
        TEXT,
    );
}

fn compact_count(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 10_000.0 {
        format!("{:.0}K", value / 1_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}
