use super::*;
use crate::settings::rgb;
use widgets::{ACCENT, BAD, MUTED, TEXT};

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
    let seconds = ((end - now) / 1000).max(0);
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
        self.layout_rail_content();
        self.rail.finish();
    }
    fn layout_rail_content(&mut self) {
        let size = self.rail.size;
        let scale = self.rail.scale;
        self.rail.begin(size, scale);
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let w = size.0 as f32 / scale;
        let h = size.1 as f32 / scale;
        self.rail
            .fill(self.rail.rect(0.0, 0.0, w, h), 10.0, widgets::PANEL);
        self.rail.fill(
            self.rail.rect(0.0, 10.0, w, (h - 10.0).max(0.0)),
            0.0,
            widgets::PANEL,
        );
        for (i, (id, label)) in [("events", "Timers & Events"), ("market", "WFMarket")]
            .into_iter()
            .enumerate()
        {
            let x = i as f32 * w / 2.0;
            let selected = (i == 1) == self.rail_tab;
            let width = self.rail.measure(label, 16.0, false);
            let start = x + (w / 2.0 - width - 24.0).max(0.0) / 2.0;
            self.rail.tinted_bitmap(
                self.rail.rect(start, 9.0, 19.0, 19.0),
                crate::media::icon(if i == 0 { "world" } else { "market" }),
                if selected { TEXT } else { MUTED },
            );
            self.rail.text(
                self.rail.rect(start + 24.0, 4.0, width + 1.0, 30.0),
                label,
                16.0,
                if selected { TEXT } else { MUTED },
            );
            if selected {
                self.rail.fill(
                    self.rail.rect(x + 7.0, 33.0, w / 2.0 - 14.0, 2.0),
                    0.0,
                    TEXT,
                );
            }
            self.rail.hit(
                format!("rail:tab:{id}"),
                self.rail.rect(x, 0.0, w / 2.0, 36.0),
                label,
                true,
            );
        }
        if self.rail_tab {
            self.layout_market_rail();
            return;
        }
        let now = unix_ms();
        self.rail
            .fill(self.rail.rect(45.0, 44.0, 152.0, 24.0), 12.0, ACCENT);
        self.rail
            .icon(self.rail.rect(50.0, 46.0, 20.0, 20.0), "resurgence");
        self.rail.text(
            self.rail.rect(76.0, 44.0, 116.0, 24.0),
            "Prime Resurgence",
            12.0,
            TEXT,
        );
        let baro = &self.activity["baro"];
        let active = timestamp(&baro["activation"]).is_some_and(|t| t <= now)
            && timestamp(&baro["expiry"]).is_some_and(|t| t > now);
        let end = timestamp(&baro[if active { "expiry" } else { "activation" }]);
        self.rail.fill(
            self.rail.rect(w - 157.0, 44.0, 141.0, 24.0),
            12.0,
            widgets::CARD,
        );
        self.rail.text(
            self.rail.rect(w - 148.0, 44.0, 124.0, 24.0),
            format!(
                "Baro {} {}",
                if active { "leaves in" } else { "in" },
                remaining(end, now)
            ),
            12.0,
            TEXT,
        );
        let cycles = entries(&self.activity["cycles"]).to_vec();
        for (i, cycle) in cycles.iter().take(4).enumerate() {
            let x = 7.0 + (i % 2) as f32 * (w - 14.0) / 2.0;
            let y = 77.0 + (i / 2) as f32 * 56.0;
            let width = (w - 22.0) / 2.0;
            self.rail
                .fill(self.rail.rect(x, y, width, 49.0), 24.5, rgb(0x101623));
            self.rail
                .fill(self.rail.rect(x, y, 49.0, 49.0), 24.5, widgets::CARD);
            self.rail.icon(
                self.rail.rect(x + 7.0, y + 7.0, 35.0, 35.0),
                text(cycle, "id"),
            );
            let name = format!("{} ", text(cycle, "name").to_uppercase());
            let state = text(cycle, "state").to_uppercase();
            let nw = self.rail.measure(&name, 13.0, false);
            let sw = self.rail.measure(&state, 14.0, true);
            let tx = x + 51.0 + ((width - 57.0 - nw - sw) / 2.0).max(0.0);
            self.rail
                .text(self.rail.rect(tx, y + 2.0, nw, 23.0), name, 13.0, MUTED);
            self.rail.bold(
                self.rail.rect(tx + nw, y + 2.0, sw + 1.0, 23.0),
                state,
                14.0,
                TEXT,
            );
            let time = remaining(timestamp(&cycle["expires_at"]), now);
            let tw = self.rail.measure(&time, 14.0, false);
            self.rail.text(
                self.rail.rect(
                    x + 51.0 + ((width - 57.0 - tw) / 2.0).max(0.0),
                    y + 24.0,
                    tw + 1.0,
                    22.0,
                ),
                time,
                14.0,
                TEXT,
            );
        }
        self.rail.fill(
            self.rail.rect(5.0, 190.0, w - 10.0, (h - 190.0).max(0.0)),
            8.0,
            rgb(0x101623),
        );
        self.rail.fill(
            self.rail.rect(5.0, 200.0, w - 10.0, (h - 200.0).max(0.0)),
            0.0,
            rgb(0x101623),
        );
        self.rail.fill(
            self.rail.rect(5.0, 190.0, w - 10.0, 28.0),
            8.0,
            widgets::CARD,
        );
        self.rail.fill(
            self.rail.rect(5.0, 204.0, w - 10.0, 14.0),
            0.0,
            widgets::CARD,
        );
        self.rail.text(
            self.rail.rect(11.0, 190.0, 140.0, 28.0),
            "Void fissures",
            17.0,
            TEXT,
        );
        let mode = text(&self.notifications["fissures"], "mode").to_owned();
        self.rail
            .icon(self.rail.rect(w - 186.0, 195.0, 18.0, 18.0), "notification");
        self.rail.choice(
            "rail:notification",
            self.rail.rect(w - 164.0, 190.0, 60.0, 27.0),
            &mode,
            &[("off", "Off"), ("session", "GUI"), ("persistent", "On")],
        );
        for (i, (id, icon, label)) in [
            ("all", "all", "All fissures"),
            ("normal", "normal", "Normal fissures"),
            ("steel", "steel", "Steel Path fissures"),
        ]
        .into_iter()
        .enumerate()
        {
            let x = w - 95.0 + i as f32 * 29.0;
            let rect = self.rail.rect(x, 190.0, 26.0, 27.0);
            if self.fissure_mode == id {
                self.rail.fill(rect, 5.0, rgb(0x3a4160));
            }
            self.rail.tinted_icon(
                self.rail.rect(x + 4.0, 195.0, 18.0, 18.0),
                icon,
                if self.fissure_mode == id { TEXT } else { MUTED },
            );
            self.rail
                .hit(format!("rail:fissure:{id}"), rect, label, true);
        }
        let viewport = self.rail.rect(7.0, 224.0, w - 14.0, (h - 224.0).max(0.0));
        let fissures = entries(&self.activity["fissures"])
            .iter()
            .filter(|f| {
                timestamp(&f["expiry"]).is_some_and(|t| t > now)
                    && match self.fissure_mode.as_str() {
                        "normal" => f["hard"] != true,
                        "steel" => f["hard"] == true,
                        _ => true,
                    }
            })
            .cloned()
            .collect::<Vec<_>>();
        self.rail.begin_scroll(viewport, self.rail_offset);
        let mut y = 0.0;
        for era in ["Lith", "Meso", "Neo", "Axi", "Requiem", "Omnia", "Vanguard"] {
            let mut rows = fissures
                .iter()
                .filter(|f| text(f, "tier") == era)
                .collect::<Vec<_>>();
            rows.sort_by_key(|f| timestamp(&f["expiry"]));
            if rows.is_empty() {
                continue;
            }
            self.rail.begin_scope(("fissures", era));
            let height = (rows.len() as f32 * 25.0 + 12.0).max(65.0);
            self.rail.fill(
                self.rail.rect(0.0, y, w - 14.0, height),
                7.0,
                widgets::PANEL,
            );
            self.rail.icon(
                self.rail.rect(12.0, y + (height - 39.0) / 2.0, 39.0, 39.0),
                &era.to_lowercase(),
            );
            self.rail.hit(
                format!("rail:era:{}", era.to_lowercase()),
                self.rail.rect(3.0, y, 56.0, height),
                format!("{} relics", era),
                true,
            );
            for (i, fissure) in rows.into_iter().enumerate() {
                self.rail.begin_scope(text(fissure, "id"));
                let top = y + 6.0 + i as f32 * 25.0;
                self.rail.tinted_icon(
                    self.rail.rect(62.0, top + 5.0, 15.0, 15.0),
                    if fissure["hard"] == true {
                        "steel"
                    } else {
                        "normal"
                    },
                    rgb(0xd8dbea),
                );
                let mission = text(fissure, "mission");
                let title_width = self.rail.measure(mission, 13.0, true) + 6.0;
                let available = (w - 172.0).max(20.0);
                self.rail.bold(
                    self.rail.rect(82.0, top, title_width.min(available), 25.0),
                    mission,
                    13.0,
                    TEXT,
                );
                if available > title_width {
                    self.rail.text(
                        self.rail
                            .rect(82.0 + title_width, top, available - title_width, 25.0),
                        format!("({})", text(fissure, "node")),
                        12.5,
                        TEXT,
                    );
                }
                self.rail.text(
                    self.rail.rect(w - 80.0, top, 62.0, 25.0),
                    remaining(timestamp(&fissure["expiry"]), now),
                    12.5,
                    TEXT,
                );
                self.rail.hit(
                    format!("rail:details:{}", text(fissure, "id")),
                    self.rail.rect(60.0, top, w - 78.0, 25.0),
                    format!("{mission} ({}) - {}", text(fissure, "node"), era),
                    false,
                );
                self.rail.end_scope();
            }
            self.rail.end_scope();
            y += height + 6.0;
        }
        self.rail.end_scroll();
        self.rail_scroll.resize(
            self.rail.px(y) as f32,
            viewport.height as f32,
            Instant::now(),
        );
        self.rail.scrollbar(
            "rail:scroll",
            viewport,
            self.rail_scroll.maximum(),
            self.rail_offset as f32,
        );
        self.rail.region("fissures", viewport);
        if fissures.is_empty() {
            self.rail.text(
                self.rail.rect(20.0, 245.0, w - 40.0, 30.0),
                if self.activity.is_null() {
                    "Loading events..."
                } else {
                    "No active fissures"
                },
                14.0,
                MUTED,
            );
        }
        if !self.connected && !self.fixture {
            self.rail.text(
                self.rail.rect(12.0, h - 28.0, w - 24.0, 24.0),
                "Disconnected - showing cached events",
                12.0,
                BAD,
            );
        }
    }
    pub(super) fn rail_signal(&mut self, signal: Signal) {
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
