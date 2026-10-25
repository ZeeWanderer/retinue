use super::*;
use crate::evaluate::model::Listing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Eye,
    Price,
    ItemLevel,
    RequiredLevel,
    GemLevel,
    Quality,
    Sockets,
    Account,
    Listed,
}

impl Column {
    fn title(self) -> &'static str {
        match self {
            Self::Eye => "",
            Self::Price => "Price",
            Self::ItemLevel => "iLvL",
            Self::RequiredLevel => "rLvL",
            Self::GemLevel => "LvL",
            Self::Quality => "Q%",
            Self::Sockets => "S+",
            Self::Account => "Account",
            Self::Listed => "Listed",
        }
    }

    fn width(self) -> u32 {
        match self {
            Self::Eye => 24,
            Self::Price => 90,
            Self::ItemLevel | Self::RequiredLevel => 50,
            Self::GemLevel | Self::Sockets => 44,
            Self::Quality => 52,
            Self::Listed => 72,
            Self::Account => 0,
        }
    }

    fn value(self, listing: &Listing, now: u64) -> String {
        let integer = |value: Option<u32>| value.map(|n| n.to_string()).unwrap_or_default();
        match self {
            Self::Eye => String::new(),
            Self::Price => format!(
                "{} \u{00d7} {}",
                number(listing.amount),
                listing.currency.replace('_', " ")
            ),
            Self::ItemLevel => integer(listing.item_level),
            Self::RequiredLevel => integer(listing.required_level),
            Self::GemLevel => integer(listing.gem_level),
            Self::Quality => format!("{}%", listing.quality),
            Self::Sockets => integer(listing.gem_sockets),
            Self::Account => listing.account.clone(),
            Self::Listed => age(listing.indexed_at, now),
        }
    }
}

fn columns(rarity: &str, rect: Rect, scale: f32) -> Vec<(Column, Rect)> {
    let mut columns = vec![Column::Eye, Column::Price];
    columns.extend(match rarity {
        "Gem" => vec![Column::GemLevel, Column::Quality, Column::Sockets],
        "Unique" => vec![Column::RequiredLevel, Column::Quality],
        _ => vec![Column::ItemLevel, Column::Quality],
    });
    columns.extend([Column::Account, Column::Listed]);
    let fixed: u32 = columns.iter().map(|c| c.width()).sum();
    let available = rect.width as f32 / scale;
    let mut offset = 0.0;
    columns
        .into_iter()
        .map(|column| {
            let start = (offset * scale).round() as u32;
            offset += if column == Column::Account {
                (available - fixed as f32).max(0.0)
            } else {
                column.width() as f32
            };
            (
                column,
                Rect {
                    x: rect.x + start,
                    width: (offset * scale).round() as u32 - start,
                    ..rect
                },
            )
        })
        .collect()
}

