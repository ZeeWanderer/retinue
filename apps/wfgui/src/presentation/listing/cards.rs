use super::*;

pub(super) fn quote_pill(
    ui: &mut ControlHost,
    key: String,
    r: Rect,
    label: &str,
    price: &str,
    color: [u8; 4],
) {
    let s = ui.scale;
    let (x, y, w, h) = (
        r.x as f32 / s,
        r.y as f32 / s,
        r.width as f32 / s,
        r.height as f32 / s,
    );
    ui.fill(r, h / 2.0, color);
    ui.text(ui.rect(x + 10.0, y, w - 20.0, h), label, 14.0, TEXT);
    let pw = ui.measure(price, 14.0, false);
    ui.text(
        ui.rect(x + w - 29.0 - pw, y, pw + 1.0, h),
        price,
        14.0,
        TEXT,
    );
    ui.icon(
        ui.rect(x + w - 26.0, y + (h - 16.0) / 2.0, 16.0, 16.0),
        "platinum",
    );
    ui.hit(key, r, format!("View {label} listings"), true);
}

pub(super) fn compact_count(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 10_000.0 {
        format!("{:.0}K", value / 1_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}
