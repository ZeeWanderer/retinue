use retinue_render::Color;
use retinue_ui::{authoring::*, motion::ReduceMotion, scene::LoadingFrame, timeline::Schedule};
use std::{time::Duration, time::UNIX_EPOCH};

#[derive(PartialEq)]
pub(crate) struct LoadingPlaceholder {
    pub color: Color,
    pub radius: f32,
}

#[view]
impl View for LoadingPlaceholder {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl Body {
        let reduced = cx.environment().get::<ReduceMotion>();
        let elapsed = if reduced {
            Duration::ZERO
        } else {
            cx.time(Schedule::WallClock {
                interval: Duration::from_millis(16),
            })
            .wall_time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
        };
        Rectangle(LoadingFrame::sample(elapsed, reduced).placeholder_color(self.color))
            .corner_radius(self.radius)
    }
}

#[cfg(test)]
mod tests;