impl View {
    pub(super) fn paint_results(
        &mut self,
        p: &mut Painter<'_>,
        layout: &UiLayout<Node>,
        model: &Model,
        interaction: &Interaction,
        s: f32,
    ) {
        let Some(results) = &model.results else {
            return;
        };
        let table = layout.bounds(Node::Table);
        fill(p, table, rgba(49, 48, 42, 255));
        let rarity = model
            .evaluation
            .as_ref()
            .map(|e| e.item.rarity.as_str())
            .unwrap_or_default();
        let columns = columns(rarity, table, s);
        for (column, rect) in &columns {
            let text_x = self.table_text(
                p,
                *rect,
                column.title(),
                true,
                *column == Column::Account,
                rgba(203, 201, 191, 255),
                s,
            );
            if *column == Column::Price {
                self.hits.push(Hit {
                    target: Target::Sort,
                    rect: *rect,
                });
                let x = text_x.saturating_sub(px(10, s)) as f64;
                let y = (rect.y + rect.height / 2) as f64;
                let direction = if model.descending { 1.0 } else { -1.0 };
                p.draw_line(
                    (x, y - 5.0 * s as f64),
                    (x, y + 5.0 * s as f64),
                    s as f64,
                    MUTED,
                );
                for dx in [-4.0, 4.0] {
                    p.draw_line(
                        (x + dx * s as f64, y),
                        (x, y + direction * 5.0 * s as f64),
                        s as f64,
                        MUTED,
                    );
                }
            }
        }
        let list = layout.bounds(Node::Listings);
        self.visible_listings = (list.height / px(32, s).max(1)) as usize;
        self.hits.push(Hit {
            target: Target::Results,
            rect: list,
        });
        if results.listings.is_empty() {
            self.label(
                p,
                Rect {
                    y: list.y + px(4, s),
                    height: px(19, s),
                    ..list
                },
                "No results",
                12.0 * s,
                MUTED,
                false,
            );
        }
        let now = self.preview_time.unwrap_or(self.time);
        let scroll = interaction
            .scroll
            .min(results.listings.len().saturating_sub(self.visible_listings));
        for (index, listing) in results
            .listings
            .iter()
            .enumerate()
            .skip(scroll)
            .take(self.visible_listings)
        {
            if let Some(indexed) = listing.indexed_at {
                let seconds = now.saturating_sub(indexed);
                let unit = if seconds >= 86400 {
                    86400
                } else if seconds >= 3600 {
                    3600
                } else if seconds >= 60 {
                    60
                } else {
                    1
                };
                let next = indexed.saturating_add((seconds / unit + 1).saturating_mul(unit));
                self.age_deadline = Some(self.age_deadline.map_or(next, |at| at.min(next)));
            }
            let row = Rect {
                y: list.y + px((index - scroll) as u32 * 32, s),
                height: px(32, s),
                ..list
            };
            fill(p, row, PANEL);
            fill(
                p,
                Rect {
                    y: row.y + row.height - u(s),
                    height: u(s),
                    ..row
                },
                rgba(35, 34, 29, 255),
            );
            let opacity = match listing
                .indexed_at
                .map(|at| now.saturating_sub(at) / 86400)
                .unwrap_or(0)
            {
                0 => 255,
                1..7 => 242,
                7..30 => 230,
                _ => 204,
            };
            for (column, cell) in &columns {
                let rect = Rect {
                    y: row.y,
                    height: row.height,
                    ..*cell
                };
                if *column == Column::Eye {
                    if listing.item.is_some() {
                        let button = Rect {
                            x: rect.x + px(1, s),
                            y: row.y + px(5, s),
                            width: px(22, s),
                            height: px(22, s),
                        };
                        self.hits.push(Hit {
                            target: Target::Listing(index),
                            rect: button,
                        });
                        let hovered = interaction.hover == Some(Target::Listing(index));
                        surface(
                            p,
                            button,
                            px(4, s),
                            BORDER,
                            if hovered {
                                rgba(49, 48, 42, 255)
                            } else {
                                PANEL
                            },
                            u(s),
                        );
                        let extent = px(16, s);
                        self.artwork.icon(
                            p,
                            Rect {
                                x: button.x + (button.width - extent) / 2,
                                y: button.y + (button.height - extent) / 2,
                                width: extent,
                                height: extent,
                            },
                            "listing-eye",
                            hovered,
                        );
                    }
                    continue;
                }
                let mut color = if *column == Column::Price { INK } else { MUTED };
                color[3] = opacity;
                if *column == Column::Price {
                    self.hits.push(Hit {
                        target: Target::ListingCurrency(index),
                        rect,
                    });
                    if self.price_icon(&listing.currency, s) {
                        self.listing_price(p, rect, listing.amount, &listing.currency, color, s);
                        continue;
                    }
                }
                self.table_text(
                    p,
                    rect,
                    &column.value(listing, now),
                    *column == Column::Price,
                    *column == Column::Account,
                    color,
                    s,
                );
            }
        }
        if results.listings.len() > self.visible_listings {
            let track = Rect {
                x: list.x + list.width - px(6, s),
                width: px(6, s),
                ..list
            };
            let height =
                track.height * self.visible_listings as u32 / results.listings.len() as u32;
            fill(p, track, BORDER);
            fill(
                p,
                Rect {
                    y: track.y
                        + (track.height - height) * scroll as u32
                            / (results.listings.len() - self.visible_listings).max(1) as u32,
                    height,
                    ..track
                },
                MUTED,
            );
        }
    }

