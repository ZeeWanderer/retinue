use super::*;
use crate::presentation::widgets::menu;
mod overlays_view;
use retinue_render::painter::CachedFont;
use std::rc::Rc;

const PANEL: Key = Key::new("listing.filters", 0);
const HEADER: Key = Key::new("listing.filters", 1);
const CLOSE: Key = Key::new("listing.filters", 2);
const CONTENT: Key = Key::new("listing.filters", 3);
const CLEAR: Key = Key::new("listing.filters", 4);
const SCROLLBAR: Key = Key::new("listing.filters", 5);
const MENU: Key = Key::new("listing.menu", 0);

#[derive(Clone, PartialEq)]
struct FilterGeometry {
    bounds: Bounds,
    offset: f32,
    groups: usize,
}

impl FilterGeometry {
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Bounds {
        Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
    }
    fn viewport(&self) -> Bounds {
        self.rect(
            12.0,
            40.0,
            self.bounds.width - 24.0,
            self.bounds.height - 88.0,
        )
    }
}

#[derive(Clone, PartialEq)]
struct Menu {
    layout: menu::Layout,
    entries: Vec<(String, String, bool)>,
    hover: Option<usize>,
    identity: Key,
    scale: f32,
}

pub(super) struct Overlays {
    filters: State<Option<FilterGeometry>>,
    menu: State<Option<Menu>>,
}

impl Overlays {
    pub fn new() -> Self {
        Self {
            filters: State::default(),
            menu: State::default(),
        }
    }

    pub fn view(&self, regular: Rc<CachedFont>) -> Element {
        overlays_view::OverlaysView {
            filters: self.filters.clone(),
            menu: self.menu.clone(),
            text: retinue_ui::drawing::TextStyle::new(
                regular,
                14.0,
                retinue_render::Color::from_bgra(TEXT),
            ),
        }
        .into_element()
    }

    pub fn sync(&self, ui: &ControlHost, listing: &mut Listing, metrics: &Metrics) {
        let margin = ui.px(8.0);
        let available = Rect {
            x: margin,
            y: margin,
            width: ui.size.0.saturating_sub(2 * margin),
            height: ui.size.1.saturating_sub(2 * margin),
        };
        self.filters.set(if listing.filters.is_open() {
            let groups = listing.filter_groups().len();
            let content_height = groups as f32 * 64.0;
            let anchor = metrics.pixel_rect(metrics.bounds(
                metrics.width - 45.0,
                metrics.tools_y(),
                32.0,
                32.0,
            ));
            listing.filters.layout(
                anchor,
                available,
                (ui.px(276.0), ui.px(content_height + 88.0)),
                margin,
            );
            let bounds = logical(listing.filters.bounds, ui.scale);
            let viewport_height = ui.px((bounds.height - 88.0).max(0.0));
            let now = Instant::now();
            listing
                .filter_scroll
                .resize(ui.px(content_height) as f32, viewport_height as f32, now);
            listing.filter_offset = listing.filter_scroll.offset(now).round() as u32;
            Some(FilterGeometry {
                bounds,
                offset: listing.filter_offset as f32 / ui.scale,
                groups,
            })
        } else {
            None
        });
        self.menu.set(listing.menu.and_then(|(index, component)| {
            let row = listing.items.get(index)?;
            let target = target(row, component, listing.page == Page::Relics);
            let entries: Vec<_> = [
                ("foundry", "Show in Foundry", true),
                ("relics", "Show in Relic Planner", true),
                ("market", "View market listings", !target.market.is_empty()),
            ]
            .into_iter()
            .map(|(id, label, on)| (format!("list:menu:{id}"), label.to_owned(), on))
            .collect();
            let anchor = Rect {
                x: listing.menu_anchor.0.max(0.0) as u32,
                y: listing.menu_anchor.1.max(0.0) as u32,
                ..Rect::default()
            };
            let layout = menu::Layout::new(
                anchor,
                available,
                target.name,
                &[],
                entries.len(),
                &ui.font_resource(controls::Weight::Regular),
                ui.scale,
            );
            let hover = entries
                .iter()
                .position(|(k, _, _)| ui.hover.as_ref() == Some(k));
            let identity = key(
                "listing.menu.paint",
                (
                    target.name,
                    &entries,
                    hover,
                    (anchor.x, anchor.y),
                    ui.size,
                    ui.scale.to_bits(),
                ),
            );
            Some(Menu {
                layout,
                entries,
                hover,
                identity,
                scale: ui.scale,
            })
        }));
    }

