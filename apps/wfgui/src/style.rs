use crate::settings::rgb;
use retinue_ui::{
    Rect,
    painter::{Painter, RasterImage},
};

pub const TEXT: [u8; 4] = rgb(0xffffff);
pub const MUTED: [u8; 4] = rgb(0x9da8bd);
pub const PANEL: [u8; 4] = rgb(0x171e30);
pub const CARD: [u8; 4] = rgb(0x20283e);
pub const CARD_HOVER: [u8; 4] = rgb(0x272f47);
pub const ACCENT: [u8; 4] = rgb(0x6332b5);
pub const FOCUS: [u8; 4] = rgb(0xaa9af9);
pub const GOOD: [u8; 4] = rgb(0x83dbab);
pub const BAD: [u8; 4] = rgb(0xff8796);
pub const POPOVER: [u8; 4] = rgb(0x30394e);
pub const MENU_HOVER: [u8; 4] = rgb(0x46516b);
pub const COMPONENT_READY: [u8; 4] = alpha(0x6e9b68, 120);
pub const COMPONENT_EMPTY: [u8; 4] = alpha(0xffffff, 0x12);
pub const COMPONENT_GLOW: [u8; 4] = rgb(0x46d234);
pub const OWNED_TEXT: [u8; 4] = rgb(0xabf5ab);
pub const SEARCH_HIGHLIGHT: [u8; 4] = rgb(0xf0c95a);
pub const SUMMARY_HEADER: [u8; 4] = rgb(0x313a58);
pub const SUMMARY_PERCENT: [u8; 4] = rgb(0xb59dff);
pub const INSET_PANEL: [u8; 4] = rgb(0x101623);
pub const SELL_QUOTE: [u8; 4] = rgb(0x6f2e43);
pub const BUY_QUOTE: [u8; 4] = rgb(0x1e5b50);
pub const UNAVAILABLE_QUOTE: [u8; 4] = rgb(0x390d0d);
pub const OWNED_CARD: [u8; 4] = alpha(0x315e32, 107);
pub const OWNED_CARD_HOVER: [u8; 4] = alpha(0x3d723e, 107);
pub const OWNED_EDGE: [u8; 4] = alpha(0x006a03, 168);
pub const OWNED_EDGE_HOVER: [u8; 4] = alpha(0x008004, 150);

pub const fn alpha(color: u32, alpha: u8) -> [u8; 4] {
    let mut color = rgb(color);
    color[3] = alpha;
    color
}

pub fn component_background(p: &mut Painter<'_>, r: Rect, ready: bool) {
    p.fill_rounded_rect(
        r.x,
        r.y,
        r.width,
        r.height,
        r.width / 2,
        if ready {
            COMPONENT_READY
        } else {
            COMPONENT_EMPTY
        },
    );
}

pub fn component_glow(diameter: u32, scale: f32) -> Result<RasterImage, String> {
    RasterImage::render(diameter, diameter, |p| {
        component_highlight(
            p,
            Rect {
                x: 0,
                y: 0,
                width: diameter,
                height: diameter,
            },
            None,
            scale,
        )
    })
    .map_err(|error| error.to_string())
}

pub fn component_highlight(p: &mut Painter<'_>, r: Rect, cached: Option<&RasterImage>, scale: f32) {
    if let Some(glow) = cached {
        p.draw_image(glow, r.x, r.y, r.width, r.height);
    } else {
        p.inset_shadow(r, r.width / 2, 4.0 * scale, scale, COMPONENT_GLOW);
    }
}

pub fn focus_ring(painter: &mut Painter<'_>, bounds: Rect, scale: f32) {
    focus_outline(painter, bounds, scale, (6.0 * scale).round() as u32);
}

pub fn focus_capsule(painter: &mut Painter<'_>, bounds: Rect, scale: f32) {
    focus_outline(painter, bounds, scale, bounds.height / 2);
}

fn focus_outline(painter: &mut Painter<'_>, bounds: Rect, scale: f32, radius: u32) {
    painter.outline(bounds, radius, (2.0 * scale).round().max(1.0) as u32, FOCUS);
}