    fn table_text(
        &self,
        p: &mut Painter<'_>,
        rect: Rect,
        value: &str,
        emphasized: bool,
        leading: bool,
        color: [u8; 4],
        s: f32,
    ) -> u32 {
        let font = if emphasized {
            &self.ui_semibold
        } else {
            &self.ui
        };
        let padding = px(12, s);
        let text = elide(
            font,
            value,
            12.0 * s,
            rect.width.saturating_sub(padding * 2),
        );
        let width = text_width(font, &text, 12.0 * s).ceil() as u32;
        let x = if leading {
            rect.x + padding
        } else {
            rect.x + rect.width.saturating_sub(padding + width)
        };
        p.draw_text_vertically_centered(
            font,
            TextLine::new(x, rect.y, rect.height),
            12.0 * s,
            &text,
            color,
        );
        x
    }

    pub(super) fn paint_item_header(
        &mut self,
        p: &mut Painter<'_>,
        rect: Rect,
        item: &Item,
        s: f32,
    ) {
        let double = !item.name.is_empty() && item.name != item.base;
        self.artwork.header(p, rect, &item.rarity, double);
        let title = if item.name.is_empty() {
            &item.base
        } else {
            &item.name
        };
        let line = Rect {
            x: rect.x + px(if double { 45 } else { 30 }, s),
            y: rect.y + px(5, s),
            width: rect
                .width
                .saturating_sub(px(if double { 90 } else { 60 }, s)),
            height: px(if double { 22 } else { 24 }, s),
        };
        self.item_label(p, line, title, 19.0 * s, rarity_color(&item.rarity));
        if double {
            self.item_label(
                p,
                Rect {
                    y: rect.y + px(27, s),
                    ..line
                },
                &item.base,
                19.0 * s,
                rarity_color(&item.rarity),
            );
        }
    }

    pub(super) fn prepare_artwork(
        &mut self,
        layout: &UiLayout<Node>,
        model: &Model,
        interaction: &Interaction,
        s: f32,
    ) -> Result<(), String> {
        self.artwork.begin_frame();
        self.artwork
            .prepare_controls((px(24, s), px(20, s)), (px(16, s), px(16, s)))?;
        let item = model.evaluation.as_ref().map(|e| &e.item);
        self.artwork.prepare_header(
            layout.bounds(Node::Header),
            item.map_or("Normal", |item| &item.rarity),
            item.is_some_and(|item| !item.name.is_empty() && item.name != item.base),
        )?;
        if let Some(item) = listing_preview_item(model, interaction) {
            let (width, height) = listing_header_size(&item, s);
            self.artwork.prepare_header(
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                &item.rarity,
                !item.name.is_empty() && item.name != item.base,
            )?;
        }
        Ok(())
    }

