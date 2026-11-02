use super::*;
use crate::domain::preferences::Preferences;
mod summary_view;
use crate::presentation::widgets::aligned_label;
use retinue_render::{painter::CachedFont, painter::text_width};

const ROOT: Key = Key::new("mastery.summary", 0);
const RANK: Key = Key::new("mastery.rank", 0);
const LOADING: Key = Key::new("mastery.loading", 0);
const TITLES: [(&str, &str, &str); 3] = [
    ("Game content", "summary_game", "mastery.summary.game"),
    ("Star Chart", "summary_star", "mastery.summary.star"),
    (
        "Intrinsics",
        "summary_intrinsics",
        "mastery.summary.intrinsics",
    ),
];

#[derive(Clone, PartialEq, Default)]
struct Row {
    label: String,
    percent: String,
    fraction: String,
    percent_width: f32,
    fraction_width: f32,
}
#[derive(Clone, PartialEq, Default)]
struct Panel {
    percent: String,
    rows: Vec<Row>,
}
#[derive(Clone, PartialEq, Default)]
struct Data {
    level: String,
    rank: Option<Asset>,
    progress: f32,
    percent: String,
    xp: String,
    extension: f32,
    panels: Vec<Panel>,
}
#[derive(Clone)]
struct PanelGeometry {
    bounds: Bounds,
    cap: Bounds,
    icon: Bounds,
    title: Bounds,
    percent: Bounds,
    rows: Vec<[Bounds; 3]>,
}
#[derive(Clone)]
struct Geometry {
    xp: Bounds,
    panels: Vec<PanelGeometry>,
}

impl Geometry {
    fn new(data: &Data, m: &Metrics, font: &CachedFont) -> Self {
        let measure = |s: &str, size| text_width(font, s, size * m.scale) / m.scale;
        let widths = data
            .panels
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let width = p
                    .rows
                    .iter()
                    .map(|r| measure(&r.label, 14.67))
                    .fold(0.0, f32::max);
                (width + 116.0).max([290.0, 252.0, 180.0][i])
            })
            .collect::<Vec<_>>();
        let positions = summary_positions(m.width - 24.0, &widths);
        let panels = data
            .panels
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (x, y) = (12.0 + positions[i].0, 86.0 + positions[i].1);
                let width = widths[i];
                let bounds = m.bounds(x, y, width, 151.0);
                let local = |rx, ry, w, h| {
                    let r = m.bounds(x + rx, y + ry, w, h);
                    Bounds::new(r.x - bounds.x, r.y - bounds.y, r.width, r.height)
                };
                let tw = measure(TITLES[i].0, 17.0);
                let pw = measure(&p.percent, 17.0);
                let left = (width - 44.0 - tw - pw) / 2.0;
                let percent_width =
                    p.rows.iter().map(|r| r.percent_width).fold(0.0, f32::max) + 3.0;
                let fraction_width =
                    p.rows.iter().map(|r| r.fraction_width).fold(0.0, f32::max) + 3.0;
                let label_width = width - percent_width - fraction_width - 34.0;
                let rh = 101.0 / p.rows.len() as f32;
                PanelGeometry {
                    bounds,
                    cap: local(0.0, 30.0, width, 10.0),
                    icon: local(left, 5.0, 30.0, 30.0),
                    title: local(left + 37.0, 5.0, tw + 1.0, 30.0),
                    percent: local(left + 44.0 + tw, 5.0, pw + 1.0, 30.0),
                    rows: (0..p.rows.len())
                        .map(|j| {
                            let y = 45.0 + j as f32 * rh;
                            [
                                local(8.0, y, label_width, rh),
                                local(label_width + 15.0, y, percent_width, rh),
                                local(width - fraction_width - 8.0, y, fraction_width, rh),
                            ]
                        })
                        .collect(),
                }
            })
            .collect();
        let xp_width = text_width(font, &data.xp, 17.0 * m.scale) / m.scale;
        Self {
            xp: m.bounds(
                154.0 + ((m.width - 170.0).max(1.0) - xp_width).max(0.0) / 2.0,
                17.0,
                xp_width + 1.0,
                25.0,
            ),
            panels,
        }
    }
}

