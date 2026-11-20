use crate::transition::Interpolate;
use retinue_render::Color;

impl Interpolate for Color {
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        let from = from.bgra();
        let to = to.bgra();
        let alpha =
            f32::interpolate(&(from[3] as f32), &(to[3] as f32), progress).clamp(0.0, 255.0);
        let mut value = [0; 4];
        for i in 0..3 {
            let color = f32::interpolate(
                &(from[i] as f32 * from[3] as f32),
                &(to[i] as f32 * to[3] as f32),
                progress,
            );
            value[i] = if alpha > 0.0 {
                (color / alpha).round().clamp(0.0, 255.0) as u8
            } else {
                0
            };
        }
        value[3] = alpha.round() as u8;
        Color::from_bgra(value)
    }
}
