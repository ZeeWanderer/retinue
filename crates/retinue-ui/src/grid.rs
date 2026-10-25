use std::ops::Range;

/// Uniform tracks: item count never determines layout or paint cost per viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub columns: usize,
    pub width: u32,
    pub height: u32,
    pub gap: u32,
}

impl Grid {
    pub fn new(available: u32, minimum: u32, height: u32, gap: u32) -> Self {
        let columns = ((available + gap) / (minimum + gap).max(1)).max(1) as usize;
        Self {
            columns,
            width: available.saturating_sub(gap * (columns - 1) as u32) / columns as u32,
            height,
            gap,
        }
    }

    pub fn content_height(&self, count: usize) -> u32 {
        (count.div_ceil(self.columns) as u32 * self.stride()).saturating_sub(self.gap)
    }

    pub fn stride(&self) -> u32 {
        (self.height + self.gap).max(1)
    }

    pub fn visible(&self, count: usize, offset: u32, viewport: u32, overscan: u32) -> Range<usize> {
        let first = (offset / self.stride()).saturating_sub(overscan) as usize * self.columns;
        let last = ((offset.saturating_add(viewport)).div_ceil(self.stride()) + overscan) as usize
            * self.columns;
        first.min(count)..last.min(count)
    }

    pub fn origin(&self, index: usize) -> (u32, u32) {
        (
            (index % self.columns) as u32 * (self.width + self.gap),
            (index / self.columns) as u32 * self.stride(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_work_is_viewport_bounded_and_columns_are_flexible() {
        let grid = Grid::new(1800, 270, 142, 8);
        assert_eq!(grid.columns, 6);
        assert!(grid.visible(100_000, 3000, 900, 1).len() <= 54);
        assert_eq!(grid.visible(0, 100, 900, 1), 0..0);
        assert_eq!(Grid::new(100, 270, 142, 8).columns, 1);
        assert_eq!(grid.content_height(6), 142);
        assert_eq!(grid.visible(100, 150, 150, 0), 6..12);
    }
}
