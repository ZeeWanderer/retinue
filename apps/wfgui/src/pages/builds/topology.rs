use super::*;
mod preview;
pub use preview::Preview;

pub fn plan_polarities(
    ui: &mut Ui,
    topology: &Value,
    values: &Value,
    width: f32,
    mut y: f32,
) -> f32 {
    for region in entries(&topology["regions"]) {
        let slots = entries(&region["slots"])
            .iter()
            .filter_map(|slot| {
                entries(values)
                    .iter()
                    .find(|value| value["slot_id"] == slot["id"])
            })
            .collect::<Vec<_>>();
        if !slots.is_empty() {
            y = polarity_region(
                ui,
                &slots,
                (number(region, "columns") as usize).clamp(1, 4),
                width,
                y,
            );
        }
    }
    y
}
fn polarity_region(ui: &mut Ui, slots: &[&Value], columns: usize, width: f32, y: f32) -> f32 {
    if slots.is_empty() {
        return y;
    }
    let columns = columns.min(slots.len());
    let cell = ((width - 24.0) / columns as f32).min(158.0);
    let left = (width - cell * columns as f32) / 2.0;
    for (i, slot) in slots.iter().enumerate() {
        let x = left + (i % columns) as f32 * cell;
        let top = y + (i / columns) as f32 * 65.0;
        let changed = slot["changed"] == true;
        ui.fill(ui.rect(x + 3.0, top, cell - 6.0, 58.0), 5.0, widgets::CARD);
        ui.text(
            ui.rect(x + 10.0, top + 3.0, cell - 20.0, 22.0),
            text(slot, "label"),
            12.0,
            MUTED,
        );
        let color = if changed { rgb(0xe8ca76) } else { GOOD };
        let final_x = if changed {
            x + cell * 0.66
        } else {
            x + cell / 2.0
        };
        if changed {
            draw_polarity(ui, text(slot, "before"), x + cell * 0.25, top + 28.0, MUTED);
            ui.text(
                ui.rect(x + cell * 0.43, top + 28.0, cell * 0.2, 24.0),
                ">",
                16.0,
                MUTED,
            );
        }
        draw_polarity(ui, text(slot, "polarity"), final_x, top + 28.0, color);
    }
    y + slots.len().div_ceil(columns) as f32 * 65.0 + 12.0
}
fn draw_polarity(ui: &mut Ui, name: &str, cx: f32, y: f32, color: [u8; 4]) {
    if let Some(bytes) = polarity(name) {
        ui.tinted_bitmap(ui.rect(cx - 12.0, y, 24.0, 24.0), bytes, color);
    } else {
        ui.text(ui.rect(cx - 5.0, y, 15.0, 24.0), "-", 16.0, color);
    }
}

