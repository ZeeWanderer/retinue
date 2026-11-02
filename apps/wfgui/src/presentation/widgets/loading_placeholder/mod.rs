use retinue_ui::{
    drawing::CanvasContent, drawing::DrawingContext, drawing::Typography, prelude::*,
    scene::LoadingFrame, view::Primitive, view::PrimitiveOutput,
};
use std::{time::Duration, time::UNIX_EPOCH};

#[derive(PartialEq)]
pub(crate) struct LoadingPlaceholder {
    pub color: Color,
    pub radius: f32,
}

impl Primitive for LoadingPlaceholder {
    fn resolve(&self, cx: &mut ViewContext<'_>) -> PrimitiveOutput {
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
        PrimitiveOutput::new(
            PlaceholderPaint {
                color: LoadingFrame::sample(elapsed, reduced).placeholder_color(self.color),
                radius: self.radius,
            },
            cx.environment().get::<Typography>(),
        )
        .fill_available()
    }
}

impl IntoElement for LoadingPlaceholder {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

#[derive(PartialEq)]
struct PlaceholderPaint {
    color: Color,
    radius: f32,
}

impl CanvasContent for PlaceholderPaint {
    fn draw(&self, cx: &mut DrawingContext) {
        cx.fill(cx.bounds, self.radius, self.color);
    }
}

#[cfg(test)]
mod tests;
