use super::Insets;
use crate::state::KeyedOrder;
use crate::{
    Bounds, Key,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews},
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniformGrid {
    columns: usize,
    cell: LayoutSize,
    gap: LayoutSize,
    padding: Insets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollAnchor {
    pub item: Key,
    pub displacement: f32,
}

impl UniformGrid {
    pub fn adaptive(
        width: f32,
        minimum: f32,
        height: f32,
        gap: LayoutSize,
        padding: Insets,
    ) -> Result<Self, LayoutError> {
        if !width.is_finite() || width <= 0.0 || !minimum.is_finite() || minimum <= 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        Self::new(1, LayoutSize::new(minimum, height), gap, padding)?;
        let available = width as f64 - padding.left as f64 - padding.right as f64;
        if available <= 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        let count = ((available + gap.width as f64) / (minimum as f64 + gap.width as f64))
            .floor()
            .max(1.0);
        if count >= usize::MAX as f64 {
            return Err(LayoutError::InvalidGeometry);
        }
        let cell_width = (available - (count - 1.0) * gap.width as f64) / count;
        Self::new(
            count as usize,
            LayoutSize::new(cell_width as f32, height),
            gap,
            padding,
        )
    }

    pub fn new(
        columns: usize,
        cell: LayoutSize,
        gap: LayoutSize,
        padding: Insets,
    ) -> Result<Self, LayoutError> {
        if columns == 0
            || !cell.valid()
            || cell.width <= 0.0
            || cell.height <= 0.0
            || !gap.valid()
            || [padding.left, padding.top, padding.right, padding.bottom]
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let grid = Self {
            columns,
            cell,
            gap,
            padding,
        };
        grid.extent(columns)?;
        Ok(grid)
    }

    pub fn columns(self) -> usize {
        self.columns
    }
    pub fn cell(self) -> LayoutSize {
        self.cell
    }
    pub fn gap(self) -> LayoutSize {
        self.gap
    }
    pub fn padding(self) -> Insets {
        self.padding
    }

    pub fn extent(self, count: usize) -> Result<LayoutSize, LayoutError> {
        let rows = count.div_ceil(self.columns);
        let size = LayoutSize::new(
            (self.padding.left as f64
                + self.padding.right as f64
                + self.columns as f64 * self.cell.width as f64
                + (self.columns - 1) as f64 * self.gap.width as f64) as f32,
            (self.padding.top as f64
                + self.padding.bottom as f64
                + rows as f64 * self.cell.height as f64
                + rows.saturating_sub(1) as f64 * self.gap.height as f64) as f32,
        );
        if size.valid() {
            Ok(size)
        } else {
            Err(LayoutError::InvalidGeometry)
        }
    }

    pub fn bounds(self, index: usize) -> Result<Bounds, LayoutError> {
        let x = self.padding.left as f64
            + (index % self.columns) as f64 * (self.cell.width as f64 + self.gap.width as f64);
        let y = self.padding.top as f64
            + (index / self.columns) as f64 * (self.cell.height as f64 + self.gap.height as f64);
        if !(x + self.cell.width as f64 <= f32::MAX as f64
            && y + self.cell.height as f64 <= f32::MAX as f64)
        {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Bounds::new(
            x as f32,
            y as f32,
            self.cell.width,
            self.cell.height,
        ))
    }

    pub fn visible(
        self,
        count: usize,
        offset: f32,
        height: f32,
        overscan_rows: usize,
    ) -> Result<Range<usize>, LayoutError> {
        if !offset.is_finite() || offset < 0.0 || !height.is_finite() || height < 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        self.extent(count)?;
        if count == 0 || height == 0.0 {
            return Ok(0..0);
        }
        let rows = count.div_ceil(self.columns);
        let stride = self.cell.height as f64 + self.gap.height as f64;
        let top = offset as f64 - self.padding.top as f64;
        let bottom = top + height as f64;
        let first = ((top - self.cell.height as f64) / stride).floor() + 1.0;
        let first = (first.max(0.0) as usize).min(rows);
        let end = ((bottom / stride).ceil().max(0.0) as usize).min(rows);
        if first >= end {
            let at = first.saturating_mul(self.columns).min(count);
            return Ok(at..at);
        }
        Ok(first
            .saturating_sub(overscan_rows)
            .saturating_mul(self.columns)
            .min(count)
            ..end
                .saturating_add(overscan_rows)
                .min(rows)
                .saturating_mul(self.columns)
                .min(count))
    }

    pub fn anchor(self, order: &KeyedOrder, offset: f32) -> Option<ScrollAnchor> {
        if !offset.is_finite() || offset < 0.0 {
            return None;
        }
        let row = (((offset as f64 - self.padding.top as f64)
            / (self.cell.height as f64 + self.gap.height as f64))
            .floor()
            .max(0.0) as usize)
            .min(order.len().div_ceil(self.columns).saturating_sub(1));
        let index = row.checked_mul(self.columns)?;
        Some(ScrollAnchor {
            item: *order.keys().get(index)?,
            displacement: offset - self.bounds(index).ok()?.y,
        })
    }

    pub fn restore(self, anchor: ScrollAnchor, order: &KeyedOrder, height: f32) -> Option<f32> {
        if !height.is_finite() || height < 0.0 || !anchor.displacement.is_finite() {
            return None;
        }
        let y = self.bounds(order.index(anchor.item)?).ok()?.y;
        let maximum = (self.extent(order.len()).ok()?.height - height).max(0.0);
        Some((y + anchor.displacement).clamp(0.0, maximum))
    }

    pub fn arrange(self, order: KeyedOrder) -> GridLayout {
        GridLayout { grid: self, order }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridLayout {
    grid: UniformGrid,
    order: KeyedOrder,
}

impl Layout for GridLayout {
    type Cache = ();

    fn render_context(&self, children: &Subviews<'_, '_>) -> retinue_render::OptimizationContext {
        use super::rendering::{ChildPartition, Collection};
        let mut partitions: Vec<ChildPartition> = Vec::new();
        let mut previous = None;
        for child in 0..children.len() {
            let Some(index) = children.key(child).and_then(|key| self.order.index(key)) else {
                return Default::default();
            };
            let row = index / self.grid.columns;
            if previous == Some(row) {
                partitions.last_mut().unwrap().children.end = child + 1;
            } else {
                partitions.push(ChildPartition {
                    key: self.order.keys()[row * self.grid.columns],
                    children: child..child + 1,
                });
                previous = Some(row);
            }
        }
        retinue_render::OptimizationContext::default().with(Collection {
            partitions,
            ..Collection::default()
        })
    }

    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        self.grid.extent(self.order.len())
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        for child in 0..children.len() {
            let key = children.key(child).unwrap();
            let index = self
                .order
                .index(key)
                .ok_or(LayoutError::MissingChild(key))?;
            let mut cell = self.grid.bounds(index)?;
            cell.x += bounds.x;
            cell.y += bounds.y;
            children.place(child, cell)?;
        }
        Ok(())
    }
}
