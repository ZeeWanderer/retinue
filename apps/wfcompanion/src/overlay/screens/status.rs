use fontdue::Font;

use retinue_overlay::painter::Painter;

pub(in crate::overlay) const WIDTH: u32 = 420;
pub(in crate::overlay) const HEIGHT: u32 = 90;
pub(in crate::overlay) const INSET: u32 = 16;

pub(in crate::overlay) struct View<'a> {
    pub(in crate::overlay) scale: u32,
    pub(in crate::overlay) origin: (u32, u32),
    pub(in crate::overlay) daemon: &'a str,
    pub(in crate::overlay) player: &'a str,
    pub(in crate::overlay) detail: &'a str,
}

pub(super) fn draw(painter: &mut Painter<'_>, font: &Font, view: View<'_>) {
    let x = view.origin.0 * view.scale;
    let y = view.origin.1 * view.scale;
    painter.fill_rounded_rect(
        x,
        y,
        WIDTH * view.scale,
        HEIGHT * view.scale,
        5 * view.scale,
        [18, 14, 12, 232],
    );
    painter.draw_text(
        font,
        x + 10 * view.scale,
        y + 8 * view.scale,
        13.0 * view.scale as f32,
        view.daemon,
        [255, 255, 255, 255],
    );
    painter.draw_text(
        font,
        x + 10 * view.scale,
        y + 32 * view.scale,
        16.0 * view.scale as f32,
        view.player,
        [255, 215, 180, 255],
    );
    painter.draw_text(
        font,
        x + 10 * view.scale,
        y + 64 * view.scale,
        11.0 * view.scale as f32,
        view.detail,
        [175, 175, 175, 255],
    );
}