pub(super) struct Summary {
    data: State<Data>,
    pending: State<usize>,
    source: Value,
    scale: f32,
    revision: Option<(u64, u64)>,
    favored: f64,
    price_names: Vec<String>,
    widths: Vec<f32>,
}
impl Summary {
    pub fn new() -> Self {
        Self {
            data: State::default(),
            pending: State::default(),
            source: Value::Null,
            scale: 0.0,
            revision: None,
            favored: 0.0,
            price_names: Vec::new(),
            widths: Vec::new(),
        }
    }
    pub fn sync(
        &mut self,
        ui: &ControlHost,
        listing: &Listing,
        pending: &HashSet<String>,
        preferences: &Preferences,
    ) -> f32 {
        let revision = (listing.order_revision, preferences.favorite_revision());
        let changed = self.revision != Some(revision);
        if changed {
            self.favored = listing
                .items
                .iter()
                .filter(|item| preferences.is_favorite(&item.id))
                .map(|item| item.potential_xp as f64)
                .sum();
            self.price_names = listing.price_names();
            self.revision = Some(revision);
        }
        if changed || self.source != listing.data || self.scale != ui.scale {
            self.source = listing.data.clone();
            self.scale = ui.scale;
            let summary = &listing.data["summary"];
            let rank = &summary["rank_progress"];
            let block = |percent: f64, rows: Vec<(&str, &Value, &str)>| Panel {
                percent: format!("{percent:.0}%"),
                rows: rows
                    .into_iter()
                    .map(|(label, v, current)| {
                        let percent = format!("{:.0}%", number(v, "percent"));
                        let fraction =
                            format!("{:.0}/{:.0}", number(v, current), number(v, "total"));
                        Row {
                            label: label.into(),
                            percent_width: ui.measure(&percent, 14.67, false),
                            fraction_width: ui.measure(&fraction, 14.67, false),
                            percent: if v["percent"].is_number() {
                                percent
                            } else {
                                "--".into()
                            },
                            fraction: if v["total"].is_number() {
                                fraction
                            } else {
                                "--".into()
                            },
                        }
                    })
                    .collect(),
            };
            self.data.set(Data {
                level: format!("{:.0}", number(summary, "player_level")),
                rank: asset(&listing.data["profile"]["rank_asset"]),
                progress: (number(rank, "percent") / 100.0).clamp(0.0, 1.0) as f32,
                percent: format!("{:.0}%", number(rank, "percent")),
                xp: if rank["available"] == true {
                    format!(
                        "{:.0} / {:.0} XP",
                        number(rank, "current"),
                        number(rank, "total")
                    )
                } else {
                    "Mastery XP unavailable".into()
                },
                extension: (self.favored / number(rank, "total").max(1.0)) as f32,
                panels: vec![
                    block(
                        number(summary, "content_percent"),
                        vec![
                            ("Warframes / Archwings:", &summary["warframes"], "mastered"),
                            ("Weapons:", &summary["weapons"], "mastered"),
                            ("Companions:", &summary["companions"], "mastered"),
                        ],
                    ),
                    block(
                        number(&summary["star_chart"], "percent"),
                        vec![
                            ("Normal:", &summary["star_chart"]["normal"], "current"),
                            ("Junctions:", &summary["star_chart"]["junctions"], "current"),
                            ("Steel Path:", &summary["star_chart"]["steel"], "current"),
                            (
                                "Steel P. junctions:",
                                &summary["star_chart"]["steel_junctions"],
                                "current",
                            ),
                        ],
                    ),
                    block(
                        number(&summary["intrinsics"], "percent"),
                        vec![
                            ("Railjack:", &summary["intrinsics"]["railjack"], "current"),
                            ("Duviri:", &summary["intrinsics"]["duviri"], "current"),
                        ],
                    ),
                ],
            });
            self.widths = self
                .data
                .get()
                .panels
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let width = p
                        .rows
                        .iter()
                        .map(|r| ui.measure(&r.label, 14.67, false))
                        .fold(0.0, f32::max);
                    (width + 116.0).max([290.0, 252.0, 180.0][i])
                })
                .collect();
        }
        self.pending.set(
            self.price_names
                .iter()
                .filter(|n| pending.contains(*n))
                .count(),
        );
        summary_positions(ui.size.0 as f32 / ui.scale - 24.0, &self.widths)
            .iter()
            .map(|(_, y)| y + 237.0)
            .fold(0.0, f32::max)
    }

    pub fn view(&self, metrics: State<Metrics>, fonts: Fonts) -> Element {
        summary_view::SummaryView {
            data: self.data.clone(),
            pending: self.pending.clone(),
            metrics,
            fonts,
        }
        .into_element()
    }

    pub fn bind(&self, ui: &mut ControlHost) {
        ui.view_control(RANK, |ui, r| ui.image(r, self.data.get().rank.as_ref()));
        if let Some(r) = ui.view_bounds(ROOT) {
            ui.region("page.summary", r);
        }
        for (i, (_, icon, region)) in TITLES.iter().enumerate() {
            ui.view_icon(Key::new("mastery.summary.icon", i as u64), icon, None);
            if let Some(r) = ui.view_bounds(Key::new("mastery.summary.panel", i as u64)) {
                ui.region(region, r);
            }
        }
        if let Some(r) = ui.view_bounds(LOADING) {
            ui.region("mastery.prices.loading", r);
        }
        ui.view_control(LOADING, |ui, r| {
            ui.progress(r);
            ui.hit(
                "mastery:prices",
                r,
                format!("Refreshing {} prices", self.pending.get()),
                false,
            );
        });
    }
}
