pub(super) mod declaration;
pub(super) mod events;

use super::*;
use crate::presentation::settings::rgb;
use controls::{ACCENT, BAD, MUTED, TEXT};

fn timestamp(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    time::OffsetDateTime::parse(v.as_str()?, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
}
fn remaining(end: Option<i64>, now: i64) -> String {
    let Some(end) = end else { return "--".into() };
    let seconds = (end.saturating_sub(now) / 1000).max(0);
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

impl Pages {
    pub(super) fn layout_rail(&mut self) {
        self.layout_rail_at(retinue_ui::timeline::FrameTime::now());
    }
    pub(super) fn layout_rail_at(&mut self, time: retinue_ui::timeline::FrameTime) {
        self.layout_rail_content(time);
        self.rail.finish();
    }
    fn layout_rail_content(&mut self, time: retinue_ui::timeline::FrameTime) {
        let size = self.rail.size;
        let scale = self.rail.scale;
        self.rail.begin(size, scale);
        let error = self
            .activity_error
            .as_ref()
            .or(self.error.as_ref())
            .filter(|_| !self.pending.contains_key("activity"));
        self.rail_view.sync(
            &self.rail,
            self.rail_tab,
            !self.activity.is_null(),
            error.map(String::as_str),
        );
        if !self.rail_view.installed {
            let regular = self.rail.font_resource(controls::Weight::Regular);
            let bold = self.rail.font_resource(controls::Weight::Bold);
            self.rail.install_view(self.rail_view.view(
                self.events.view(regular.clone(), bold.clone()),
                self.market_rail_view.view(regular.clone(), bold),
                regular,
            ));
            self.rail_view.installed = true;
        }
        if self.rail_tab {
            self.sync_market_rail(time.instant);
        } else {
            self.events.sync(
                &self.rail,
                &self.activity,
                &self.fissure_mode,
                !self.connected && !self.fixture,
                events::milliseconds(time),
            );
            let height = (size.1 as f32 / scale - 224.0).max(0.0);
            self.rail_scroll.resize(
                self.rail.px(self.events.content_height()) as f32,
                self.rail.px(height) as f32,
                time.instant,
            );
            self.rail_offset = self.rail_scroll.offset(time.instant).round() as u32;
            self.events.scroll(self.rail_offset as f32 / scale);
        }
        self.rail.update_view(time);
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        self.rail_view
            .bind(&mut self.rail, self.rail_tab, self.connected);
        if self.rail_tab {
            self.bind_market_rail();
        } else if !self.activity.is_null() {
            self.events.bind(&mut self.rail);
            let mode = text(&self.notifications["fissures"], "mode");
            self.rail
                .view_control(declaration::NOTIFICATION, |ui, rect| {
                    ui.choice(
                        "rail:notification",
                        rect,
                        mode,
                        &[("off", "Off"), ("session", "GUI"), ("persistent", "On")],
                    )
                });
            for (i, (id, icon, label)) in [
                ("all", "all", "All fissures"),
                ("normal", "normal", "Normal fissures"),
                ("steel", "steel", "Steel Path fissures"),
            ]
            .into_iter()
            .enumerate()
            {
                self.rail.view_control(
                    retinue_render::Key::new("rail.filter", i as u64),
                    |ui, rect| {
                        let selected = self.fissure_mode == id;
                        if selected {
                            ui.fill(rect, 5.0, rgb(0x3a4160));
                        }
                        ui.tinted_icon(
                            ui.rect(4.0, 5.0, 18.0, 18.0),
                            icon,
                            if selected { TEXT } else { MUTED },
                        );
                        ui.hit(format!("rail:fissure:{id}"), rect, label, true);
                    },
                );
            }
            self.rail.view_control(declaration::SCROLL, |ui, rect| {
                ui.scrollbar(
                    "rail:scroll",
                    rect,
                    self.rail_scroll.maximum(),
                    self.rail_offset as f32,
                )
            });
        }
    }
    pub(super) fn rail_signal(&mut self, signal: Signal) {
        if signal.key == "rail:retry" {
            self.queue("activity", json!({"op":"activity_view"}));
            return;
        }
        if signal.key == "rail:scroll" {
            if let Some(offset) = signal.value.and_then(|v| v.parse().ok()) {
                self.rail_scroll.to(offset, false, Instant::now());
                self.rail_offset = self.rail_scroll.offset(Instant::now()).round() as u32;
            }
            return;
        }
        if let Some(tab) = signal.key.strip_prefix("rail:tab:") {
            self.rail_tab = tab == "market";
            self.rail_scroll.to(0.0, false, Instant::now());
        } else if let Some(mode) = signal.key.strip_prefix("rail:fissure:") {
            self.fissure_mode = mode.into();
        } else if signal.key == "rail:notification" {
            let mode = signal.value.unwrap_or_else(|| "off".into());
            self.queue(
                "notifications",
                json!({"op":"notification_settings_set","fissures":{"mode":mode}}),
            );
        } else if let Some(era) = signal.key.strip_prefix("rail:era:") {
            self.relics.search.clear();
            self.select_listing_category(Page::Relics, if era == "omnia" { "all" } else { era });
            self.navigation = Some((Page::Relics, String::new()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timers_parse_offsets_and_expire_at_zero() {
        assert_eq!(
            timestamp(&json!("2026-10-25T15:00:00+02:00")),
            timestamp(&json!("2026-10-25T13:00:00Z"))
        );
        assert_eq!(remaining(Some(59999), 0), "0m 59s");
        assert_eq!(remaining(Some(1), 1000), "0m 0s");
        assert_eq!(timestamp(&json!("invalid")), None);
    }
}
