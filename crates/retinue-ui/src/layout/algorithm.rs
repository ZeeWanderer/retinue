use super::alignment::AlignmentGuide;
use super::{Layout, LayoutContext, LayoutError, LayoutSize, Proposal, Subviews};
use crate::Bounds;
use retinue_render::OptimizationContext;
use std::{
    any::{Any, TypeId},
    fmt,
    rc::Rc,
    sync::Arc,
};

trait ErasedLayout: Any {
    fn render_context(&self, children: &Subviews<'_, '_>) -> OptimizationContext;
    fn value(&self) -> &dyn Any;
    fn same(&self, other: &dyn Any) -> bool;
    fn name(&self) -> &'static str;
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
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut dyn Any,
    ) -> Result<Option<f32>, LayoutError>;
}

impl<L: Layout> ErasedLayout for L {
    fn render_context(&self, children: &Subviews<'_, '_>) -> OptimizationContext {
        Layout::render_context(self, children)
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
}

trait ErasedSharedLayout: ErasedLayout + Send + Sync {
    fn make_shared_cache(&self) -> Box<dyn Any + Send>;
}

impl<L: Layout + Send + Sync> ErasedSharedLayout for L
where
    L::Cache: Send,
{
    fn make_shared_cache(&self) -> Box<dyn Any + Send> {
        Box::new(L::Cache::default())
    }
}

#[derive(Clone)]
pub(crate) struct SharedLayout(Arc<dyn ErasedSharedLayout>);

impl PartialEq for SharedLayout {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.same(other.0.value())
    }
}

#[derive(Clone)]
enum Dispatch {
    Shared(SharedLayout),
    Owner(Rc<dyn ErasedLayout>),
}

#[derive(Clone)]
pub struct LayoutAlgorithm(Dispatch);

impl PartialEq for LayoutAlgorithm {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Dispatch::Shared(a), Dispatch::Shared(b)) => a == b,
            (Dispatch::Owner(a), Dispatch::Owner(b)) => Rc::ptr_eq(a, b) || a.same(b.value()),
            _ => false,
        }
    }
}

impl fmt::Debug for LayoutAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub(crate) struct LayoutCache<V: ?Sized = dyn Any> {
    kind: TypeId,
    value: Box<V>,
    pub dirty: bool,
    pub rendering: OptimizationContext,
}

/// Read-only output from this node's completed custom layout.
#[derive(Clone, Copy, Default)]
pub struct LayoutResult<'a> {
    cache: Option<(TypeId, &'a dyn Any)>,
}

impl<'a> LayoutResult<'a> {
    pub fn get<L: Layout>(self) -> Option<&'a L::Cache> {
        let (kind, value) = self.cache?;
        (kind == TypeId::of::<L>())
            .then(|| value.downcast_ref::<L::Cache>())
            .flatten()
    }
}

impl LayoutCache {
    pub(crate) fn result(&self) -> LayoutResult<'_> {
        LayoutResult {
            cache: (!self.dirty).then_some((self.kind, self.value.as_ref())),
        }
    }
}

impl LayoutCache<dyn Any + Send> {
    pub(crate) fn result(&self) -> LayoutResult<'_> {
        LayoutResult {
            cache: (!self.dirty).then_some((self.kind, self.value.as_ref())),
        }
    }
}

pub(crate) enum WorkingCache {
    Shared(LayoutCache<dyn Any + Send>),
    Owner(LayoutCache),
}

impl WorkingCache {
    fn parts(&mut self) -> (&mut bool, &mut OptimizationContext, &mut dyn Any) {
        match self {
            Self::Shared(cache) => (&mut cache.dirty, &mut cache.rendering, cache.value.as_mut()),
            Self::Owner(cache) => (&mut cache.dirty, &mut cache.rendering, cache.value.as_mut()),
        }
    }
}