pub fn render(
    ui: &mut Ui,
    preview: &mut Preview,
    topology: &Value,
    upgrades: &Value,
    polarities: &Value,
    shards: &Value,
    overrides: &Value,
    width: f32,
    mut y: f32,
) -> f32 {
    let start = y;
    let upgrades = entries(upgrades)
        .iter()
        .map(|u| (text(u, "topology_slot"), u))
        .collect::<HashMap<_, _>>();
    let polarities = entries(polarities)
        .iter()
        .map(|p| (text(p, "slot_id"), text(p, "polarity")))
        .collect::<HashMap<_, _>>();
    for region in entries(&topology["regions"]) {
        let slots = entries(&region["slots"]);
        if slots.is_empty() {
            continue;
        }
        let arcane = text(region, "id") == "arcanes";
        let shard = text(region, "id") == "shards";
        let columns = (number(region, "columns") as usize).max(1).min(slots.len());
        let unit = if arcane {
            184.0
        } else if shard {
            110.0
        } else {
            212.0
        };
        let factor = ((width - 20.0) / (unit * columns as f32)).min(1.0).max(0.1);
        let cell = unit * factor;
        let height = if shard { 90.0 } else { 120.0 } * factor;
        let left = (width - cell * columns as f32) / 2.0;
        ui.text(
            ui.rect(12.0, y, width - 24.0, 24.0),
            text(region, "label"),
            14.0,
            MUTED,
        );
        y += 32.0;
        for (i, slot) in slots.iter().enumerate() {
            let id = text(slot, "id");
            let upgrade = upgrades.get(id).copied().unwrap_or(&Value::Null);
            let x = left + (i % columns) as f32 * cell;
            let top = y + (i / columns) as f32 * height;
            if arcane {
                arcane_card(ui, upgrade, slot, x, top, factor);
            } else if shard {
                let shard = entries(shards)
                    .iter()
                    .find(|s| text(s, "topology_slot") == id || text(s, "slot_id") == id)
                    .or_else(|| entries(shards).get(i))
                    .unwrap_or(&Value::Null);
                ui.image(
                    ui.rect(x + 25.0 * factor, top, 60.0 * factor, 60.0 * factor),
                    asset(&shard["asset"]).as_ref(),
                );
                let label = if shard.is_null() {
                    "Empty"
                } else {
                    text(shard, "name")
                };
                ui.text(
                    ui.rect(x, top + 62.0 * factor, cell, 24.0 * factor),
                    label,
                    12.0 * factor,
                    MUTED,
                );
            } else {
                preview.card(
                    ui,
                    upgrade,
                    slot,
                    polarities.get(id).copied().unwrap_or("none"),
                    x + (unit - 200.0) * factor / 2.0,
                    top,
                    factor,
                );
                mod_card(
                    ui,
                    upgrade,
                    slot,
                    polarities.get(id).copied().unwrap_or("none"),
                    x + (unit - 200.0) * factor / 2.0,
                    top,
                    factor,
                    0.0,
                );
            }
            let label = if upgrade.is_null() {
                format!(
                    "{} - {}",
                    text(slot, "label"),
                    if slot["unlocked"] == false {
                        "Locked"
                    } else {
                        "Empty"
                    }
                )
            } else {
                format!(
                    "{} - Rank {:.0}/{:.0}\n{}",
                    text(upgrade, "name"),
                    number(upgrade, "rank"),
                    number(upgrade, "max_rank"),
                    entries(&upgrade["effects"])
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            ui.hit(
                format!("build:mod:{id}"),
                ui.rect(x, top, cell, height),
                label,
                true,
            );
        }
        y += height * slots.len().div_ceil(columns) as f32 + 12.0;
    }
    for override_ in entries(overrides) {
        let label = if override_.is_string() {
            override_.as_str().unwrap_or("").to_owned()
        } else {
            format!("Ability override: {}", text(override_, "name"))
        };
        ui.text(ui.rect(16.0, y, width - 32.0, 28.0), label, 14.0, TEXT);
        y += 30.0;
    }
    ui.region("build.topology", ui.rect(0.0, start, width, y - start));
    y
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Frame {
    Bronze,
    Silver,
    Gold,
    Legendary,
    Galvanized,
    Amalgam,
    Riven,
}
impl Frame {
    fn background(self) -> &'static [u8] {
        macro_rules! a {
            ($n:literal) => {
                include_bytes!(concat!(
                    "../../../resources/mod-frames/",
                    $n,
                    "Background.png"
                ))
            };
        }
        match self {
            Self::Legendary | Self::Galvanized => a!("Legendary"),
            Self::Amalgam => a!("Amalgam"),
            Self::Gold => a!("Gold"),
            Self::Silver | Self::Riven => a!("Silver"),
            Self::Bronze => a!("Bronze"),
        }
    }
    fn side(self) -> &'static [u8] {
        macro_rules! a {
            ($n:literal) => {
                include_bytes!(concat!(
                    "../../../resources/mod-frames/",
                    $n,
                    "SideLight.png"
                ))
            };
        }
        match self {
            Self::Legendary | Self::Galvanized | Self::Amalgam => a!("Legendary"),
            Self::Riven => a!("Omega"),
            Self::Gold => a!("Gold"),
            Self::Silver => a!("Silver"),
            Self::Bronze => a!("Bronze"),
        }
    }
    fn from(upgrade: &Value) -> Self {
        match text(upgrade, "mod_variant") {
            "galvanized" => Self::Galvanized,
            "amalgam" => Self::Amalgam,
            "riven" => Self::Riven,
            _ => match text(upgrade, "rarity").to_ascii_lowercase().as_str() {
                "legendary" => Self::Legendary,
                "rare" => Self::Gold,
                "uncommon" => Self::Silver,
                _ => Self::Bronze,
            },
        }
    }
    fn special(self) -> bool {
        matches!(self, Self::Galvanized | Self::Amalgam | Self::Riven)
    }
    fn bytes(self, top: bool) -> &'static [u8] {
        macro_rules! asset {
            ($n:literal) => {
                include_bytes!(concat!("../../../resources/mod-frames/", $n, ".png"))
            };
        }
        match (self, top) {
            (Self::Bronze, true) => asset!("BronzeFrameTop"),
            (Self::Bronze, false) => asset!("BronzeFrameBottom"),
            (Self::Silver, true) => asset!("SilverFrameTop"),
            (Self::Silver, false) => asset!("SilverFrameBottom"),
            (Self::Gold, true) => asset!("GoldFrameTop"),
            (Self::Gold, false) => asset!("GoldFrameBottom"),
            (Self::Legendary, true) => asset!("LegendaryFrameTop"),
            (Self::Legendary, false) => asset!("LegendaryFrameBottom"),
            (Self::Galvanized, true) => asset!("GalvanizedFrameTop"),
            (Self::Galvanized, false) => asset!("GalvanizedFrameBottom"),
            (Self::Amalgam, true) => asset!("AmalgamFrameTop"),
            (Self::Amalgam, false) => asset!("AmalgamFrameBottom"),
            (Self::Riven, true) => asset!("OmegaFrameTop"),
            (Self::Riven, false) => asset!("OmegaFrameBottom"),
        }
    }
}