    pub(super) fn paint_item_row(
        &self,
        p: &mut Painter<'_>,
        rect: Rect,
        row: &Row,
        rarity: &str,
        selected: bool,
        s: f32,
    ) {
        let color = if row.lines.first().is_some_and(|line| line == "Corrupted") {
            rgba(210, 0, 0, 255)
        } else if row.modifier.is_some() || matches!(row.query, Some(Target::Condition(_))) {
            BLUE
        } else {
            ITEM_LABEL
        };
        let start = rect.y
            + ((rect.height as f32 - row.lines.len() as f32 * 20.0 * s) / 2.0).max(0.0) as u32;
        for (n, line) in row.lines.iter().enumerate() {
            let y = start + px(n as u32 * 20, s);
            // Fontin's centered raster baseline lands one device pixel below the browser's.
            let text_y = y.saturating_sub(1);
            if selected {
                let width = (text_width(&self.item, line, ITEM_TEXT_SIZE * s).ceil() as u32
                    + px(8, s))
                .min(rect.width);
                fill(
                    p,
                    Rect {
                        x: rect.x + (rect.width - width) / 2,
                        y: y + u(s),
                        width,
                        height: px(17, s),
                    },
                    rgba(255, 255, 255, 41),
                );
            }
            if let Some((label, value)) = line.split_once(": ")
                && matches!(row.query, Some(Target::Level | Target::Menu(Menu::Rarity)))
            {
                let prefix = format!("{label}: ");
                let x = rect.x
                    + rect.width.saturating_sub(
                        text_width(&self.item, line, ITEM_TEXT_SIZE * s).ceil() as u32,
                    ) / 2;
                p.draw_text_vertically_centered(
                    &self.item,
                    TextLine::new(x, text_y, px(20, s)),
                    ITEM_TEXT_SIZE * s,
                    &prefix,
                    ITEM_LABEL,
                );
                p.draw_text_vertically_centered(
                    &self.item,
                    TextLine::new(
                        x + text_width(&self.item, &prefix, ITEM_TEXT_SIZE * s).round() as u32,
                        text_y,
                        px(20, s),
                    ),
                    ITEM_TEXT_SIZE * s,
                    value,
                    if row.query == Some(Target::Menu(Menu::Rarity)) {
                        rarity_color(rarity)
                    } else {
                        ITEM_VALUE
                    },
                );
            } else {
                self.item_label(
                    p,
                    Rect {
                        x: rect.x + px(16, s),
                        y: text_y,
                        width: rect.width - px(32, s),
                        height: px(20, s),
                    },
                    line,
                    ITEM_TEXT_SIZE * s,
                    color,
                );
            }
        }
    }

    pub(super) fn paint_listing_preview(
        &mut self,
        p: &mut Painter<'_>,
        layout: &UiLayout<Node>,
        model: &Model,
        interaction: &Interaction,
        size: (u32, u32),
        s: f32,
    ) {
        let Some(Target::Listing(index)) = interaction.hover else {
            return;
        };
        let Some(item) = listing_preview_item(model, interaction) else {
            return;
        };
        let rows = self.rows_for_item(&item, s);
        let width = px(CONTENT as u32 + 18, s);
        let (_, header_height) = listing_header_size(&item, s);
        let height = (header_height
            + px(30, s)
            + (rows.iter().map(Row::height).sum::<f32>() * s).ceil() as u32)
            .min(size.1);
        let content = layout.bounds(Node::Content);
        let right = content.x + content.width;
        let x = if right + width <= size.0 {
            right
        } else {
            content.x.saturating_sub(width)
        };
        let rect = Rect {
            x,
            y: (content.y + px(4, s)).min(size.1.saturating_sub(height)),
            width,
            height,
        };
        surface(p, rect, px(4, s), BORDER, rgba(35, 34, 29, 255), u(s));
        let body = rect.inset(px(9, s));
        fill(p, body, PANEL);
        self.paint_item_header(
            p,
            Rect {
                height: header_height,
                ..body
            },
            &item,
            s,
        );
        let mut y = body.y + header_height + px(6, s);
        for row in &rows {
            let h = (row.height() * s).round() as u32;
            if y + h > body.y + body.height - px(6, s) {
                break;
            }
            let mut bounds = Rect {
                x: body.x,
                y,
                width: body.width,
                height: h,
            };
            if row.separator {
                self.artwork.divider(
                    p,
                    Rect {
                        y: bounds.y + u(s),
                        height: px(8, s),
                        ..bounds
                    },
                    &item.rarity,
                    s,
                );
                bounds.y += px(ITEM_SEPARATOR, s);
                bounds.height -= px(ITEM_SEPARATOR, s);
            }
            let selected =
                match row.query {
                    Some(Target::Menu(Menu::Rarity)) => model.rarity.is_some(),
                    Some(Target::Level) => model.level.enabled,
                    Some(Target::Condition(index)) => {
                        model.evaluation.as_ref().is_some_and(|evaluation| {
                            let condition = &item.conditions[index];
                            condition.id.is_some()
                                && evaluation.item.conditions.iter().enumerate().any(
                                    |(i, other)| {
                                        condition.id == other.id
                                            && condition.text == other.text
                                            && model.conditions.get(i).copied().unwrap_or(false)
                                    },
                                )
                        })
                    }
                    _ => false,
                };
            self.paint_item_row(p, bounds, row, &item.rarity, selected, s);
            y += h;
        }
        self.hits.push(Hit {
            target: Target::Listing(index),
            rect,
        });
        let left = self.bounds.x.min(rect.x);
        let top = self.bounds.y.min(rect.y);
        self.bounds = Rect {
            x: left,
            y: top,
            width: (self.bounds.x + self.bounds.width).max(rect.x + rect.width) - left,
            height: (self.bounds.y + self.bounds.height).max(rect.y + rect.height) - top,
        };
    }