impl From<SharedLayout> for LayoutAlgorithm {
    fn from(layout: SharedLayout) -> Self {
        Self(Dispatch::Shared(layout))
    }
}

impl LayoutAlgorithm {
    /// Share read-only parameters; each computation exclusively owns its cache.
    pub fn new<L: Layout + Send + Sync>(layout: L) -> Self
    where
        L::Cache: Send,
    {
        Self(Dispatch::Shared(SharedLayout(Arc::new(layout))))
    }

    /// For algorithms that access owner-thread state. Their subtrees cannot run detached.
    pub fn owner<L: Layout>(layout: L) -> Self {
        Self(Dispatch::Owner(Rc::new(layout)))
    }

    pub(crate) fn split(self) -> Result<SharedLayout, Self> {
        match self.0 {
            Dispatch::Shared(layout) => Ok(layout),
            Dispatch::Owner(layout) => Err(Self(Dispatch::Owner(layout))),
        }
    }

    pub(crate) fn shared(&self) -> Option<&SharedLayout> {
        match &self.0 {
            Dispatch::Shared(layout) => Some(layout),
            Dispatch::Owner(_) => None,
        }
    }

    fn erased(&self) -> &dyn ErasedLayout {
        match &self.0 {
            Dispatch::Shared(layout) => layout.0.as_ref(),
            Dispatch::Owner(layout) => layout.as_ref(),
        }
    }
    pub fn name(&self) -> &'static str {
        self.erased().name()
    }
    pub fn downcast_ref<L: Layout>(&self) -> Option<&L> {
        self.erased().value().downcast_ref()
    }

    pub(crate) fn cache(&self, previous: Option<WorkingCache>) -> WorkingCache {
        let kind = self.erased().value().type_id();
        let matches = match (&self.0, &previous) {
            (Dispatch::Shared(_), Some(WorkingCache::Shared(cache))) => cache.kind == kind,
            (Dispatch::Owner(_), Some(WorkingCache::Owner(cache))) => cache.kind == kind,
            _ => false,
        };
        if matches {
            return previous.unwrap();
        }
        match &self.0 {
            Dispatch::Shared(layout) => WorkingCache::Shared(LayoutCache {
                kind,
                value: layout.0.make_shared_cache(),
                dirty: true,
                rendering: Default::default(),
            }),
            Dispatch::Owner(layout) => WorkingCache::Owner(LayoutCache {
                kind,
                value: layout.make_cache(),
                dirty: true,
                rendering: Default::default(),
            }),
        }
    }

    pub(crate) fn refresh(
        &self,
        cache: &mut WorkingCache,
        context: &mut LayoutContext<'_>,
    ) -> Result<(), LayoutError> {
        let (dirty, rendering, value) = cache.parts();
        if *dirty {
            self.erased()
                .update_cache(value, &mut Subviews { context })?;
            let updated = self.erased().render_context(&Subviews { context });
            if *rendering != updated {
                context.invalidate_projection();
                *rendering = updated;
            }
            *dirty = false;
        }
        Ok(())
    }

    pub(crate) fn measure(
        &self,
        proposal: Proposal,
        context: &mut LayoutContext<'_>,
        cache: &mut WorkingCache,
    ) -> Result<LayoutSize, LayoutError> {
        self.erased()
            .measure(proposal, &mut Subviews { context }, cache.parts().2)
    }
    pub(crate) fn place(
        &self,
        bounds: Bounds,
        context: &mut LayoutContext<'_>,
        cache: &mut WorkingCache,
    ) -> Result<(), LayoutError> {
        self.erased()
            .place(bounds, &mut Subviews { context }, cache.parts().2)
    }
    pub(crate) fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        bounds: Bounds,
        context: &mut LayoutContext<'_>,
        cache: &mut WorkingCache,
    ) -> Result<Option<f32>, LayoutError> {
        self.erased()
            .explicit_alignment(guide, bounds, &mut Subviews { context }, cache.parts().2)
    }
}
