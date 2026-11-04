use crate::presentation::settings::rgb;
use retinue_render::{Bounds, Color, Command};
use retinue_ui::{Rect, drawing::PixelCanvas as Canvas};
use std::rc::Rc;

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
pub const MENU_EDGE: [u8; 4] = rgb(0x596176);
pub const FILTER_EDGE: [u8; 4] = rgb(0x9097a8);

pub fn filter_color(selected: bool, hovered: bool) -> [u8; 4] {
    if selected {
        rgb(0x5923cb)
    } else if hovered {
        rgb(0x5e44af)
    } else {
        CARD
    }
}
pub const NAV_TEXT: [u8; 4] = rgb(0x8a8c95);
pub const NAV_HOVER: [u8; 4] = rgb(0xccccce);
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

pub fn component_glow(bounds: Bounds, scale: f32) -> Command {
    Command::Translate {
        offset: (bounds.x, bounds.y),
        commands: Rc::new(vec![Command::InsetShadow {
            bounds: Bounds::new(0.0, 0.0, bounds.width, bounds.height),
            radius: ((bounds.width * scale).round() as u32 / 2) as f32 / scale,
            blur: 4.0,
            spread: 1.0,
            color: Color::from_bgra(COMPONENT_GLOW),
        }]),
    }
}

pub fn focus_ring(painter: &mut Canvas, bounds: Rect, scale: f32) {
    focus_outline(painter, bounds, scale, (6.0 * scale).round() as u32);
}

pub fn focus_capsule(painter: &mut Canvas, bounds: Rect, scale: f32) {
    focus_outline(painter, bounds, scale, bounds.height / 2);
}

fn focus_outline(painter: &mut Canvas, bounds: Rect, scale: f32, radius: u32) {
    painter.outline(bounds, radius, (2.0 * scale).round().max(1.0) as u32, FOCUS);
}
