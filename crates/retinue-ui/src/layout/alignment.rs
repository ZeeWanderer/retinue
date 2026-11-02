use super::{
    Available, Layout, LayoutError, LayoutSize, Proposal, Subviews,
    values::{LayoutValueKey, LayoutValues},
};
use crate::Bounds;
use std::{
    any::{TypeId, type_name},
    fmt,
    marker::PhantomData,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
impl Axis {
    fn extent(self, size: LayoutSize) -> f32 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }
    fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

pub trait AlignmentId: 'static {
    const AXIS: Axis;
    fn default_value(size: LayoutSize) -> f32;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentValue {
    pub fraction: f32,
    pub offset: f32,
}
impl AlignmentValue {
    pub const fn at(offset: f32) -> Self {
        Self {
            fraction: 0.0,
            offset,
        }
    }
    pub const fn fraction(fraction: f32) -> Self {
        Self {
            fraction,
            offset: 0.0,
        }
    }
}

pub struct GuideValue<A>(PhantomData<A>);
impl<A: AlignmentId> LayoutValueKey for GuideValue<A> {
    type Value = Option<AlignmentValue>;
    fn default_value() -> Self::Value {
        None
    }
}

#[derive(Clone, Copy)]
pub struct AlignmentGuide {
    kind: TypeId,
    name: &'static str,
    axis: Axis,
    fallback: fn(LayoutSize) -> f32,
}
impl AlignmentGuide {
    pub fn of<A: AlignmentId>() -> Self {
        Self {
            kind: TypeId::of::<GuideValue<A>>(),
            name: type_name::<A>(),
            axis: A::AXIS,
            fallback: A::default_value,
        }
    }
    pub fn name(self) -> &'static str {
        self.name
    }
    pub fn axis(self) -> Axis {
        self.axis
    }
    pub fn is<A: AlignmentId>(self) -> bool {
        self.kind == TypeId::of::<GuideValue<A>>()
    }
    pub(crate) fn explicit(self, values: &LayoutValues, size: LayoutSize) -> Option<f32> {
        let value = values
            .by_type(self.kind)?
            .downcast_ref::<Option<AlignmentValue>>()?
            .as_ref()?;
        Some(self.axis.extent(size) * value.fraction + value.offset)
    }
    pub(crate) fn fallback(self, size: LayoutSize) -> f32 {
        (self.fallback)(size)
    }
}
impl PartialEq for AlignmentGuide {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}
impl Eq for AlignmentGuide {}
impl fmt::Debug for AlignmentGuide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

pub struct HorizontalCenter;
impl AlignmentId for HorizontalCenter {
    const AXIS: Axis = Axis::Horizontal;
    fn default_value(size: LayoutSize) -> f32 {
        size.width / 2.0
    }
}
pub struct VerticalCenter;
impl AlignmentId for VerticalCenter {
    const AXIS: Axis = Axis::Vertical;
    fn default_value(size: LayoutSize) -> f32 {
        size.height / 2.0
    }
}

/// Content-hugging stack with named cross-axis alignment; no flex grow/shrink.
#[derive(Clone, Debug, PartialEq)]
pub struct AlignedStack {
    axis: Axis,
    guide: AlignmentGuide,
    gap: f32,
}

#[derive(Default)]
pub struct StackCache {
    proposal: Option<Proposal>,
    children: Vec<(LayoutSize, f32)>,
    size: LayoutSize,
    before: f32,
}

impl AlignedStack {
    pub fn row<A: AlignmentId>(gap: f32) -> Self {
        Self {
            axis: Axis::Horizontal,
            guide: AlignmentGuide::of::<A>(),
            gap,
        }
    }
    pub fn column<A: AlignmentId>(gap: f32) -> Self {
        Self {
            axis: Axis::Vertical,
            guide: AlignmentGuide::of::<A>(),
            gap,
        }
    }
    pub fn guide(&self) -> AlignmentGuide {
        self.guide
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn gap(&self) -> f32 {
        self.gap
    }

    fn plan(
        &self,
        mut proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut StackCache,
    ) -> Result<(), LayoutError> {
        if !self.gap.is_finite() || self.gap < 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        if self.guide.axis != self.axis.other() {
            return Err(LayoutError::AlignmentAxis);
        }
        match self.axis {
            Axis::Horizontal => proposal.width = Available::Ideal,
            Axis::Vertical => proposal.height = Available::Ideal,
        }
        if cache.proposal == Some(proposal) {
            return Ok(());
        }
        cache.proposal = None;
        cache.children.clear();
        let (mut main, mut before, mut after) = (0.0_f32, 0.0_f32, 0.0_f32);
        for index in 0..children.len() {
            let size = children.measure(index, proposal)?;
            let guide = children.alignment_for(index, self.guide, proposal)?;
            main += self.axis.extent(size);
            before = before.max(guide);
            after = after.max(self.guide.axis.extent(size) - guide);
            cache.children.push((size, guide));
        }
        main += self.gap * children.len().saturating_sub(1) as f32;
        cache.size = match self.axis {
            Axis::Horizontal => LayoutSize::new(main, before + after),
            Axis::Vertical => LayoutSize::new(before + after, main),
        };
        cache.before = before;
        cache.proposal = Some(proposal);
        Ok(())
    }

    fn alignment_position(&self, bounds: Bounds, cache: &StackCache) -> f32 {
        let (start, extent) = match self.guide.axis {
            Axis::Horizontal => (bounds.x, bounds.width),
            Axis::Vertical => (bounds.y, bounds.height),
        };
        start + (extent - self.guide.axis.extent(cache.size)) / 2.0 + cache.before
    }
}

impl Layout for AlignedStack {
    type Cache = StackCache;
    fn update_cache(
        &self,
        cache: &mut StackCache,
        _: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        cache.proposal = None;
        Ok(())
    }
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut StackCache,
    ) -> Result<LayoutSize, LayoutError> {
        self.plan(proposal, children, cache)?;
        Ok(cache.size)
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut StackCache,
    ) -> Result<(), LayoutError> {
        self.plan(
            Proposal::bounded(bounds.width, bounds.height),
            children,
            cache,
        )?;
        let line = self.alignment_position(bounds, cache);
        let mut main = match self.axis {
            Axis::Horizontal => bounds.x,
            Axis::Vertical => bounds.y,
        };
        for (index, &(size, guide)) in cache.children.iter().enumerate() {
            let (x, y) = match self.axis {
                Axis::Horizontal => (main, line - guide),
                Axis::Vertical => (line - guide, main),
            };
            children.place(index, Bounds::new(x, y, size.width, size.height))?;
            main += self.axis.extent(size) + self.gap;
        }
        Ok(())
    }
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut StackCache,
    ) -> Result<Option<f32>, LayoutError> {
        if guide != self.guide {
            return Ok(None);
        }
        self.plan(
            Proposal::bounded(bounds.width, bounds.height),
            children,
            cache,
        )?;
        Ok(Some(self.alignment_position(bounds, cache)))
    }
}
