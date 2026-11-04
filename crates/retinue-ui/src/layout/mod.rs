pub mod alignment;
pub mod anchors;
pub mod grid;
mod insets;
pub mod positioned;
pub mod rendering;
pub mod values;
pub use insets::Insets;

use crate::scene::layout::LayoutContext;
use crate::{Bounds, Key};
use alignment::{AlignmentGuide, AlignmentId};
use std::{
    any::{Any, TypeId},
    fmt,
    rc::Rc,
};

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
    MissingChild(Key),
    InvalidChild(usize),
    InvalidGeometry,
    AlignmentAxis,
    ChildFailed(Key),
    PlacementDuringMeasurement,
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

trait ErasedLayout: Any {
    fn render_context(&self, children: &Subviews<'_, '_>) -> retinue_render::OptimizationContext;
    fn value(&self) -> &dyn Any;
    fn same(&self, other: &dyn Any) -> bool;
    fn name(&self) -> &'static str;
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<Option<f32>, LayoutError>;
    fn make_cache(&self) -> Box<dyn Any>;
    fn update_cache(
        &self,
        cache: &mut dyn Any,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError>;
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<LayoutSize, LayoutError>;
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<(), LayoutError>;
}

impl<L: Layout> ErasedLayout for L {
    fn render_context(&self, children: &Subviews<'_, '_>) -> retinue_render::OptimizationContext {
        Layout::render_context(self, children)
    }
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<Option<f32>, LayoutError> {
        Layout::explicit_alignment(
            self,
            guide,
            bounds,
            children,
            cache.downcast_mut::<L::Cache>().unwrap(),
        )
    }
    fn value(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>() == Some(self)
    }
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn make_cache(&self) -> Box<dyn Any> {
        Box::new(L::Cache::default())
    }
    fn update_cache(
        &self,
        cache: &mut dyn Any,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        Layout::update_cache(self, cache.downcast_mut::<L::Cache>().unwrap(), children)
    }
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<LayoutSize, LayoutError> {
        Layout::measure(
            self,
            proposal,
            children,
            cache.downcast_mut::<L::Cache>().unwrap(),
        )
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<(), LayoutError> {
        Layout::place(
            self,
            bounds,
            children,
            cache.downcast_mut::<L::Cache>().unwrap(),
        )
    }
}

#[derive(Clone)]
pub struct LayoutAlgorithm(Rc<dyn ErasedLayout>);

impl PartialEq for LayoutAlgorithm {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.same(other.0.value())
    }
}

impl fmt::Debug for LayoutAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub(super) struct LayoutCache {
    kind: TypeId,
    value: Box<dyn Any>,
    pub dirty: bool,
    pub rendering: retinue_render::OptimizationContext,
}

impl LayoutAlgorithm {
    pub(super) fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        context: &mut LayoutContext<'_>,
        cache: &mut LayoutCache,
    ) -> Result<Option<f32>, LayoutError> {
        self.0.explicit_alignment(
            guide,
            bounds,
            &mut Subviews { context },
            cache.value.as_mut(),
        )
    }
    pub fn new<L: Layout>(layout: L) -> Self {
        Self(Rc::new(layout))
    }
    pub fn name(&self) -> &'static str {
        self.0.name()
    }

    pub fn downcast_ref<L: Layout>(&self) -> Option<&L> {
        self.0.value().downcast_ref()
    }

    pub(super) fn cache(&self, previous: Option<LayoutCache>) -> LayoutCache {
        let kind = self.0.value().type_id();
        previous
            .filter(|cache| cache.kind == kind)
            .unwrap_or_else(|| LayoutCache {
                kind,
                value: self.0.make_cache(),
                dirty: true,
                rendering: Default::default(),
            })
    }

    pub(super) fn refresh(
        &self,
        cache: &mut LayoutCache,
        context: &mut LayoutContext<'_>,
    ) -> Result<(), LayoutError> {
        if cache.dirty {
            self.0
                .update_cache(cache.value.as_mut(), &mut Subviews { context })?;
            let rendering = self.0.render_context(&Subviews { context });
            if cache.rendering != rendering {
                context.invalidate_projection();
                cache.rendering = rendering;
            }
            cache.dirty = false;
        }
        Ok(())
    }

    pub(super) fn measure(
        &self,
        proposal: Proposal,
        context: &mut LayoutContext<'_>,
        cache: &mut LayoutCache,
    ) -> Result<LayoutSize, LayoutError> {
        self.0
            .measure(proposal, &mut Subviews { context }, cache.value.as_mut())
    }

    pub(super) fn place(
        &self,
        bounds: Bounds,
        context: &mut LayoutContext<'_>,
        cache: &mut LayoutCache,
    ) -> Result<(), LayoutError> {
        self.0
            .place(bounds, &mut Subviews { context }, cache.value.as_mut())
    }
}
