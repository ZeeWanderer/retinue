use crate::transition::ScalarTransition;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Padding {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    pub bounds: crate::Rect,
    pub padding: Padding,
}

impl Viewport {
    pub fn content_width(self) -> u32 {
        self.bounds
            .width
            .saturating_sub(self.padding.left.saturating_add(self.padding.right))
    }

    pub fn extent(self, content_height: u32) -> u32 {
        self.padding
            .top
            .saturating_add(content_height)
            .saturating_add(self.padding.bottom)
    }

    pub fn translation(self, offset: u32) -> (i32, i32) {
        (
            (self.bounds.x + self.padding.left) as i32,
            (self.bounds.y + self.padding.top) as i32 - offset as i32,
        )
    }

    pub fn visible_content(self, offset: u32) -> (u32, u32) {
        (
            offset.saturating_sub(self.padding.top),
            self.bounds
                .height
                .saturating_sub(self.padding.top.saturating_sub(offset)),
        )
    }
}

#[derive(Default)]
pub struct Scroll {
    position: ScalarTransition,
    target: f32,
    maximum: f32,
}

impl Scroll {
    pub fn resize(&mut self, content: f32, viewport: f32, now: Instant) {
        self.maximum = (content - viewport).max(0.0);
        if self.target > self.maximum {
            self.to(self.maximum, false, now);
        }
    }

    pub fn offset(&self, now: Instant) -> f32 {
        self.position.value(now).clamp(0.0, self.maximum)
    }
    pub fn maximum(&self) -> f32 {
        self.maximum
    }

    pub fn by(&mut self, delta: f32, smooth: bool, now: Instant) {
        self.to(self.target + delta, smooth, now);
    }

    pub fn to(&mut self, position: f32, smooth: bool, now: Instant) {
        if !position.is_finite() {
            return;
        }
        self.target = position.clamp(0.0, self.maximum);
        self.position.set(
            self.target,
            now,
            if smooth {
                Duration::from_millis(120)
            } else {
                Duration::ZERO
            },
        );
    }

    pub fn reveal(&mut self, top: f32, bottom: f32, viewport: f32, now: Instant) {
        if top < self.target {
            self.to(top, false, now);
        } else if bottom > self.target + viewport {
            self.to(bottom - viewport, false, now);
        }
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.position.deadline(now, Duration::from_millis(16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_scrolls_with_content_instead_of_insetting_the_clip() {
        let viewport = Viewport {
            bounds: crate::Rect {
                x: 10,
                y: 20,
                width: 100,
                height: 80,
            },
            padding: Padding {
                top: 12,
                right: 7,
                bottom: 16,
                left: 5,
            },
        };
        assert_eq!(viewport.content_width(), 88);
        assert_eq!(viewport.extent(200), 228);
        assert_eq!(viewport.translation(0), (15, 32));
        assert_eq!(viewport.visible_content(0), (0, 68));
        assert_eq!(viewport.translation(50), (15, -18));
        assert_eq!(viewport.visible_content(50), (38, 80));
        let now = Instant::now();
        let mut scroll = Scroll::default();
        scroll.resize(
            viewport.extent(200) as f32,
            viewport.bounds.height as f32,
            now,
        );
        scroll.to(f32::MAX, false, now);
        assert_eq!(scroll.maximum(), 148.0);
        let (_, y) = viewport.translation(scroll.offset(now) as u32);
        assert_eq!(y + 200 + viewport.padding.bottom as i32, 100);

        let tiny = Viewport {
            bounds: crate::Rect {
                width: 3,
                height: 4,
                ..Default::default()
            },
            ..viewport
        };
        assert_eq!(tiny.content_width(), 0);
        assert_eq!(tiny.visible_content(0), (0, 0));
        assert_eq!(tiny.extent(0), 28);
    }

    #[test]
    fn scroll_clamps_reveals_and_stops_waking_when_settled() {
        let now = Instant::now();
        let mut scroll = Scroll::default();
        scroll.resize(1200.0, 400.0, now);
        scroll.by(300.0, true, now);
        assert_eq!(scroll.offset(now), 0.0);
        assert!(scroll.offset(now + Duration::from_millis(60)) > 200.0);
        let settled = now + Duration::from_secs(1);
        assert_eq!(scroll.offset(settled), 300.0);
        assert_eq!(scroll.deadline(settled), None);
        scroll.reveal(900.0, 1000.0, 400.0, settled);
        assert_eq!(scroll.offset(settled), 600.0);
        scroll.resize(500.0, 400.0, settled);
        assert_eq!(scroll.offset(settled), 100.0);
        scroll.to(f32::NAN, false, settled);
        assert_eq!(scroll.offset(settled), 100.0);
    }
}
