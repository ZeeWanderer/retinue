use crate::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Inactive,
    Content,
    Dismissed,
}

#[derive(Default)]
pub struct Popover {
    pub bounds: Rect,
    open: bool,
}

impl Popover {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn layout(&mut self, anchor: Rect, viewport: Rect, size: (u32, u32), gap: u32) {
        let width = size.0.min(viewport.width);
        let bottom = viewport.y.saturating_add(viewport.height);
        let below = anchor
            .y
            .saturating_add(anchor.height)
            .saturating_add(gap)
            .clamp(viewport.y, bottom);
        let above = anchor.y.saturating_sub(gap).clamp(viewport.y, bottom);
        let room_below = bottom - below;
        let room_above = above - viewport.y;
        let use_below = size.1 <= room_below || room_below >= room_above;
        let height = size.1.min(if use_below { room_below } else { room_above });
        self.bounds = Rect {
            x: anchor
                .x
                .saturating_add(anchor.width)
                .saturating_sub(width)
                .clamp(
                    viewport.x,
                    viewport.x.saturating_add(viewport.width - width),
                ),
            y: if use_below { below } else { above - height },
            width,
            height,
        };
    }

    /// A dismissing press belongs to the popover, not to the control underneath.
    pub fn press(&mut self, point: (f64, f64)) -> Press {
        if !self.open {
            Press::Inactive
        } else if self.bounds.contains(point.0, point.1) {
            Press::Content
        } else {
            self.close();
            Press::Dismissed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_and_clamps_at_edges_and_narrow_viewports() {
        for scale in [1, 2, 3] {
            let viewport = Rect {
                x: 10,
                y: 10,
                width: 400,
                height: 300,
            }
            .scaled(scale);
            for anchor in [
                Rect {
                    x: 380,
                    y: 20,
                    width: 20,
                    height: 20,
                },
                Rect {
                    x: 20,
                    y: 280,
                    width: 20,
                    height: 20,
                },
                Rect {
                    x: 500,
                    y: 400,
                    width: 20,
                    height: 20,
                },
            ] {
                let mut popup = Popover::default();
                popup.layout(
                    anchor.scaled(scale),
                    viewport,
                    (600 * scale, 200 * scale),
                    8 * scale,
                );
                assert_eq!(popup.bounds.intersection(viewport), popup.bounds);
                assert_eq!(popup.bounds.width, viewport.width);
                assert!(popup.bounds.height > 0);
            }
        }
        let mut popup = Popover::default();
        popup.layout(Rect::default(), Rect::default(), (200, 300), 8);
        assert!(popup.bounds.is_empty());
    }

    #[test]
    fn outside_press_dismisses_without_falling_through() {
        let mut popup = Popover::default();
        popup.bounds = Rect {
            x: 20,
            y: 30,
            width: 100,
            height: 80,
        };
        assert_eq!(popup.press((0.0, 0.0)), Press::Inactive);
        popup.open();
        assert_eq!(popup.press((40.0, 50.0)), Press::Content);
        assert!(popup.is_open());
        assert_eq!(popup.press((0.0, 0.0)), Press::Dismissed);
        assert!(!popup.is_open());
    }
}