    pub(super) fn paint_error(&self, p: &mut Painter<'_>, rect: Rect, message: &str, s: f32) {
        plaques::plaque(p, rect, plaques::Plaque::Error, s);
        let diameter = px(16, s);
        let icon = Rect {
            x: rect.x + px(13, s),
            y: rect.y + (rect.height - diameter) / 2,
            width: diameter,
            height: diameter,
        };
        self.artwork.icon(p, icon, "search-error", false);
        for (index, line) in wrap(
            &self.ui,
            message,
            14.0 * s,
            rect.width.saturating_sub(px(52, s)),
        )
        .iter()
        .enumerate()
        {
            p.draw_text_vertically_centered(
                &self.ui,
                TextLine::new(
                    rect.x + px(39, s),
                    rect.y + px(18, s) + px(index as u32 * 17, s),
                    px(17, s),
                ),
                14.0 * s,
                line,
                rgba(203, 201, 191, 255),
            );
        }
    }
}

fn listing_preview_item(model: &Model, interaction: &Interaction) -> Option<Item> {
    let Target::Listing(index) = interaction.hover? else {
        return None;
    };
    let mut item = model.results.as_ref()?.listings.get(index)?.item.clone()?;
    if item.rarity != "Unique" {
        item.name.clear();
    }
    Some(item)
}

