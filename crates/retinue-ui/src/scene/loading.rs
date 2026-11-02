use super::{Bounds, Color, Command};
use std::time::Duration;

/// One host-sampled phase shared by every loading shape in a frame.
pub struct LoadingFrame(f32);

impl LoadingFrame {
    pub fn sample(elapsed: Duration, reduced_motion: bool) -> Self {
        Self(if reduced_motion {
            0.5
        } else {
            let phase = (elapsed.as_secs_f64() % 1.4) / 1.4;
            (0.5 - 0.5 * (phase * std::f64::consts::TAU).cos()) as f32
        })
    }

    pub fn placeholder(&self, bounds: Bounds, radius: f32, color: Color) -> Command {
        Command::Fill {
            bounds,
            radius,
            color: self.placeholder_color(color),
        }
    }

    pub fn placeholder_color(&self, color: Color) -> Color {
        let mut color = color.bgra();
        color[3] = (color[3] as f32 * (0.35 + 0.25 * self.0)).round() as u8;
        Color::from_bgra(color)
    }

    pub fn progress(&self, bounds: Bounds, color: Color) -> Command {
        Command::Fill {
            bounds: Bounds::new(
                bounds.x + bounds.width * 0.75 * self.0,
                bounds.y,
                bounds.width * 0.25,
                bounds.height,
            ),
            radius: bounds.height / 2.0,
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_phase_stays_in_bounds_and_reduced_motion_is_static() {
        let bounds = Bounds::new(12.0, 8.0, 300.0, 3.0);
        let color = Color::rgba(120, 140, 160, 100);
        let still = LoadingFrame::sample(Duration::ZERO, true).placeholder(bounds, 2.0, color);
        for millis in [0, 16, 350, 700, 1050, 1400, 140000] {
            let elapsed = Duration::from_millis(millis);
            let frame = LoadingFrame::sample(elapsed, false);
            let bar = frame.progress(bounds, color).bounds();
            assert!(bar.x >= bounds.x && bar.x + bar.width <= bounds.x + bounds.width);
            assert!(LoadingFrame::sample(elapsed, true).placeholder(bounds, 2.0, color) == still);
            let Command::Fill { color, .. } = frame.placeholder(bounds, 2.0, color) else {
                unreachable!()
            };
            assert!((35..=60).contains(&color.bgra()[3]));
        }
    }
}