fn mod_card(
    ui: &mut Ui,
    upgrade: &Value,
    slot: &Value,
    slot_polarity: &str,
    x: f32,
    y: f32,
    scale: f32,
    expansion: f32,
) {
    let x = x + 8.0 * scale;
    let y = y + 17.0 * scale;
    let height = 90.0 + 170.0 * expansion;
    if upgrade.is_null() {
        ui.bitmap(
            ui.rect(x, y, 184.0 * scale, 90.0 * scale),
            include_bytes!("../../../resources/mod-slots/empty.png"),
        );
        let label = if slot["unlocked"] == false {
            "Locked"
        } else {
            text(slot, "label")
        };
        let icon = match text(slot, "role") {
            "exilus" => Some(include_bytes!("../../../resources/mod-slots/exilus.png").as_slice()),
            "aura" => Some(include_bytes!("../../../resources/mod-slots/aura.png").as_slice()),
            "stance" => Some(include_bytes!("../../../resources/mod-slots/stance.png").as_slice()),
            _ => None,
        };
        if let Some(icon) = icon {
            ui.bitmap(
                ui.rect(
                    x + 72.0 * scale,
                    y + 10.0 * scale,
                    40.0 * scale,
                    40.0 * scale,
                ),
                icon,
            );
        }
        let label_width = ui.measure(label, 13.0 * scale, false);
        ui.text(
            ui.rect(
                x + (184.0 * scale - label_width) / 2.0,
                y + if icon.is_some() { 49.0 } else { 30.0 } * scale,
                label_width + 1.0,
                30.0 * scale,
            ),
            label,
            13.0 * scale,
            MUTED,
        );
        if let Some(bytes) = polarity(slot_polarity) {
            ui.bitmap(
                ui.rect(
                    x + 148.0 * scale,
                    y + 12.0 * scale,
                    18.0 * scale,
                    18.0 * scale,
                ),
                bytes,
            );
        }
        return;
    }
    let frame = Frame::from(upgrade);
    let special = frame.special();
    let art_x = if special { 10.0 } else { 5.0 };
    let art_w = if special { 164.0 } else { 174.0 };
    let content_h = if frame == Frame::Legendary {
        75.0
    } else {
        70.0
    };
    ui.covered_texture(
        ui.rect(
            x + (if special { 5.0 } else { 0.0 }) * scale,
            y,
            (if special { 174.0 } else { 184.0 }) * scale,
            (height - 90.0 + content_h) * scale,
        ),
        frame.background(),
        ui.px(286.0 * scale / (1.0 + 0.1 * expansion)),
    );
    let effects = clean_effects(
        &entries(&upgrade["effects"])
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let lines = retinue_ui::painter::wrap_text(
        &ui.regular,
        text(upgrade, "name"),
        16.0 * scale * ui.scale,
        150.0 * scale * ui.scale,
    );
    let effect_lines = retinue_ui::painter::wrap_text(
        &ui.regular,
        &effects,
        12.0 * scale * ui.scale,
        150.0 * scale * ui.scale,
    );
    let effect_height = effect_lines.len() as f32 * 15.0;
    let name_height = lines.len().min(3) as f32 * 19.0;
    let expanded_name_y = 215.0 - effect_height - 2.5 - name_height;
    let name_top = 23.0 + (expanded_name_y - 23.0) * expansion;
    let expanded_art_height = (expanded_name_y - 8.0).clamp(20.0, 174.0);
    let art = ui.rect(
        x + art_x * scale,
        y + 3.0 * scale,
        art_w * scale,
        (content_h - 3.0 + expansion * (expanded_art_height - content_h + 3.0)) * scale,
    );
    // Keep endpoint-sized derivatives throughout the animation, not one per frame.
    let sample_scale = scale / (1.0 + expansion * 0.1) * 1.1;
    ui.covered_sampled(
        art,
        asset(&upgrade["asset"]).as_ref(),
        ui.px(184.0 * sample_scale),
    );
    ui.fill(art, 0.0, [0, 0, 0, 85]);
    if expansion > 0.0 {
        let side_top = if special { 21.0 } else { 11.0 };
        for x in [x + 2.0 * scale, x + 174.0 * scale] {
            ui.texture(
                ui.rect(
                    x,
                    y + side_top * scale,
                    8.0 * scale,
                    182.0 * scale * expansion,
                ),
                frame.side(),
                ui.px(200.0 * sample_scale),
            );
        }
    }
    let name_y = y + name_top * scale;
    let line_h = 19.0 * scale;
    for (i, line) in lines.iter().take(3).enumerate() {
        let width =
            retinue_ui::painter::text_width(&ui.regular, line, 16.0 * scale * ui.scale) / ui.scale;
        ui.text(
            ui.rect(
                x + (184.0 * scale - width) / 2.0,
                name_y + i as f32 * line_h,
                width + 1.0,
                line_h,
            ),
            *line,
            16.0 * scale,
            TEXT,
        );
    }
    if expansion > 0.2 {
        for (index, line) in effect_lines.iter().enumerate() {
            let width = ui.measure(line, 12.0 * scale, false);
            ui.text(
                ui.rect(
                    x + (184.0 * scale - width) / 2.0,
                    name_y + name_height * scale + (3.0 + 15.0 * index as f32) * scale,
                    width + 1.0,
                    15.0 * scale,
                ),
                *line,
                12.0 * scale,
                [
                    TEXT[0],
                    TEXT[1],
                    TEXT[2],
                    ((expansion - 0.2) / 0.8 * 255.0) as u8,
                ],
            );
        }
    }
    let drain = upgrade["effective_drain"]
        .as_i64()
        .or_else(|| upgrade["drain"].as_i64())
        .or_else(|| upgrade["cost"].as_i64());
    let tint = match text(upgrade, "polarity_state") {
        "matched" => rgb(0x68ce5f),
        "mismatched" => rgb(0xf73533),
        _ => TEXT,
    };
    let right = if frame == Frame::Galvanized {
        13.0
    } else if special {
        10.0
    } else {
        4.0
    };
    if let Some(drain) = drain {
        let label = drain.to_string();
        let content = ui.measure(&label, 12.0 * scale, true) + 15.0 * scale;
        let left = x + (184.0 - right) * scale - content - 10.0 * scale;
        let top = y + 8.0 * scale;
        let width = content + 10.0 * scale;
        ui.polygon(
            &[
                [left + 6.0 * scale, top],
                [left + width, top],
                [left + width, top + 15.0 * scale],
                [left + 6.0 * scale, top + 15.0 * scale],
                [left, top + 7.5 * scale],
            ],
            [0, 0, 0, 155],
            rgb(0xdfdfdf),
        );
        ui.bold(
            ui.rect(
                left + 7.0 * scale,
                top,
                content - 13.0 * scale,
                15.0 * scale,
            ),
            label,
            12.0 * scale,
            tint,
        );
        let p = if text(upgrade, "mod_polarity").is_empty() {
            text(upgrade, "polarity")
        } else {
            text(upgrade, "mod_polarity")
        };
        if let Some(bytes) = polarity(p) {
            ui.tinted_bitmap(
                ui.rect(
                    x + (184.0 - right - 14.0) * scale,
                    y + 9.0 * scale,
                    12.0 * scale,
                    12.0 * scale,
                ),
                bytes,
                tint,
            );
        }
    }
    let top_width = if frame == Frame::Galvanized {
        200.0
    } else {
        184.0
    };
    let top_ratio = if frame == Frame::Galvanized {
        131.0 / 292.0
    } else if special {
        117.0 / 281.0
    } else {
        64.0 / 256.0
    };
    let top = ui.rect(
        x + (184.0 - top_width) / 2.0 * scale,
        y - 10.0 * scale,
        top_width * scale,
        top_width * top_ratio * scale,
    );
    ui.clipped_bitmap_sampled(
        ui.rect(
            x + (184.0 - top_width) / 2.0 * scale,
            y - 10.0 * scale,
            top_width * scale,
            (if special { 90.0 } else { 50.0 }) * scale,
        ),
        top,
        frame.bytes(true),
        ui.px(top_width * sample_scale),
    );
    let bottom_width = if special { 200.0 } else { 184.0 };
    let bottom_y = 10.0 + 170.0 * expansion;
    let bottom_h = bottom_width * if special { 120.0 / 292.0 } else { 0.5 };
    ui.clipped_bitmap_sampled(
        ui.rect(
            x + (184.0 - bottom_width) / 2.0 * scale,
            y + bottom_y * scale,
            bottom_width * scale,
            (if special { 85.0 } else { 80.0 }) * scale,
        ),
        ui.rect(
            x + (184.0 - bottom_width) / 2.0 * scale,
            y + bottom_y * scale,
            bottom_width * scale,
            bottom_h * scale,
        ),
        frame.bytes(false),
        ui.px(bottom_width * sample_scale),
    );
    let max = number(upgrade, "max_rank").min(10.0) as usize;
    let rank = number(upgrade, "rank") as usize;
    let bottom = if frame == Frame::Riven {
        3.0
    } else if special {
        0.0
    } else {
        2.0
    };
    if max > 0 {
        let width = 5.0 + (max - 1) as f32 * 7.0;
        let left = x + (184.0 - width) / 2.0 * scale;
        for i in 0..max {
            ui.fill(
                ui.rect(
                    left + i as f32 * 7.0 * scale,
                    y + (height - bottom - 10.0) * scale,
                    5.0 * scale,
                    5.0 * scale,
                ),
                2.5 * scale,
                if i < rank {
                    rgb(0xa6e6ff)
                } else {
                    rgb(0x525252)
                },
            );
        }
    }
    if matches!(slot_polarity, "omni" | "universal")
        || text(upgrade, "polarity_state") == "mismatched"
    {
        if let Some(bytes) = polarity(slot_polarity) {
            ui.tinted_bitmap(
                ui.rect(
                    x + 162.0 * scale,
                    y - 15.0 * scale,
                    14.0 * scale,
                    14.0 * scale,
                ),
                bytes,
                tint,
            );
        }
    }
}

fn clean_effects(text: &str) -> String {
    let mut tag = false;
    text.chars()
        .filter(|&c| match c {
            '<' => {
                tag = true;
                false
            }
            '>' => {
                tag = false;
                false
            }
            _ => !tag,
        })
        .collect()
}

fn arcane_card(ui: &mut Ui, upgrade: &Value, slot: &Value, x: f32, y: f32, scale: f32) {
    let x = x + 12.0 * scale;
    if upgrade.is_null() {
        ui.bitmap(
            ui.rect(
                x + 30.0 * scale,
                y + 15.0 * scale,
                100.0 * scale,
                60.0 * scale,
            ),
            include_bytes!("../../../resources/arcane-frames/empty.png"),
        );
        ui.text(
            ui.rect(
                x + 25.0 * scale,
                y + 82.0 * scale,
                110.0 * scale,
                24.0 * scale,
            ),
            if slot["unlocked"] == false {
                "Locked"
            } else {
                "Empty"
            },
            13.0 * scale,
            MUTED,
        );
        return;
    }
    macro_rules! frame {
        ($n:literal) => {
            include_bytes!(concat!("../../../resources/arcane-frames/", $n, ".png"))
        };
    }
    let bytes: &'static [u8] = match text(upgrade, "rarity").to_ascii_lowercase().as_str() {
        "legendary" => frame!("legendary"),
        "rare" => frame!("rare"),
        "uncommon" => frame!("uncommon"),
        _ => frame!("common"),
    };
    ui.clipped_bitmap(
        ui.rect(x, y, 160.0 * scale, 108.0 * scale),
        ui.rect(
            x - 10.0 * scale,
            y - 36.0 * scale,
            180.0 * scale,
            180.0 * scale,
        ),
        bytes,
    );
    ui.image(
        ui.rect(
            x + 20.0 * scale,
            y + 20.0 * scale,
            120.0 * scale,
            68.0 * scale,
        ),
        asset(&upgrade["asset"]).as_ref(),
    );
    let name = text(upgrade, "name");
    let width =
        retinue_ui::painter::text_width(&ui.regular, name, 14.0 * scale * ui.scale) / ui.scale;
    ui.text(
        ui.rect(
            x + (160.0 * scale - width.min(170.0 * scale)) / 2.0,
            y + 83.0 * scale,
            170.0 * scale,
            24.0 * scale,
        ),
        name,
        14.0 * scale,
        TEXT,
    );
    let rank = number(upgrade, "rank") as usize;
    for i in 0..rank.min(8) {
        let cx = x + (80.0 - (rank.min(8) - 1) as f32 * 4.0 + i as f32 * 8.0) * scale;
        let cy = y + 112.5 * scale;
        ui.polygon(
            &[
                [cx, cy - 3.0 * scale],
                [cx + 3.0 * scale, cy],
                [cx, cy + 3.0 * scale],
                [cx - 3.0 * scale, cy],
            ],
            [0; 4],
            rgb(0xdbc071),
        );
    }
}

pub fn polarity(name: &str) -> Option<&'static [u8]> {
    macro_rules! image {
        ($n:literal) => {
            include_bytes!(concat!("../../../resources/polarities/", $n, ".png"))
        };
    }
    Some(match name {
        "madurai" => image!("madurai"),
        "naramon" => image!("naramon"),
        "vazarin" => image!("vazarin"),
        "zenurik" => image!("zenurik"),
        "unairu" => image!("unairu"),
        "penjaga" => image!("penjaga"),
        "umbra" | "umbral" => image!("umbra"),
        "universal" | "omni" => image!("universal"),
        "aura" => image!("aura"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn galvanized_is_independent_of_legendary_rarity() {
        assert_eq!(
            Frame::from(&json!({"mod_variant":"galvanized","rarity":"LEGENDARY"})),
            Frame::Galvanized
        );
        assert_eq!(
            Frame::from(&json!({"rarity":"LEGENDARY"})),
            Frame::Legendary
        );
    }
}
