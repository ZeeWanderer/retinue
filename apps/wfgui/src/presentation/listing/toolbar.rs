use super::*;

const TRACES: Key = Key::new("listing.traces", 0);
const TRACE_COUNT: Key = Key::new("listing.traces", 1);
const OWNED: Key = Key::new("listing.owned", 0);
const REFRESH: Key = Key::new("listing.refresh", 0);
const REFRESH_ICON: Key = Key::new("listing.refresh", 1);
const MODES: [(&str, &str, &str, f32); 3] = [
    ("easy", "Easy", "easy", 79.0),
    ("relics", "From relics", "axi", 132.0),
    ("platinum", "With platinum", "platinum", 132.0),
];

fn tab(m: &Metrics, weights: &[f32], index: usize) -> Bounds {
    let (collapsed, height, _) = m.tab_style();
    let x = 12.0
        + weights[..index]
            .iter()
            .enumerate()
            .map(|(i, weight)| collapsed + (m.labels[i] + 5.0) * weight + 5.0)
            .sum::<f32>();
    Bounds::new(
        x,
        m.tab_top(),
        collapsed + (m.labels[index] + 5.0) * weights[index],
        height,
    )
}

#[derive(PartialEq)]
pub(super) struct ToolbarView {
    pub model: Model,
    pub fonts: Fonts,
}

impl View for ToolbarView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.get();
        let weights = self.model.weights.get();
        let mode = self.model.mode.get();
        let x = if m.wrapped() {
            12.0
        } else {
            12.0 + weights
                .iter()
                .enumerate()
                .map(|(i, w)| m.tab_style().0 + (m.labels[i] + 5.0) * w + 5.0)
                .sum::<f32>()
        };
        ui! {
            ZStack() {
                for (i, _) in categories(m.page).iter().enumerate() {
                    let r = tab(&m, &weights, i);
                    let (collapsed, height, icon) = m.tab_style();
                    let color = std::array::from_fn(|channel| {
                        (controls::CARD[channel] as f32 + (ACCENT[channel] as f32
                            - controls::CARD[channel] as f32) * weights[i]).round() as u8
                    });
                    place(ui! {
                        ZStack() {
                            fill(color, if m.page == Page::Relics { 16.0 } else { 17.0 })
                            place(slot(Key::new("listing.category.icon", i as u64)),
                                Bounds::new((collapsed - icon) / 2.0, (height - icon) / 2.0, icon, icon))
                            place(slot(Key::new("listing.category.label", i as u64)),
                                Bounds::new((collapsed - icon) / 2.0 + icon + 5.0, 0.0,
                                    (r.width - collapsed - 5.0).max(0.0), height))
                        }.alignment(Alignment::Start)
                    }, r).id(Key::new("listing.category", i as u64)).interactive()
                }
                place(slot(SEARCH), m.bounds(x + 5.0, m.tools_y(),
                    (m.width - x - m.tools_width() - 17.0).max(80.0), 32.0)).interactive()
                match m.page {
                    Page::Inventory => {
                        place(slot(SORT), m.bounds(m.width - 247.0, m.tools_y(), 155.0, 32.0)).interactive()
                        place(slot(DIRECTION), m.bounds(m.width - 91.0, m.tools_y(), 34.0, 32.0)).interactive()
                        let r = m.bounds(m.width - 85.0, m.tools_y() + 8.0, 17.0, 17.0);
                        let pad = 2.0;
                        place(slot(DIRECTION_ICON), Bounds::new(r.x - pad, r.y - pad, r.width + 2.0 * pad, r.height + 2.0 * pad))
                        place(slot(FILTER), m.bounds(m.width - 45.0, m.tools_y(), 32.0, 32.0)).interactive()
                    }
                    Page::Relics => {
                        place(slot(TRACES), m.bounds(m.width - 270.0, m.tools_y() + 5.0, 22.0, 22.0))
                        place(slot(TRACE_COUNT), m.bounds(m.width - 242.0, m.tools_y(), 104.0, 32.0))
                        place(slot(OWNED), m.bounds(m.width - 136.0, m.tools_y(), 89.0, 32.0)).interactive()
                        place(slot(REFRESH_ICON), m.bounds(m.width - 39.0, m.tools_y() + 7.0, 20.0, 20.0))
                        place(slot(REFRESH), m.bounds(m.width - 45.0, m.tools_y(), 32.0, 32.0)).interactive()
                    }
                    Page::Mastery => {
                        place(slot(FILTER), m.bounds(m.width - 45.0, m.tools_y(), 32.0, 32.0)).interactive()
                        place(fill(crate::presentation::style::INSET_PANEL, 8.0),
                            m.bounds(4.0, m.tools_y() + 38.0, m.width - 8.0, m.height - m.tools_y() - 38.0))
                        place(fill(crate::presentation::style::INSET_PANEL, 0.0),
                            m.bounds(4.0, m.tools_y() + 50.0, m.width - 8.0, m.height - m.tools_y() - 50.0))
                        place(fitted_label("Best ways to level mastery", self.fonts.regular.clone(), 18.0, TEXT, false, m.scale),
                            m.bounds(16.0, m.tools_y() + 42.0, 230.0, 34.0))
                        place(fill(controls::CARD, 10.0),
                            m.bounds((m.width - 359.0).max(16.0), m.modes_y(), 343.0, 34.0))
                        for (i, &(id, label, _, width)) in MODES.iter().enumerate() {
                            let x = MODES[..i].iter().map(|m| m.3).sum::<f32>();
                            let r = Bounds::new((m.width - 359.0).max(16.0) + x, m.modes_y(), width, 34.0);
                            place(ui! {
                                ZStack() {
                                    if mode == id { fill(crate::presentation::style::SUMMARY_HEADER, 10.0) }
                                    place(slot(Key::new("listing.mode.icon", i as u64)),
                                        m.local(r, Bounds::new(8.0, 7.0, 20.0, 20.0)))
                                    place(fitted_label(label, self.fonts.light.clone(), 15.0, TEXT, false, m.scale),
                                        m.local(r, Bounds::new(33.0, 0.0, width - 38.0, 34.0)))
                                }.alignment(Alignment::Start)
                            }, m.bounds(r.x, r.y, r.width, r.height))
                                .id(Key::new("listing.mode", i as u64)).interactive()
                        }
                    }
                    _ => {}
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING)
        }
    }
}

