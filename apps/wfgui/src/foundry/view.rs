use super::*;
use retinue_ui::composition::{Canvas, Color, Command, ImageFit};

impl Foundry {
    pub(super) fn text(&self, p: &mut Painter<'_>, text: &str, r: Rect, size: f32, color: [u8; 4]) {
        p.draw_text_vertically_centered(
            self.regular.as_ref(),
            TextLine::new(r.x, r.y, r.height),
            size * self.scale,
            text,
            color,
        );
    }
    pub(super) fn icon(&self, p: &mut Painter<'_>, badge: Badge, r: Rect) {
        if let Some(image) = self.icons.get(&badge).and_then(|r| self.images.get(&r.key)) {
            let (w, h) = image.dimensions();
            p.draw_image(
                image,
                r.x + r.width.saturating_sub(w) / 2,
                r.y + r.height.saturating_sub(h) / 2,
                w,
                h,
            );
        }
    }
    fn picture(&self, p: &mut Canvas, asset: Option<&Asset>, r: Rect, bottom: bool) {
        if let Some(image) = self.displayed_image(asset, r.width) {
            let (w, h) = image.dimensions();
            p.image(
                Rect {
                    x: r.x + r.width.saturating_sub(w) / 2,
                    y: r.y
                        + if bottom {
                            r.height.saturating_sub(h)
                        } else {
                            r.height.saturating_sub(h) / 2
                        },
                    width: w,
                    height: h,
                },
                image.clone(),
                ImageFit::Stretch,
            );
        }
    }
    pub fn paint(&self, p: &mut Painter<'_>, focus: retinue_ui::focus_visible::FocusVisible) {
        p.fill_rounded_rect(0, 0, self.size.0, self.size.1, self.px(10.0), PANEL);
        p.fill_rounded_rect(
            0,
            self.px(10.0),
            self.size.0,
            self.size.1.saturating_sub(self.px(10.0)),
            0,
            PANEL,
        );
        drop(self.resolved_composition());
        let mut toolbar = Canvas::new(self.scale);
        let mut overlays = Canvas::new(self.scale);
        self.paint_toolbar(&mut toolbar, focus);
        self.paint_overlays(&mut overlays, focus);
        let mut composition = self.composition.borrow_mut();
        composition.record_chrome(toolbar.commands, overlays.commands);
        composition.paint(self, p);
    }

