mod algorithm;
pub mod alignment;
pub mod anchors;
pub mod grid;
mod insets;
pub(crate) mod layered;
pub mod positioned;
pub mod rendering;
pub(crate) mod scroll;
pub mod values;
pub use algorithm::{LayoutAlgorithm, LayoutResult};
pub(crate) use algorithm::{LayoutCache, SharedLayout, WorkingCache};
pub use insets::Insets;

use crate::scene::layout::LayoutContext;
use crate::{Bounds, Key};
use alignment::{AlignmentGuide, AlignmentId};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutSize {
    pub width: f32,
    pub height: f32,
}

impl LayoutSize {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    pub(crate) fn valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Available content space; a proposal does not override a child's size constraints.
pub enum Available {
    MinContent,
    Ideal,
    Limit(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proposal {
    pub width: Available,
    pub height: Available,
}

impl Proposal {
    pub const IDEAL: Self = Self {
        width: Available::Ideal,
        height: Available::Ideal,
    };
    pub fn bounded(width: f32, height: f32) -> Self {
        Self {
            width: Available::Limit(width),
            height: Available::Limit(height),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    Text(retinue_render::text::TextError),
    MissingChild(Key),
    InvalidChild(usize),
    InvalidGeometry,
    AlignmentAxis,
    ChildFailed(Key),
    PlacementDuringMeasurement,
    DuplicatePlacement(Key),
    UnplacedChild(Key),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutFailure {
    pub container: Key,
    pub error: LayoutError,
}

pub struct Subviews<'a, 'tree> {
    pub(super) context: &'a mut LayoutContext<'tree>,
}

impl Subviews<'_, '_> {
    pub fn len(&self) -> usize {
        self.context.child_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn key(&self, index: usize) -> Option<Key> {
        self.context.child_key(index)
    }
    pub fn index(&self, key: Key) -> Option<usize> {
        (0..self.len()).find(|&index| self.key(index) == Some(key))
    }
    pub fn value<K: values::LayoutValueKey>(&self, index: usize) -> Result<K::Value, LayoutError> {
        Ok(self.context.child_values(index)?.get::<K>())
    }
    pub fn measure(&mut self, index: usize, proposal: Proposal) -> Result<LayoutSize, LayoutError> {
        self.context.measure_child(index, proposal)
    }
    pub fn place(&mut self, index: usize, bounds: Bounds) -> Result<(), LayoutError> {
        self.context.place_child(index, bounds)
    }
    /// Place independent children once their rectangles are known. Measurement
    /// remains available through `measure` for layouts with child dependencies.
    pub fn place_each(
        &mut self,
        bounds: impl FnMut(usize, Key) -> Result<Bounds, LayoutError>,
    ) -> Result<(), LayoutError> {
        self.context.place_each(bounds)
    }
    pub fn alignment<A: AlignmentId>(
        &mut self,
        index: usize,
        proposal: Proposal,
    ) -> Result<f32, LayoutError> {
        self.alignment_for(index, AlignmentGuide::of::<A>(), proposal)
    }
    pub fn alignment_for(
        &mut self,
        index: usize,
        guide: AlignmentGuide,
        proposal: Proposal,
    ) -> Result<f32, LayoutError> {
        self.context.child_alignment(index, guide, proposal)
    }
}

/// Arrange declared children in logical coordinates. Equality covers all layout inputs.
pub trait Layout: PartialEq + 'static {
    type Cache: Default + 'static;

    fn render_context(&self, _: &Subviews<'_, '_>) -> retinue_render::OptimizationContext {
        retinue_render::OptimizationContext::default()
    }

    /// Called for changed parameters or children, not for each new size proposal.
    fn update_cache(
        &self,
        cache: &mut Self::Cache,
        _: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        *cache = Self::Cache::default();
        Ok(())
    }

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut Self::Cache,
    ) -> Result<LayoutSize, LayoutError>;
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut Self::Cache,
    ) -> Result<(), LayoutError>;

    /// Optional upward export, in this node's border-box coordinate space.
    fn explicit_alignment(
        &self,
        _: AlignmentGuide,
        _: Bounds,
        _: &mut Subviews<'_, '_>,
        _: &mut Self::Cache,
    ) -> Result<Option<f32>, LayoutError> {
        Ok(None)
    }
}