fn listing_header_size(item: &Item, scale: f32) -> (u32, u32) {
    (
        px(CONTENT as u32 + 18, scale).saturating_sub(2 * px(9, scale)),
        px(
            if !item.name.is_empty() && item.name != item.base {
                54
            } else {
                34
            },
            scale,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::model::Results;

    #[test]
    fn selected_text_includes_reference_side_shadows_without_expanding_the_row() {
        let view = View::new().unwrap();
        let row = Row {
            lines: vec!["Modifiable".into()],
            modifier: None,
            separator: false,
            query: Some(Target::Condition(0)),
        };
        for scale in [1.0, 1.2, 1.25, 1.5] {
            let rect = Rect {
                x: 0,
                y: 0,
                width: px(400, scale),
                height: px(20, scale),
            };
            let draw = |selected| {
                let mut pixels = vec![0; (rect.width * rect.height * 4) as usize];
                let mut painter = Painter::new(&mut pixels, rect.width, rect.height).unwrap();
                view.paint_item_row(&mut painter, rect, &row, "Rare", selected, scale);
                painter.finish().unwrap();
                pixels
            };
            let plain = draw(false);
            let selected = draw(true);
            let changed: Vec<_> = selected
                .chunks_exact(4)
                .zip(plain.chunks_exact(4))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| (i as u32 % rect.width, i as u32 / rect.width))
                .collect();
            let left = changed.iter().map(|p| p.0).min().unwrap();
            let right = changed.iter().map(|p| p.0).max().unwrap();
            let top = changed.iter().map(|p| p.1).min().unwrap();
            let bottom = changed.iter().map(|p| p.1).max().unwrap();
            assert_eq!(
                right - left + 1,
                text_width(&view.item, "Modifiable", ITEM_TEXT_SIZE * scale).ceil() as u32
                    + px(8, scale)
            );
            assert_eq!(top, u(scale));
            assert_eq!(bottom - top + 1, px(17, scale));
        }
    }

    #[test]
    fn item_groups_and_profiles_match_reference_vertical_spacing() {
        let view = View::new().unwrap();
        let mut model = fixture();
        model.results = None;
        for scale in [1.0, 1.2, 1.25, 1.5] {
            let rows = view.rows(&model, scale);
            let layout = view
                .layout((2560, 1440), (0, 0), scale, &rows, 0, true, &model)
                .unwrap();
            let search = layout.bounds(Node::Search);
            let profiles = layout.bounds(Node::Profiles);
            assert!((search.y as i32 - px(289, scale) as i32).abs() <= 1);
            assert!((profiles.y as i32 - px(240, scale) as i32).abs() <= 1);
        }
    }

    fn fixture() -> Model {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/evaluate-reference.json"
        ))
        .unwrap();
        let mut model = Model::new("Forbidden Rites".into());
        model.evaluated(
            0,
            serde_json::from_value(data["evaluation"].clone()).unwrap(),
        );
        model.results = Some(serde_json::from_value::<Results>(data["results"].clone()).unwrap());
        model
    }

    #[test]
    fn columns_share_fixed_boundaries_at_fractional_scale() {
        for s in [1.0, 1.2, 1.25, 1.5] {
            let table = Rect {
                x: 73,
                y: 30,
                width: px(400, s),
                height: px(32, s),
            };
            for rarity in ["Rare", "Unique", "Gem"] {
                let columns = columns(rarity, table, s);
                assert_eq!(columns[0].0, Column::Eye);
                assert_eq!(columns[0].1.width, px(24, s));
                assert_eq!(columns[1].1.x, table.x + px(24, s));
                for pair in columns.windows(2) {
                    assert_eq!(pair[0].1.x + pair[0].1.width, pair[1].1.x);
                }
                let last = columns.last().unwrap().1;
                assert_eq!(last.x + last.width, table.x + table.width);
                assert_eq!(
                    columns.iter().any(|(c, _)| *c == Column::Sockets),
                    rarity == "Gem"
                );
                assert_eq!(
                    columns.iter().any(|(c, _)| *c == Column::RequiredLevel),
                    rarity == "Unique"
                );
            }
        }
    }

    #[test]
    fn fetch_states_preserve_table_geometry_and_cooldown_disables_refresh() {
        let mut model = fixture();
        let mut view = View::new().unwrap();
        for scale in [1.0, 1.25] {
            let rows = view.rows(&model, scale);
            let base = view
                .layout((1920, 1080), (100, 32), scale, &rows, 0, true, &model)
                .unwrap();
            for state in ["refreshing", "fetching", "failed", "cooldown", "ready"] {
                model.loading = state == "refreshing";
                model.fetching = state == "fetching";
                model.fetch_error =
                    matches!(state, "failed" | "cooldown").then(|| "rate limited".into());
                model.retry_at = (state == "cooldown")
                    .then(|| std::time::Instant::now() + std::time::Duration::from_secs(30));
                let layout = view
                    .layout((1920, 1080), (100, 32), scale, &rows, 0, true, &model)
                    .unwrap();
                for node in [
                    Node::Status,
                    Node::ResultFilters,
                    Node::Table,
                    Node::Listings,
                ] {
                    assert_eq!(layout.bounds(node), base.bounds(node));
                }
                assert!(layout.find_bounds(Node::Error).is_none());
                let mut pixels = vec![0; 1920 * 1080 * 4];
                view.paint(
                    &mut pixels,
                    Frame {
                        size: (1920, 1080),
                        scale,
                        zoom: scale,
                        phase: 0.0,
                    },
                    &model,
                    &Interaction::default(),
                )
                .unwrap();
                assert_eq!(
                    view.hits.iter().any(|h| h.target == Target::Search),
                    matches!(state, "failed" | "ready")
                );
                assert_eq!(
                    view.hits.iter().any(|h| h.target == Target::TradeLink),
                    state == "ready"
                );
            }
        }
    }

    #[test]
    fn listing_preview_uses_fetched_item_and_has_no_editable_controls() {
        let model = fixture();
        let mut view = View::new().unwrap();
        for s in [1.0, 1.25] {
            let size = (1920, 1080);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let interaction = Interaction {
                hover: Some(Target::Listing(1)),
                tooltip: true,
                ..Interaction::default()
            };
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale: s,
                    zoom: s,
                    phase: 0.0,
                },
                &model,
                &interaction,
            )
            .unwrap();
            let popup = view
                .hits
                .iter()
                .find(|hit| hit.target == Target::Listing(1) && hit.rect.width == px(418, s))
                .unwrap()
                .rect;
            assert_eq!(popup.x, view.panel.x + view.panel.width);
            assert!(popup.x + popup.width <= size.0 && popup.y + popup.height <= size.1);
            assert_eq!(
                view.hit((popup.x + 20) as f64, (popup.y + 20) as f64),
                Some(Target::Listing(1))
            );
            assert!(
                view.hits
                    .iter()
                    .filter(|hit| popup.contains(hit.rect.x as f64, hit.rect.y as f64))
                    .all(|hit| view.hit(hit.rect.x as f64, hit.rect.y as f64)
                        == Some(Target::Listing(1)))
            );
            assert!(
                view.rows_for_item(
                    model.results.as_ref().unwrap().listings[1]
                        .item
                        .as_ref()
                        .unwrap(),
                    s
                )
                .iter()
                .any(|row| row.lines.iter().any(|line| line == "+90 to maximum Life"))
            );
            let item = model.results.as_ref().unwrap().listings[1]
                .item
                .as_ref()
                .unwrap();
            assert!(
                view.rows_for_item(item, s)
                    .iter()
                    .find(|row| row.lines.iter().any(|line| line == "Quality: +20%"))
                    .unwrap()
                    .separator
            );
            assert_eq!(
                model.filters.iter().filter(|filter| filter.enabled).count(),
                0
            );
            assert_eq!(view.bounds.x + view.bounds.width, popup.x + popup.width);
        }
    }

    #[test]
    fn error_plaque_precedes_search_and_wraps_without_clipping() {
        let mut model = fixture();
        model.results = None;
        let view = View::new().unwrap();
        for message in [
            "Trade service unavailable",
            "The trade service returned an error. Please wait for its rate limit to expire before trying this search again.",
        ] {
            model.error = Some(message.into());
            for s in [1.0, 1.25] {
                let rows = view.rows(&model, s);
                let layout = view
                    .layout((1920, 1080), (100, 32), s, &rows, 0, true, &model)
                    .unwrap();
                let error = layout.bounds(Node::Error);
                let search = layout.bounds(Node::Search);
                let lines = wrap(&view.ui, message, 14.0 * s, px(348, s)).len();
                assert_eq!(error.width, px(400, s));
                assert!((error.height as f32 - (36 + lines as u32 * 17) as f32 * s).abs() <= 1.0);
                assert!(search.y >= error.y + error.height + px(8, s) - 1);
                assert!(layout.find_bounds(Node::Footer).is_none());
            }
        }
    }
}