    pub(super) fn paint_toolbar(
        &self,
        p: &mut Canvas,
        focus: retinue_ui::focus_visible::FocusVisible,
    ) {
        for (action, r) in self.layout_controls() {
            match action {
                Action::Category(i) => {
                    let progress = self.category_motion.weights[i];
                    let color = std::array::from_fn(|c| {
                        (rgb(0x232b42)[c] as f32
                            + (rgb(0x543494)[c] as f32 - rgb(0x232b42)[c] as f32) * progress)
                            .round() as u8
                    });
                    p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.height / 2, color);
                    self.record_icon(
                        p,
                        Badge::Category(i),
                        Rect {
                            x: r.x + self.px(7.0),
                            y: r.y + self.px(5.0),
                            width: self.px(22.0),
                            height: self.px(22.0),
                        },
                    );
                    if progress > 0.0 && r.width > self.px(43.0) {
                        let label = Rect {
                            x: r.x + self.px(35.0),
                            width: r.width.saturating_sub(self.px(43.0)),
                            ..r
                        };
                        p.viewport(label, (0, 0), |p| {
                            self.record_text(
                                p,
                                CATEGORIES[i],
                                Rect {
                                    width: self
                                        .px(text_width(&self.regular, CATEGORIES[i], 14.0) + 2.0),
                                    ..label
                                },
                                14.0,
                                [TEXT[0], TEXT[1], TEXT[2], (255.0 * progress).round() as u8],
                            )
                        });
                    }
                }
                Action::Search => p.commands.push(Command::External {
                    bounds: p.bounds(r),
                    key: composition::SEARCH,
                }),
                Action::ClearSearch => self.record_icon(p, Badge::ClearSearch, r),
                Action::Refresh => self.record_icon(p, Badge::Refresh, r),
                Action::Filters => {
                    p.fill_rounded_rect(
                        r.x,
                        r.y,
                        r.width,
                        r.height,
                        r.height / 2,
                        if self.filters.count() > 0 || self.filters_open() {
                            rgb(0x5923cb)
                        } else {
                            CARD
                        },
                    );
                    self.record_icon(p, Badge::Filters, r);
                }
                _ => (),
            }
            if focus.shows(self.focus == Some(action), action == Action::Search) {
                crate::style::focus_capsule(p, r, self.scale);
            }
        }
        let count = if let Some(error) = self.input_error.as_ref().or(self.error.as_ref()) {
            error.clone()
        } else if !self.ready {
            "Loading Foundry...".into()
        } else {
            format!("{} items", self.count())
        };
        let width = text_width(&self.regular, &count, 12.0 * self.scale).ceil() as u32;
        self.record_text(
            p,
            &count,
            Rect {
                x: self
                    .size
                    .0
                    .saturating_sub(width + self.px(20.0))
                    .max(self.px(10.0)),
                y: self.viewport.y.saturating_sub(self.px(21.0)),
                width,
                height: self.px(20.0),
            },
            12.0,
            if self.error.is_some() || self.input_error.is_some() {
                rgb(0xff8796)
            } else {
                MUTED
            },
        );
    }

    pub(super) fn paint_overlays(
        &self,
        p: &mut Canvas,
        focus: retinue_ui::focus_visible::FocusVisible,
    ) {
        if self.ready && self.filtered.is_empty() {
            self.record_text(
                p,
                "No matching Foundry items",
                Rect {
                    x: self.px(30.0),
                    y: self.viewport.y + self.px(50.0),
                    width: self.viewport.width,
                    height: self.px(30.0),
                },
                16.0,
                MUTED,
            );
        }
        if !self.popup_open()
            && focus.shows(
                matches!(
                    self.focus,
                    Some(Action::Favorite(_) | Action::Component(_, _))
                ),
                false,
            )
        {
            for (action, r) in self.item_controls() {
                if focus.shows(self.focus == Some(action), false) {
                    crate::style::focus_capsule(p, r, self.scale);
                }
            }
        }
        if self.scroll.maximum() > 0.0 {
            let r = self.scrollbar();
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.width / 2, rgb(0x727b92));
        }
        if self.filters_open() {
            p.viewport(self.filter_panel.popup.bounds, (0, 0), |p| {
                self.paint_filters(p, focus)
            });
        }
        if self.component_open() {
            p.viewport(self.component_panel.popup.bounds, (0, 0), |p| {
                self.paint_component(p, focus)
            });
        }
        if self.tooltip {
            if let Some(text) = &self.hover {
                let width = (text_width(&self.regular, text, 13.0 * self.scale).ceil() as u32
                    + self.px(16.0))
                .min(self.size.0);
                let r = Rect {
                    x: (self.pointer.0.max(0.0) as u32).min(self.size.0.saturating_sub(width)),
                    y: (self.pointer.1.max(0.0) as u32 + self.px(18.0))
                        .min(self.size.1.saturating_sub(self.px(30.0))),
                    width,
                    height: self.px(30.0),
                };
                p.fill_rounded_rect(r.x, r.y, r.width, r.height, self.px(4.0), rgb(0x323c58));
                p.viewport(r, (0, 0), |p| {
                    self.record_text(
                        p,
                        text,
                        Rect {
                            x: r.x + self.px(8.0),
                            ..r
                        },
                        13.0,
                        TEXT,
                    )
                });
            }
        }
    }
    pub(super) fn record_icon(&self, p: &mut Canvas, badge: Badge, r: Rect) {
        if let Some(image) = self.icons.get(&badge).and_then(|r| self.images.get(&r.key)) {
            let (w, h) = image.dimensions();
            p.image(
                Rect {
                    x: r.x + r.width.saturating_sub(w) / 2,
                    y: r.y + r.height.saturating_sub(h) / 2,
                    width: w,
                    height: h,
                },
                image.clone(),
                ImageFit::Stretch,
            );
        }
    }

    pub(super) fn record_text(
        &self,
        p: &mut Canvas,
        text: &str,
        r: Rect,
        size: f32,
        color: [u8; 4],
    ) {
        p.text(r, self.regular.clone(), text.into(), size, color);
    }

    pub(super) fn record_card_content(
        &self,
        p: &mut Canvas,
        c: &Card,
        layout: &composition::CardLayout,
    ) {
        let w = self.grid.width;
        let pad = self.px(10.0);
        let (star, _, mastered) = layout.title;
        self.record_icon(
            p,
            if self.preferences.is_favorite(&c.id) {
                Badge::FavoriteOn
            } else {
                Badge::Favorite
            },
            star,
        );
        if c.mastered {
            self.record_icon(p, Badge::Mastered, mastered);
        }
        if c.is_prime {
            self.record_icon(
                p,
                if c.vaulted {
                    Badge::Vaulted
                } else {
                    Badge::Unvaulted
                },
                Rect {
                    x: w.saturating_sub(pad + self.px(24.0)),
                    y: self.px(11.0),
                    width: self.px(24.0),
                    height: self.px(24.0),
                },
            );
        }
        if c.pending {
            let r = Rect {
                x: pad,
                y: self.px(14.0),
                width: self.px(18.0),
                height: self.px(18.0),
            };
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.height / 2, rgb(0x365ca0));
            self.record_icon(p, Badge::Pending, r);
        }
        let warframe = c.group == "warframe";
        let r = layout.hero;
        self.picture(p, c.asset.as_ref(), r, warframe);
        let level = self.snapshot.profile["player_level"].as_u64().unwrap_or(0);
        if c.mastery_requirement > level {
            let badge = Rect {
                x: r.x + self.px(2.0),
                y: r.y + r.height - self.px(23.0),
                width: self.px(46.0),
                height: self.px(21.0),
            };
            p.fill_rounded_rect(
                badge.x,
                badge.y,
                badge.width,
                badge.height,
                badge.height / 2,
                rgb(0x101620),
            );
            self.record_text(
                p,
                &format!("MR {}", c.mastery_requirement),
                Rect {
                    x: badge.x + self.px(6.0),
                    ..badge
                },
                12.0,
                TEXT,
            );
        }
        if c.subsumed && warframe {
            let r = layout.subsumed;
            p.fill_rounded_rect(r.x, r.y, r.width, r.height, r.height / 2, rgb(0x7e3a3a));
            p.outline(r, r.height / 2, self.px(1.0), rgb(0xb86767));
            self.record_icon(p, Badge::Subsumed, r);
        }
        for (part, layout) in c.components.iter().zip(&layout.parts) {
            let r = layout.image;
            let owned = part.owned >= part.required;
            let blueprint = part.blueprint_owned > 0;
            p.fill(
                r,
                r.width / 2,
                if blueprint || owned {
                    crate::style::COMPONENT_READY
                } else {
                    crate::style::COMPONENT_EMPTY
                },
            );
            if let Some(image) = self.displayed_image(part.asset.as_ref(), self.px(35.0)) {
                p.image(
                    Rect {
                        x: r.x + self.px(2.0),
                        y: r.y + self.px(2.0),
                        width: self.px(35.0),
                        height: self.px(35.0),
                    },
                    image.clone(),
                    ImageFit::Circle,
                );
            }
            if owned || blueprint {
                let bounds = p.bounds(r);
                p.commands.push(if let Some(image) = &self.component_glow {
                    Command::Image {
                        bounds,
                        image: image.clone().into(),
                        fit: retinue_ui::composition::ImageFit::Stretch,
                    }
                } else {
                    Command::InsetShadow {
                        bounds,
                        radius: (r.width / 2) as f32 / self.scale,
                        blur: 4.0,
                        spread: 1.0,
                        color: Color::from_bgra(crate::style::COMPONENT_GLOW),
                    }
                });
            }
            let query = self.search.borrow().text().to_lowercase();
            if !query.is_empty() && part.name.to_lowercase().contains(&query) {
                for dot in 0..24 {
                    let angle = std::f64::consts::TAU * dot as f64 / 24.0;
                    let radius = r.width as f64 / 2.0 - self.scale as f64;
                    p.fill_rounded_rect(
                        (r.x as f64 + r.width as f64 / 2.0 + angle.cos() * radius
                            - self.scale as f64 / 2.0)
                            .round()
                            .max(0.0) as u32,
                        (r.y as f64 + r.height as f64 / 2.0 + angle.sin() * radius
                            - self.scale as f64 / 2.0)
                            .round()
                            .max(0.0) as u32,
                        self.px(1.5),
                        self.px(1.5),
                        self.px(1.0),
                        crate::style::SEARCH_HIGHLIGHT,
                    );
                }
            }
            if part.owned > 0 {
                let mut count = part.owned.to_string();
                let badge = layout.count;
                let available = badge.width.saturating_sub(self.px(8.0));
                if text_width(&self.regular, &count, 12.5 * self.scale) > available as f32 {
                    while !count.is_empty()
                        && text_width(
                            &self.regular,
                            &format!("{count}\u{2026}"),
                            12.5 * self.scale,
                        ) > available as f32
                    {
                        count.pop();
                    }
                    count.push('\u{2026}');
                }
                p.fill_rounded_rect(
                    badge.x,
                    badge.y,
                    badge.width,
                    badge.height,
                    badge.height / 2,
                    [0, 0, 0, 0xbd],
                );
                p.clip(badge, |p| {
                    self.record_text(
                        p,
                        &count,
                        Rect {
                            x: badge.x + self.px(4.0),
                            y: badge.y + self.px(2.0),
                            width: available,
                            height: badge.height.saturating_sub(self.px(3.0)),
                        },
                        12.5,
                        TEXT,
                    )
                });
            }
            if part.owned_relic {
                let badge = layout.relic;
                p.fill_rounded_rect(
                    badge.x,
                    badge.y,
                    badge.width,
                    badge.height,
                    badge.width / 2,
                    [0x16, 0x4a, 0x4b, 0xb3],
                );
                self.record_icon(p, Badge::Relic, badge);
                p.outline(badge, badge.width / 2, self.px(2.0), rgb(0xcfb020));
            }
            if self.preferences.is_favorite(&part.id) {
                self.record_icon(
                    p,
                    Badge::FavoriteOn,
                    Rect {
                        x: r.x + r.width - self.px(16.0),
                        y: r.y.saturating_sub(self.px(2.0)),
                        width: self.px(16.0),
                        height: self.px(16.0),
                    },
                );
            }
        }
    }
}