pub(super) fn bind(view: &ListingController, ui: &mut ControlHost, listing: &Listing) {
    for (i, (id, label, icon)) in categories(view.page).iter().enumerate() {
        let weight = view.model.weights.get()[i];
        let icon_key = Key::new("listing.category.icon", i as u64);
        ui.view_icon(icon_key, icon, None);
        if let Some(rect) = ui.view_bounds(icon_key) {
            ui.region(&format!("list:category:{id}:icon"), rect);
        }
        ui.view_control(Key::new("listing.category.label", i as u64), |ui, r| {
            let mut color = TEXT;
            color[3] = (255.0 * weight).round() as u8;
            ui.clipped_text(r, *label, 13.0, color, controls::Weight::Regular);
        });
        ui.view_hit(
            Key::new("listing.category", i as u64),
            format!("list:category:{id}"),
            *label,
            true,
        );
    }
    ui.view_control(SEARCH, |ui, r| {
        ui.search_field(
            "list:search",
            r,
            &listing.search,
            if view.page == Page::Relics {
                "Search relics or rewards"
            } else {
                "Search items and components"
            },
            crate::presentation::widgets::search::Anchor::Trailing,
        );
    });
    match view.page {
        Page::Inventory => {
            ui.view_control(SORT, |ui, r| {
                ui.choice(
                    "list:sort",
                    r,
                    &listing.sort,
                    &[
                        ("name", "Name"),
                        ("price", "Platinum"),
                        ("ducats", "Ducats"),
                        ("amount", "Amount"),
                        ("value", "Ducats / platinum"),
                        ("complete", "Set completion"),
                    ],
                )
            });
            ui.view_control(DIRECTION_ICON, |ui, r| {
                let pad = ui.px(2.0);
                ui.arrow(
                    Rect {
                        x: r.x + pad,
                        y: r.y + pad,
                        width: r.width - 2 * pad,
                        height: r.height - 2 * pad,
                    },
                    listing.descending,
                    TEXT,
                );
            });
            ui.view_hit(
                DIRECTION,
                "list:direction",
                if listing.descending {
                    "Descending"
                } else {
                    "Ascending"
                },
                true,
            );
        }
        Page::Relics => {
            ui.view_icon(TRACES, "traces", None);
            ui.view_control(TRACE_COUNT, |ui, r| {
                ui.text(
                    r,
                    format!("{:.0} traces", number(&listing.data, "trace_count")),
                    13.0,
                    TEXT,
                )
            });
            ui.view_control(OWNED, |ui, r| {
                ui.toggle("list:owned", r, "Owned", listing.owned)
            });
            ui.view_icon(REFRESH_ICON, "refresh", None);
            ui.view_hit(REFRESH, "list:retry", "Refresh relics", true);
        }
        Page::Mastery => {
            for (i, (id, label, icon, _)) in MODES.iter().enumerate() {
                ui.view_icon(Key::new("listing.mode.icon", i as u64), icon, None);
                ui.view_hit(
                    Key::new("listing.mode", i as u64),
                    format!("list:mode:{id}"),
                    *label,
                    true,
                );
            }
        }
        _ => unreachable!(),
    }
    if view.page != Page::Relics {
        ui.view_control(FILTER, |ui, r| {
            filters::button(ui, r, listing.filter_count(), listing.filters.is_open())
        });
    }
}
