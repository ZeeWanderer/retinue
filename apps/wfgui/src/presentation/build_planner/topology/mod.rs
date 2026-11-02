use super::*;
mod preview;
mod slot;
pub(super) use preview::Frame as PreviewFrame;
pub use preview::Preview;
pub(super) use slot::Kind;
pub(super) use slot::Slot;

pub(super) fn draw_polarity(ui: &mut ControlHost, name: &str, cx: f32, y: f32, color: [u8; 4]) {
    if let Some(bytes) = polarity(name) {
        ui.tinted_bitmap(ui.rect(cx - 12.0, y, 24.0, 24.0), bytes, color);
    } else {
        ui.text(ui.rect(cx - 5.0, y, 15.0, 24.0), "-", 16.0, color);
    }
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
    fn color(self) -> [u8; 4] {
        rgb(match self {
            Self::Bronze => 0xc79989,
            Self::Silver => 0xbec0c2,
            Self::Gold => 0xfbecc4,
            Self::Riven => 0xa783c8,
            _ => 0xdfdfdf,
        })
    }
    fn lower_tab(self) -> &'static [u8] {
        macro_rules! tab {
            ($n:literal) => {
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-frames/",
                    $n,
                    "LowerTab.png"
                ))
            };
        }
        match self {
            Self::Bronze => tab!("Bronze"),
            Self::Silver => tab!("Silver"),
            Self::Gold => tab!("Gold"),
            Self::Riven => tab!("Riven"),
            _ => tab!("Legendary"),
        }
    }
    fn side_geometry(self) -> (f32, f32) {
        match self {
            Self::Legendary => (3.0, 11.0),
            Self::Galvanized | Self::Amalgam => (7.0, 21.0),
            Self::Riven => (7.0, 24.0),
            _ => (2.0, 24.0),
        }
    }
    fn background(self) -> &'static [u8] {
        macro_rules! a {
            ($n:literal) => {
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-frames/",
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
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-frames/",
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
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-frames/",
                    $n,
                    ".png"
                ))
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

struct ModText {
    name: Vec<String>,
    effects: Vec<String>,
    height: f32,
    name_y: f32,
    effects_y: f32,
    art_height: f32,
}

