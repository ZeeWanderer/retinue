mod countdown_view;
mod cycle_view;
mod events_view;
mod fissure_group_view;
use super::*;
use crate::{
    presentation::widgets::fill, presentation::widgets::key, presentation::widgets::label,
    presentation::widgets::place, presentation::widgets::slot as icon,
};
use retinue_render::{Bounds, Color, Key, painter::CachedFont, painter::text_width};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};
use std::rc::Rc;

const ERAS: [&str; 7] = ["Lith", "Meso", "Neo", "Axi", "Requiem", "Omnia", "Vanguard"];
const VIEWPORT: Key = Key::new("rail.fissures.viewport", 0);

pub(in crate::presentation) fn milliseconds(time: FrameTime) -> i64 {
    time.wall_time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Countdown {
    activation: Option<i64>,
    end: Option<i64>,
}

impl Countdown {
    fn end_at(self, now: i64) -> Option<i64> {
        self.activation.filter(|at| *at > now).or(self.end)
    }
    fn label(self, now: i64, baro: bool) -> String {
        let end = self.end_at(now);
        let label = remaining(end, now);
        if baro {
            let active =
                self.activation.is_some_and(|at| at <= now) && self.end.is_some_and(|at| at > now);
            format!("Baro {} {label}", if active { "leaves in" } else { "in" })
        } else {
            label
        }
    }
}

impl TimelineSchedule for Countdown {
    fn next(&self, time: FrameTime) -> Option<Instant> {
        let now = milliseconds(time);
        let remaining = self.end_at(now)?.saturating_sub(now);
        if remaining <= 0 {
            return None;
        }
        let unit = if remaining >= 86_400_000 {
            3_600_000
        } else if remaining >= 3_600_000 {
            60_000
        } else {
            1_000
        };
        // The label floors remaining time, so its next change is just after this boundary.
        time.instant.checked_add(Duration::from_millis(
            (remaining % unit + 1).min(remaining) as u64
        ))
    }
}

#[derive(Clone, PartialEq)]
struct Cycle {
    index: usize,
    id: String,
    name: String,
    state: String,
    expiry: Option<i64>,
}

#[derive(Clone, PartialEq)]
struct Fissure {
    id: String,
    top: f32,
    mission: String,
    node: String,
    hard: bool,
    expiry: i64,
}

#[derive(Clone, PartialEq)]
struct Group {
    frame: State<Bounds>,
    rows: Items<Fissure>,
}

pub(in crate::presentation) struct Events {
    area: State<(f32, f32)>,
    ready: State<bool>,
    baro: State<Countdown>,
    cycles: Items<Cycle>,
    groups: Vec<Group>,
    offset: State<f32>,
    extent: State<f32>,
    disconnected: State<bool>,
    next_expiry: Option<i64>,
}

impl Events {
    pub fn new() -> Self {
        Self {
            area: State::new((0.0, 0.0)),
            ready: State::new(false),
            baro: State::new(Countdown {
                activation: None,
                end: None,
            }),
            cycles: Items::default(),
            groups: ERAS
                .iter()
                .map(|_| Group {
                    frame: State::new(Bounds::default()),
                    rows: Items::default(),
                })
                .collect(),
            offset: State::new(0.0),
            extent: State::new(0.0),
            disconnected: State::new(false),
            next_expiry: None,
        }
    }

    pub fn view(&self, regular: Rc<CachedFont>, bold: Rc<CachedFont>) -> Element {
        events_view::EventsView {
            area: self.area.clone(),
            ready: self.ready.clone(),
            baro: self.baro.clone(),
            cycles: self.cycles.clone(),
            groups: self.groups.clone(),
            offset: self.offset.clone(),
            extent: self.extent.clone(),
            disconnected: self.disconnected.clone(),
            regular: TextStyle::new(regular, 14.0, Color::from_bgra(TEXT)),
            bold: TextStyle::new(bold, 14.0, Color::from_bgra(TEXT)),
        }
        .into_element()
    }

    pub fn sync(
        &mut self,
        ui: &ControlHost,
        activity: &Value,
        filter: &str,
        disconnected: bool,
        now: i64,
    ) {
        let (w, h) = (ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale);
        self.area.set((w, h));
        self.ready.set(!activity.is_null());
        self.disconnected.set(disconnected);
        self.baro.set(Countdown {
            activation: timestamp(&activity["baro"]["activation"]),
            end: timestamp(&activity["baro"]["expiry"]),
        });
        self.cycles
            .replace(
                entries(&activity["cycles"])
                    .iter()
                    .take(4)
                    .enumerate()
                    .map(|(index, c)| {
                        (
                            key("rail.cycle", text(c, "id")),
                            Cycle {
                                index,
                                id: text(c, "id").into(),
                                name: text(c, "name").into(),
                                state: text(c, "state").into(),
                                expiry: timestamp(&c["expires_at"]),
                            },
                        )
                    }),
            )
            .expect("unique cycle identities");
        self.next_expiry = None;
        let mut y = 0.0;
        for (group, era) in self.groups.iter_mut().zip(ERAS) {
            let mut rows = entries(&activity["fissures"])
                .iter()
                .filter(|r| {
                    text(r, "tier") == era
                        && timestamp(&r["expiry"]).is_some_and(|at| at > now)
                        && match filter {
                            "normal" => r["hard"] != true,
                            "steel" => r["hard"] == true,
                            _ => true,
                        }
                })
                .collect::<Vec<_>>();
            rows.sort_by_key(|r| timestamp(&r["expiry"]));
            let height = if rows.is_empty() {
                0.0
            } else {
                (rows.len() as f32 * 25.0 + 12.0).max(65.0)
            };
            group
                .frame
                .set(Bounds::new(0.0, y, (w - 14.0).max(0.0), height));
            group
                .rows
                .replace(rows.iter().enumerate().map(|(index, r)| {
                    let expiry = timestamp(&r["expiry"]).unwrap();
                    self.next_expiry = Some(self.next_expiry.map_or(expiry, |at| at.min(expiry)));
                    (
                        key("rail.fissure", text(r, "id")),
                        Fissure {
                            id: text(r, "id").into(),
                            top: 6.0 + index as f32 * 25.0,
                            mission: text(r, "mission").into(),
                            node: text(r, "node").into(),
                            hard: r["hard"] == true,
                            expiry,
                        },
                    )
                }))
                .expect("unique fissure identities");
            if height > 0.0 {
                y += height + 6.0;
            }
        }
        self.extent.set(y);
    }

    pub fn content_height(&self) -> f32 {
        self.extent.get()
    }
    pub fn scroll(&mut self, offset: f32) {
        self.offset.set(offset);
    }
    pub fn expired(&self, now: i64) -> bool {
        self.next_expiry.is_some_and(|at| now >= at)
    }
    pub fn expiry_deadline(&self, time: FrameTime) -> Option<Instant> {
        let (width, height) = self.area.get();
        (width > 0.0 && height > 0.0).then_some(())?;
        time.instant.checked_add(Duration::from_millis(
            self.next_expiry?.saturating_sub(milliseconds(time)).max(0) as u64,
        ))
    }

    pub fn bind(&self, ui: &mut ControlHost) {
        ui.view_icon(Key::new("rail.resurgence", 0), "resurgence", None);
        ui.view_icon(Key::new("rail.notification", 0), "notification", None);
        for &id in self.cycles.order().keys() {
            let data = self.cycles.get(id).unwrap();
            ui.view_icon(Key::new("rail.cycle.icon", id.value), &data.id, None);
        }
        for (group, era) in self.groups.iter().zip(ERAS) {
            ui.view_icon(key("rail.era.icon", era), &era.to_lowercase(), None);
            ui.view_hit(
                key("rail.era.hit", era),
                format!("rail:era:{}", era.to_lowercase()),
                format!("{era} relics"),
                true,
            );
            for &id in group.rows.order().keys() {
                let row = group.rows.get(id).unwrap();
                ui.view_icon(
                    Key::new("rail.fissure.icon", id.value),
                    if row.hard { "steel" } else { "normal" },
                    Some(rgb(0xd8dbea)),
                );
                ui.view_hit(
                    Key::new("rail.fissure.hit", id.value),
                    format!("rail:details:{}", row.id),
                    format!("{} ({}) - {era}", row.mission, row.node),
                    false,
                );
            }
        }
        if let Some(viewport) = ui.view_bounds(VIEWPORT) {
            ui.region("fissures", viewport);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness(scale: f32, rows: usize) -> (ControlHost, Events, Value, FrameTime) {
        let mut ui = ControlHost::new().unwrap();
        ui.begin(((400.0 * scale) as u32, (400.0 * scale) as u32), scale);
        let mut events = Events::new();
        ui.install_view(ui! {
            ZStack() {
                fill(controls::PANEL, 0.0)
                events.view(ui.font_resource(controls::Weight::Regular), ui.font_resource(controls::Weight::Bold))
            }.alignment(Alignment::Start).frame(400.0, 400.0)
        });
        let time = FrameTime::now();
        let data = json!({"fissures": (0..rows).map(|i|json!({
            "id":format!("row-{i}"),"tier":"Lith","mission":"Survival","node":"Void",
            "hard":false,"expiry":milliseconds(time)+600_500+i as i64*1000,
        })).collect::<Vec<_>>()});
        events.sync(&ui, &data, "all", false, milliseconds(time));
        ui.update_view(time);
        events.bind(&mut ui);
        ui.finish();
        (ui, events, data, time)
    }

    #[test]
    fn countdown_deadlines_follow_display_precision_and_baro_transitions() {
        let time = FrameTime {
            instant: Instant::now(),
            wall_time: UNIX_EPOCH + Duration::from_secs(1000),
        };
        let now = milliseconds(time);
        for remaining_ms in [
            1, 999, 1000, 1555, 3_600_000, 3_655_000, 86_400_000, 90_056_000,
        ] {
            let value = Countdown {
                activation: None,
                end: Some(now + remaining_ms),
            };
            let due = value.next(time).unwrap();
            assert!(due > time.instant);
            let before = time.at(due - Duration::from_millis(1));
            assert_eq!(
                value.label(milliseconds(before), false),
                value.label(now, false)
            );
            let after = time.at(due);
            if remaining_ms >= 1000 {
                assert_ne!(
                    value.label(milliseconds(after), false),
                    value.label(now, false)
                );
            }
            assert_eq!(
                value.next(time.at(time.instant + Duration::from_millis(remaining_ms as u64))),
                None
            );
        }
        let baro = Countdown {
            activation: Some(now + 1000),
            end: Some(now + 3_600_000),
        };
        assert_eq!(baro.label(now, true), "Baro in 0m 1s");
        assert_eq!(baro.label(now + 1000, true), "Baro leaves in 59m 59s");
        assert_eq!(
            Countdown {
                activation: None,
                end: None
            }
            .next(time),
            None
        );
    }

    #[test]
    fn countdown_tick_changes_only_its_paint_and_keeps_input_geometry() {
        for scale in [1.0, 1.25, 2.0] {
            let (mut ui, _, _, time) = harness(scale, 1);
            let controls = ui.layout_report()["controls"].clone();
            assert_eq!(
                ui.update_view(time.at(time.instant + Duration::from_millis(100))),
                Damage::None
            );
            let damage = ui.update_view(time.at(time.instant + Duration::from_secs(1)));
            let rect = ui
                .view_geometry(Key::new(
                    "rail.fissure.timer",
                    key("rail.fissure", "row-0").value,
                ))
                .unwrap()
                .paint;
            assert_eq!(damage, Damage::rect(rect));
            let report = ui.layout_report();
            assert_eq!(report["composition"]["layout_computations"], 0);
            assert_eq!(report["composition"]["recordings"], 1);
            assert_eq!(report["composition"]["view"]["mounted"], 0);
            assert_eq!(report["composition"]["view"]["unmounted"], 0);
            assert_eq!(report["controls"], controls);
        }
    }

    #[test]
    fn offscreen_rows_and_collapsed_rail_do_not_keep_timers_or_images_active() {
        let (mut ui, mut events, data, time) = harness(1.0, 40);
        let initial = ui.requests(&Media::default()).len();
        ui.update_view(time.at(time.instant + Duration::from_secs(1)));
        assert!(
            ui.layout_report()["composition"]["view"]["bodies"]
                .as_u64()
                .unwrap()
                < 10
        );
        ui.begin((400, 400), 1.0);
        events.scroll(900.0);
        ui.update_view(time.at(time.instant + Duration::from_secs(1)));
        events.bind(&mut ui);
        ui.finish();
        assert!(ui.bounds("rail:details:row-0").is_none());
        assert!(ui.requests(&Media::default()).len() <= initial + 1);
        ui.begin((0, 400), 1.0);
        events.sync(&ui, &data, "all", false, milliseconds(time));
        ui.update_view(time.at(time.instant + Duration::from_secs(2)));
        ui.finish();
        assert_eq!(ui.deadline(), None);
        assert_eq!(events.expiry_deadline(time), None);
        assert!(ui.requests(&Media::default()).is_empty());
        assert_eq!(ui.layout_report()["composition"]["chunks"], 0);
    }

    #[test]
    fn refresh_keeps_row_identity_and_updates_expiry_without_clock_polling() {
        let (mut ui, mut events, mut data, time) = harness(1.0, 2);
        let geometry = ui.layout_report()["composition"]["geometry"].clone();
        events.sync(&ui, &data, "all", false, milliseconds(time));
        assert_eq!(ui.update_view(time), Damage::None);
        assert_eq!(ui.layout_report()["composition"]["view"]["bodies"], 0);
        data["fissures"][0]["expiry"] = json!(milliseconds(time) + 500_000);
        events.sync(&ui, &data, "all", false, milliseconds(time));
        assert_ne!(ui.update_view(time), Damage::None);
        assert_eq!(ui.layout_report()["composition"]["geometry"], geometry);
        assert!(!events.expired(milliseconds(time) + 499_999));
        assert!(events.expired(milliseconds(time) + 500_000));
        let later = time.at(time.instant + Duration::from_millis(500_000));
        events.sync(&ui, &data, "all", false, milliseconds(later));
        ui.begin((400, 400), 1.0);
        ui.update_view(later);
        events.bind(&mut ui);
        ui.finish();
        assert!(ui.bounds("rail:details:row-0").is_none());
        assert!(ui.bounds("rail:details:row-1").is_some());
    }

    fn render(ui: &ControlHost, media: &Media, pixels: &mut [u8], damage: Damage) {
        for rect in damage.regions(ui.size).rects() {
            let mut painter = Painter::new(pixels, ui.size.0, ui.size.1).unwrap();
            painter.clip_to_rect(*rect);
            ui.paint(&mut painter, media);
            painter.finish().unwrap();
        }
    }

    #[test]
    fn timers_and_late_icons_repair_like_full_replay_across_scales() {
        for scale in [1.0, 1.25, 2.0] {
            let (mut ui, mut events, mut data, time) = harness(scale, 8);
            data["cycles"] = json!(
                ["earth", "cetus", "vallis", "cambion"]
                    .into_iter()
                    .map(|id| json!({
                        "id":id,"name":id,"state":"Day","expires_at":milliseconds(time)+120_500,
                    }))
                    .collect::<Vec<_>>()
            );
            data["baro"] =
                json!({"activation":milliseconds(time)-1,"expiry":milliseconds(time)+600_500});
            events.sync(&ui, &data, "all", false, milliseconds(time));
            ui.update_view(time);
            events.bind(&mut ui);
            let mut media = Media::default();
            let mut pixels = vec![0; (ui.size.0 * ui.size.1 * 4) as usize];
            render(&ui, &media, &mut pixels, Damage::Full);
            let requests = ui.requests(&media);
            let mut cache = crate::presentation::foundry::images::cache(None);
            let images = requests
                .iter()
                .map(|request| (request.key, cache.prepare(request).unwrap()))
                .collect::<Vec<_>>();
            media.accept(&images, &requests);
            let keys = images.iter().map(|(key, _)| *key).collect();
            let damage = ui.image_damage(&keys, &media);
            render(&ui, &media, &mut pixels, damage);
            for seconds in [1, 2, 10] {
                let damage = ui.update_view(time.at(time.instant + Duration::from_secs(seconds)));
                render(&ui, &media, &mut pixels, damage);
                let mut full = vec![0; pixels.len()];
                ui.with_full_replay(|| render(&ui, &media, &mut full, Damage::Full));
                assert!(
                    pixels == full,
                    "partial timer/image repair at {scale}, {seconds}s"
                );
            }
            ui.begin((0, 0), scale);
            events.sync(&ui, &data, "all", false, milliseconds(time));
            ui.update_view(time);
            ui.begin(((400.0 * scale) as u32, (400.0 * scale) as u32), scale);
            events.sync(&ui, &data, "all", false, milliseconds(time));
            ui.update_view(time.at(time.instant + Duration::from_secs(10)));
            events.bind(&mut ui);
            ui.finish();
            let mut remounted = vec![0; pixels.len()];
            render(&ui, &media, &mut remounted, Damage::Full);
            assert!(
                pixels == remounted,
                "retained icons restored after remount at {scale}"
            );
        }
    }
}