    pub fn bind(&self, ui: &mut ControlHost, listing: &Listing) {
        if self.filters.get().is_some() {
            ui.begin_modal();
            for (id, name) in [(PANEL, "page.filters"), (CONTENT, "page.filters.content")] {
                if let Some(r) = ui.view_bounds(id) {
                    ui.region(name, r);
                }
            }
            ui.view_control(PANEL, |ui, r| {
                ui.fill(r, 10.0, crate::presentation::style::PANEL);
                ui.outline(r, 10.0, crate::presentation::style::FILTER_EDGE);
            });
            ui.view_control(HEADER, |ui, r| {
                let w = r.width as f32 / ui.scale;
                ui.text(ui.rect(12.0, 0.0, w - 48.0, 40.0), "Filters", 15.0, TEXT);
                ui.icon(ui.rect(w - 29.0, 10.0, 14.0, 14.0), "close");
            });
            ui.view_hit(CLOSE, "list:filters:close", "Close filters", true);
            let values = [
                &listing.prime,
                &listing.mastered,
                &listing.vaulted,
                &listing.duplicates,
                &listing.complete,
            ];
            for (i, (id, title, yes, no)) in listing.filter_groups().into_iter().enumerate() {
                ui.view_control(Key::new("listing.filter.title", i as u64), |ui, r| {
                    ui.aligned(r, title, 15.0, TEXT, controls::Weight::Regular, 0.5)
                });
                for (j, (value, label)) in [("yes", yes), ("no", no)].into_iter().enumerate() {
                    let name = format!("list:filter:{id}:{value}");
                    ui.view_control(
                        Key::new("listing.filter.choice", (i * 2 + j) as u64),
                        |ui, r| {
                            ui.fill(
                                r,
                                16.0,
                                crate::presentation::style::filter_color(
                                    values[i] == value,
                                    ui.hover.as_ref() == Some(&name),
                                ),
                            );
                            ui.aligned(r, label, 14.0, TEXT, controls::Weight::Regular, 0.5);
                            ui.hit(name, r, label, true);
                        },
                    );
                }
            }
            ui.view_control(SCROLLBAR, |ui, r| {
                ui.scrollbar(
                    "list:filters:scroll",
                    r,
                    listing.filter_scroll.maximum(),
                    listing.filter_offset as f32,
                )
            });
            ui.view_control(CLEAR, |ui, r| {
                let active = listing.filter_count() > 0;
                ui.fill(
                    r,
                    15.0,
                    crate::presentation::style::filter_color(
                        false,
                        ui.hover.as_deref() == Some("list:filters:clear"),
                    ),
                );
                ui.aligned(
                    r,
                    "Clear filters",
                    14.0,
                    if active { TEXT } else { MUTED },
                    controls::Weight::Regular,
                    0.5,
                );
                ui.hit("list:filters:clear", r, "Clear filters", active);
            });
        }
        if let Some(menu) = self.menu.get() {
            ui.begin_context(&menu.entries, "list:menu:close");
            if let Some(bounds) = ui.view_bounds(MENU) {
                ui.region("page.menu", bounds);
            }
            for (i, (key, label, enabled)) in menu.entries.iter().enumerate() {
                ui.view_hit(Key::new("listing.menu.row", i as u64), key, label, *enabled);
            }
        }
    }
}

fn logical(r: Rect, scale: f32) -> Bounds {
    Bounds::new(
        r.x as f32 / scale,
        r.y as f32 / scale,
        r.width as f32 / scale,
        r.height as f32 / scale,
    )
}