impl ModText {
    fn new(ui: &ControlHost, upgrade: &Value, frame: Frame) -> Self {
        let name_width = match frame {
            Frame::Galvanized => 150.0,
            Frame::Amalgam | Frame::Riven => 160.0,
            _ => 159.0,
        };
        let name = retinue_render::painter::wrap_text(
            &ui.regular,
            text(upgrade, "name"),
            17.0,
            name_width,
        )
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let effects = entries(&upgrade["effects"])
            .iter()
            .filter_map(Value::as_str)
            .flat_map(|effect| {
                let text = clean_effects(effect);
                retinue_render::painter::wrap_text(&ui.regular, &text, 13.0, 159.0)
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let name_h = name.len() as f32 * 19.0;
        let effects_h = effects.len() as f32 * 15.0;
        let gap = if effects.is_empty() { 0.0 } else { 2.5 };
        let footer = if frame.special() || frame == Frame::Legendary {
            61.0
        } else {
            51.0
        };
        let name_min = match frame {
            Frame::Galvanized => 80.0,
            Frame::Amalgam | Frame::Riven => 68.0,
            _ => 40.0,
        };
        let height = (name_min + name_h + gap + effects_h + footer).max(260.0);
        let name_y = height - footer - effects_h - gap - name_h;
        Self {
            name,
            effects,
            height,
            name_y,
            effects_y: name_y + name_h + gap,
            art_height: (name_y - 8.0).clamp(20.0, 174.0),
        }
    }
}

fn mod_card(
    ui: &mut ControlHost,
    upgrade: &Value,
    slot: &Value,
    slot_polarity: &str,
    x: f32,
    y: f32,
    scale: f32,
    expansion: f32,
) -> f32 {
    let origin_x = x;
    let x = x + 8.0 * scale;
    let y = y + 17.0 * scale;
    if upgrade.is_null() {
        ui.bitmap(
            ui.rect(x, y, 184.0 * scale, 90.0 * scale),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/mod-slots/empty.png"
            )),
        );
        let label = if slot["unlocked"] == false {
            "Locked"
        } else {
            text(slot, "label")
        };
        let icon = match text(slot, "role") {
            "exilus" => Some(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-slots/exilus.png"
                ))
                .as_slice(),
            ),
            "aura" => Some(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-slots/aura.png"
                ))
                .as_slice(),
            ),
            "stance" => Some(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/mod-slots/stance.png"
                ))
                .as_slice(),
            ),
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
                origin_x + (200.0 * scale - label_width) / 2.0,
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
        return 260.0;
    }
    let frame = Frame::from(upgrade);
    let layout = ModText::new(ui, upgrade, frame);
    let height = 90.0 + (layout.height - 90.0) * expansion;
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
            (content_h + (layout.height - 18.0 - content_h) * expansion) * scale,
        ),
        frame.background(),
        ui.px(286.0 * scale / (1.0 + 0.1 * expansion)),
    );
    let compact_name_y = 46.0 - layout.name.len() as f32 * 19.0 / 2.0;
    let name_top = compact_name_y + (layout.name_y - compact_name_y) * expansion;
    let art = ui.rect(
        x + art_x * scale,
        y + 3.0 * scale,
        art_w * scale,
        (content_h - 3.0 + expansion * (layout.art_height - content_h + 3.0)) * scale,
    );
    // Keep endpoint-sized derivatives throughout the animation, not one per frame.
    let sample_scale = scale / (1.0 + expansion * 0.1) * 1.1;
    ui.begin_scroll(art, 0);
    ui.covered_sampled(
        ui.rect(0.0, 0.0, art_w * scale, art_w * scale),
        asset(&upgrade["asset"]).as_ref(),
        ui.px(184.0 * sample_scale),
    );
    ui.end_scroll();
    ui.fill(art, 0.0, [0, 0, 0, (85.0 * (1.0 - expansion)) as u8]);
    if expansion > 0.0 {
        let (side_x, side_top) = frame.side_geometry();
        for x in [x + side_x * scale, x + (184.0 - side_x - 8.0) * scale] {
            ui.texture(
                ui.rect(
                    x,
                    y + side_top * scale,
                    8.0 * scale,
                    (182.0 + layout.height - 260.0) * scale * expansion,
                ),
                frame.side(),
                ui.px(200.0 * sample_scale),
            );
        }
    }
    let name_y = y + name_top * scale;
    let line_h = 19.0 * scale;
    for (i, line) in layout.name.iter().enumerate() {
        let width = retinue_render::painter::text_width(&ui.regular, line, 17.0 * scale * ui.scale)
            / ui.scale;
        let bounds = ui.rect(
            x + (184.0 * scale - width) / 2.0,
            name_y + i as f32 * line_h,
            width + 1.0,
            line_h,
        );
        ui.text(
            Rect {
                x: bounds.x + ui.px(scale),
                y: bounds.y + ui.px(scale),
                ..bounds
            },
            line.as_str(),
            17.0 * scale,
            [0, 0, 0, 255],
        );
        ui.text(bounds, line.as_str(), 17.0 * scale, [255; 4]);
    }
    if expansion > 0.2 {
        let effects_y = name_top + layout.effects_y - layout.name_y;
        ui.begin_scroll(
            ui.rect(x + art_x * scale, y, art_w * scale, (height - 40.0) * scale),
            0,
        );
        for (index, line) in layout.effects.iter().enumerate() {
            let width = ui.measure(line, 13.0 * scale, false);
            let mut color = frame.color();
            color[3] = ((expansion - 0.2) / 0.8 * 255.0) as u8;
            ui.text(
                ui.rect(
                    ((184.0 - 2.0 * art_x) * scale - width) / 2.0,
                    (effects_y + 15.0 * index as f32) * scale,
                    width + 1.0,
                    15.0 * scale,
                ),
                line.as_str(),
                13.0 * scale,
                color,
            );
        }
        ui.end_scroll();
    }
    let drain = upgrade["effective_drain"]
        .as_i64()
        .or_else(|| upgrade["drain"].as_i64())
        .or_else(|| upgrade["cost"].as_i64());
    let tint = match text(upgrade, "polarity_state") {
        "matched" => rgb(0x68ce5f),
        "mismatched" => rgb(0xf73533),
        _ => frame.color(),
    };
    let right = if frame == Frame::Galvanized {
        13.0
    } else if frame == Frame::Riven {
        15.0
    } else if special {
        10.0
    } else {
        4.0
    };
    if let Some(drain) = drain {
        let label = drain.to_string();
        let content = ui.measure(&label, 12.0 * scale, true) + 15.0 * scale;
        let left = x + (184.0 - right) * scale - content - 10.0 * scale;
        let top = y
            + (if frame == Frame::Galvanized {
                11.0
            } else {
                8.0
            }) * scale;
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
            frame.color(),
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
                    top + scale,
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
        origin_x + (200.0 - top_width) / 2.0 * scale,
        y - 10.0 * scale,
        top_width * scale,
        top_width * top_ratio * scale,
    );
    ui.clipped_bitmap_sampled(
        ui.rect(
            origin_x + (200.0 - top_width) / 2.0 * scale,
            y - 10.0 * scale,
            top_width * scale,
            (if special { 90.0 } else { 50.0 }) * scale,
        ),
        top,
        frame.bytes(true),
        ui.px(top_width * sample_scale),
    );
    let bottom_width = if special { 200.0 } else { 184.0 };
    let bottom_y = height - 80.0 - if special { 5.0 * expansion } else { 0.0 };
    let bottom_h = bottom_width * if special { 120.0 / 292.0 } else { 0.5 };
    ui.clipped_bitmap_sampled(
        ui.rect(
            origin_x + (200.0 - bottom_width) / 2.0 * scale,
            y + bottom_y * scale,
            bottom_width * scale,
            (if special { 85.0 } else { 80.0 }) * scale,
        ),
        ui.rect(
            origin_x + (200.0 - bottom_width) / 2.0 * scale,
            y + bottom_y * scale,
            bottom_width * scale,
            bottom_h * scale,
        ),
        frame.bytes(false),
        ui.px(bottom_width * sample_scale),
    );
    let compatibility = text(upgrade, "compat_name").to_uppercase();
    if expansion > 0.2 && !compatibility.is_empty() {
        let bounds = ui.rect(
            x + 15.0 * scale,
            y + (height - 14.0 - if special { 40.0 } else { 35.0 }) * scale,
            154.0 * scale,
            20.0 * scale,
        );
        ui.texture(bounds, frame.lower_tab(), ui.px(154.0 * sample_scale));
        let mut color = frame.color();
        color[3] = (255.0 * expansion) as u8;
        ui.aligned(
            bounds,
            compatibility,
            12.0 * scale,
            color,
            controls::Weight::Bold,
            0.5,
        );
    }
    let max = number(upgrade, "max_rank").min(10.0) as usize;
    let rank = number(upgrade, "rank") as usize;
    let bottom = if frame == Frame::Riven {
        3.0 + 5.0 * expansion
    } else if special {
        5.0 * expansion
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
    layout.height
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

fn arcane_card(ui: &mut ControlHost, upgrade: &Value, slot: &Value, x: f32, y: f32, scale: f32) {
    let x = x + 12.0 * scale;
    if upgrade.is_null() {
        ui.bitmap(
            ui.rect(
                x + 30.0 * scale,
                y + 15.0 * scale,
                100.0 * scale,
                60.0 * scale,
            ),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/arcane-frames/empty.png"
            )),
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
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/arcane-frames/",
                $n,
                ".png"
            ))
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
        retinue_render::painter::text_width(&ui.regular, name, 14.0 * scale * ui.scale) / ui.scale;
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
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/polarities/",
                $n,
                ".png"
            ))
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
    fn descriptions_grow_without_losing_properties_or_equipment_labels() {
        let short = json!({"name":"Galvanized Hell", "mod_variant":"galvanized",
            "effects":["+110% Multishot"], "compat_name":"SHOTGUN"});
        let mut long = short.clone();
        long["effects"] = json!(
            (0..16)
                .map(|i| format!("Property {i}: +30% Damage"))
                .collect::<Vec<_>>()
        );
        for scale in [1.0, 1.25, 2.0] {
            let mut ui = ControlHost::new().unwrap();
            ui.begin((2000, 2000), scale);
            let short_layout = ModText::new(&ui, &short, Frame::Galvanized);
            let long_layout = ModText::new(&ui, &long, Frame::Galvanized);
            assert_eq!(short_layout.height, 260.0);
            assert!(long_layout.height > short_layout.height);
            assert!(long_layout.name_y >= 80.0);
            assert!(
                long_layout.effects_y + long_layout.effects.len() as f32 * 15.0
                    < long_layout.height - 40.0
            );
            mod_card(&mut ui, &long, &Value::Null, "none", 0.0, 0.0, 1.0, 1.0);
            let report = ui.layout_report();
            let texts = report["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|element| element["text"].as_str())
                .collect::<Vec<_>>();
            assert!(texts.contains(&"SHOTGUN"));
            for line in &long_layout.effects {
                assert!(texts.contains(&line.as_str()));
            }
            capture(&mut ui, 200.0, long_layout.height + 24.0, "long-mod");
        }
    }

    pub(super) fn capture(ui: &mut ControlHost, width: f32, height: f32, name: &str) {
        let Some(path) = std::env::var_os("GUI_MOD_CAPTURES") else {
            return;
        };
        use retinue_render::{image::RgbaImage, painter::Painter, thumbnails::Thumbnails};
        ui.finish();
        let mut media = crate::domain::media::Media::default();
        let requests = ui.requests(&media);
        let mut thumbnails = Thumbnails::new(0, 0);
        let images = requests
            .iter()
            .map(|r| (r.key, thumbnails.prepare(r).unwrap()))
            .collect::<Vec<_>>();
        media.accept(&images, &requests);
        let (width, height) = (ui.px(width), ui.px(height));
        let mut pixels = vec![0; (width * height * 4) as usize];
        let mut painter = Painter::new(&mut pixels, width, height).unwrap();
        ui.paint(&mut painter, &media);
        painter.finish().unwrap();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        RgbaImage::from_raw(width, height, pixels)
            .unwrap()
            .save(path.join(format!("{name}-{}.png", (ui.scale * 100.0) as u32)))
            .unwrap();
    }

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
