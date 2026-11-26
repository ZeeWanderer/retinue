use super::{Input, Kind, Owner, State, Work};
mod alignment;
mod batch;
mod incremental;
use crate::{
    Bounds, Key,
    layout::{
        Available, LayoutAlgorithm, LayoutError, LayoutFailure, LayoutSize, Proposal, WorkingCache,
        alignment::AlignmentGuide, values::LayoutValues,
    },
};
use taffy::{
    AvailableSpace, CacheTree, Layout, LayoutFlexboxContainer, LayoutGridContainer, LayoutInput,
    LayoutOutput, LayoutPartialTree, NodeId, Point, RunMode, Size, Style, TraversePartialTree,
    TraverseTree, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
    compute_hidden_layout, compute_leaf_layout, compute_root_layout, util::ResolveOrZero,
};

pub(super) struct Computation<'a> {
    inputs: &'a [Option<Input>],
    state: &'a mut State,
    owners: Option<&'a mut [Option<Owner>]>,
    parallel: Option<&'a mut retinue_render::work::Parallelism>,
    minimum_batch_nodes: usize,
    partition: Option<&'a mut dyn batch::GroupGeometry>,
}

impl<'a> Computation<'a> {
    pub(super) fn new(
        inputs: &'a [Option<Input>],
        state: &'a mut State,
        owners: Option<&'a mut [Option<Owner>]>,
    ) -> Self {
        Self {
            inputs,
            state,
            owners,
            parallel: None,
            minimum_batch_nodes: usize::MAX,
            partition: None,
        }
    }

    pub(super) fn batched(
        mut self,
        parallel: &'a mut retinue_render::work::Parallelism,
        minimum_nodes: usize,
    ) -> Self {
        self.parallel = Some(parallel);
        self.minimum_batch_nodes = minimum_nodes;
        self
    }

    fn reborrow(&mut self) -> Computation<'_> {
        Computation {
            inputs: self.inputs,
            state: self.state,
            owners: self.owners.as_deref_mut(),
            parallel: self.parallel.as_deref_mut(),
            minimum_batch_nodes: self.minimum_batch_nodes,
            partition: self
                .partition
                .as_mut()
                .map(|p| &mut **p as &mut dyn batch::GroupGeometry),
        }
    }

    fn geometry(&self, slot: usize) -> &super::Geometry {
        match &self.partition {
            Some(group) => group.at(slot),
            None => self.state.nodes[slot].as_ref().unwrap(),
        }
    }

    fn geometry_mut(&mut self, slot: usize) -> &mut super::Geometry {
        match &mut self.partition {
            Some(group) => group.at_mut(slot),
            None => self.state.nodes[slot].as_mut().unwrap(),
        }
    }

    fn owner(&self, slot: usize) -> &Owner {
        self.owners.as_ref().expect("owner layout callbacks")[slot]
            .as_ref()
            .unwrap()
    }

    fn owner_mut(&mut self, slot: usize) -> &mut Owner {
        self.owners.as_mut().expect("owner layout callbacks")[slot]
            .as_mut()
            .unwrap()
    }

    fn algorithm(&self, slot: usize) -> Option<LayoutAlgorithm> {
        let input = self.inputs[slot].as_ref().unwrap();
        match input.kind {
            Kind::Shared => input.custom.clone().map(LayoutAlgorithm::from),
            Kind::Owner => self.owner(slot).custom.clone(),
            Kind::Flex | Kind::Grid => None,
        }
    }

    fn take_cache(&mut self, slot: usize) -> Option<WorkingCache> {
        match self.inputs[slot].as_ref().unwrap().kind {
            Kind::Shared => self
                .geometry_mut(slot)
                .shared_cache
                .take()
                .map(WorkingCache::Shared),
            Kind::Owner => self
                .owner_mut(slot)
                .custom_cache
                .take()
                .map(WorkingCache::Owner),
            Kind::Flex | Kind::Grid => None,
        }
    }

    fn put_cache(&mut self, slot: usize, cache: WorkingCache) {
        match cache {
            WorkingCache::Shared(cache) => self.geometry_mut(slot).shared_cache = Some(cache),
            WorkingCache::Owner(cache) => self.owner_mut(slot).custom_cache = Some(cache),
        }
    }

    pub(super) fn compute(&mut self, root: usize, size: Size<f32>) -> Result<(), &'static str> {
        if self.owners.is_none()
            && self
                .inputs
                .iter()
                .flatten()
                .any(|input| input.kind == Kind::Owner)
        {
            return Err("custom layout requires owner callbacks");
        }
        self.state.computed = 0;
        self.state.failure = None;
        self.repair_dependencies(root, size);
        compute_root_layout(self, root.into(), size.map(AvailableSpace::Definite));
        if self.state.failure.is_some() {
            // Failed measurements must not become valid cached layout on retry.
            self.state.invalidate_caches();
            if let Some(owners) = &mut self.owners {
                for node in owners.iter_mut().flatten() {
                    if let Some(cache) = &mut node.custom_cache {
                        cache.dirty = true;
                    }
                }
            }
            Err("custom layout failed")
        } else {
            Ok(())
        }
    }

    fn custom(
        &mut self,
        node: usize,
        input: LayoutInput,
        layout: LayoutAlgorithm,
        #[cfg(feature = "geometry-census")] mut visit: Option<
            &mut crate::scene::census::solver::Visit,
        >,
    ) -> LayoutOutput {
        let inputs = self.inputs;
        let entry = inputs[node].as_ref().unwrap();
        let key = entry.key;
        #[cfg(feature = "geometry-census")]
        if let Some(v) = &mut visit {
            v.algorithm(layout.name());
        }
        let style: &Style = &entry.style;
        let mut cache = layout.cache(self.take_cache(node));
        let mut context = LayoutContext {
            tree: self.reborrow(),
            node,
            input,
            parent_size: input.parent_size,
            placed: None,
            #[cfg(feature = "geometry-census")]
            pairs: None,
        };
        if let Err(error) = layout.refresh(&mut cache, &mut context) {
            context.tree.state.failure.get_or_insert(LayoutFailure {
                container: key,
                error,
            });
            context.tree.put_cache(node, cache);
            return LayoutOutput::DEFAULT;
        }
        #[cfg(feature = "geometry-census")]
        let (adapter, mut callback_ns) = (visit.as_deref().and_then(|v| v.clock()), 0);
        #[cfg(feature = "geometry-census")]
        if visit.is_some() && input.run_mode == RunMode::PerformLayout {
            context.pairs = Some(crate::scene::census::solver::MeasurePairs::new(
                adapter.is_some(),
            ));
        }
        let output = compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |_, space| {
                #[cfg(feature = "geometry-census")]
                let begin = adapter.map(|_| {
                    (
                        std::time::Instant::now(),
                        crate::scene::census::solver::Visit::child_ns(),
                    )
                });
                let result = match layout.measure(
                    Proposal {
                        width: available(space.width),
                        height: available(space.height),
                    },
                    &mut context,
                    &mut cache,
                ) {
                    Ok(size) if size.valid() => Size {
                        width: size.width,
                        height: size.height,
                    },
                    result => {
                        context.tree.state.failure.get_or_insert(LayoutFailure {
                            container: key,
                            error: result.err().unwrap_or(LayoutError::InvalidGeometry),
                        });
                        Size::ZERO
                    }
                };
                #[cfg(feature = "geometry-census")]
                if let Some(v) = &mut visit {
                    callback_ns = v.measured(begin);
                }
                result
            },
        );
        #[cfg(feature = "geometry-census")]
        if let (Some(t), Some(v)) = (adapter, visit.as_deref_mut()) {
            v.adapter_ns = (t.elapsed().as_nanos() as u64).saturating_sub(callback_ns);
        }
        if input.run_mode == RunMode::PerformLayout && context.tree.state.failure.is_none() {
            let insets = style
                .padding
                .resolve_or_zero(input.parent_size.width, &|_, _| 0.0)
                + style
                    .border
                    .resolve_or_zero(input.parent_size.width, &|_, _| 0.0);
            let bounds = Bounds::new(
                insets.left,
                insets.top,
                (output.size.width - insets.left - insets.right).max(0.0),
                (output.size.height - insets.top - insets.bottom).max(0.0),
            );
            context.parent_size = Size {
                width: Some(bounds.width),
                height: Some(bounds.height),
            };
            context.placed = Some(vec![false; context.child_count()]);
            if let Err(error) = layout.place(bounds, &mut context, &mut cache) {
                context.tree.state.failure.get_or_insert(LayoutFailure {
                    container: key,
                    error,
                });
            } else if let Some(index) = context
                .placed
                .as_ref()
                .unwrap()
                .iter()
                .position(|placed| !placed)
            {
                let child = context.child_key(index).unwrap();
                context.tree.state.failure.get_or_insert(LayoutFailure {
                    container: key,
                    error: LayoutError::UnplacedChild(child),
                });
            }
            if context.tree.state.failure.is_none() {
                let geometry = context.tree.geometry_mut(node);
                geometry.revision = geometry.revision.wrapping_add(1);
                if entry.records_layout {
                    context.tree.state.changed.mark(node, Work::PAINT);
                }
            }
        }
        #[cfg(feature = "geometry-census")]
        if let (Some(v), Some(pairs)) = (visit, context.pairs.take()) {
            v.placement = pairs.totals;
        }
        context.tree.put_cache(node, cache);
        output
    }
}

pub(crate) struct LayoutContext<'a> {
    tree: Computation<'a>,
    node: usize,
    input: LayoutInput,
    parent_size: Size<Option<f32>>,
    placed: Option<Vec<bool>>,
    #[cfg(feature = "geometry-census")]
    pairs: Option<crate::scene::census::solver::MeasurePairs>,
}

impl LayoutContext<'_> {
    pub(crate) fn invalidate_projection(&mut self) {
        self.tree
            .state
            .changed
            .mark(self.node, Work::RENDER_CONTEXT);
    }

    pub fn child_count(&self) -> usize {
        self.tree.inputs[self.node].as_ref().unwrap().children.len()
    }

    pub fn child_key(&self, child: usize) -> Option<Key> {
        let slot = *self.tree.inputs[self.node].as_ref()?.children.get(child)?;
        let input = self.tree.inputs[slot].as_ref()?;
        Some(input.layout_key.unwrap_or(input.key))
    }

    pub fn child_values(&self, child: usize) -> Result<&LayoutValues, LayoutError> {
        let slot = *self.tree.inputs[self.node]
            .as_ref()
            .unwrap()
            .children
            .get(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        Ok(&self.tree.inputs[slot].as_ref().unwrap().layout_values)
    }

    pub fn child_alignment(
        &mut self,
        child: usize,
        guide: AlignmentGuide,
        proposal: Proposal,
    ) -> Result<f32, LayoutError> {
        let slot = *self.tree.inputs[self.node]
            .as_ref()
            .unwrap()
            .children
            .get(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        self.tree
            .compute_alignment(slot, guide, proposal, self.input)
    }

    pub fn measure_child(
        &mut self,
        child: usize,
        proposal: Proposal,
    ) -> Result<LayoutSize, LayoutError> {
        let key = self
            .child_key(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        let available = Size {
            width: space(proposal.width)?,
            height: space(proposal.height)?,
        };
        let id = self.tree.get_child_id(self.node.into(), child);
        #[cfg(feature = "geometry-census")]
        let matched = self.pairs.as_mut().and_then(|p| {
            (self.placed.is_some() && p.matches(child, proposal)).then(|| {
                (
                    self.tree.state.computed,
                    p.timed.then(std::time::Instant::now),
                )
            })
        });
        let size = self
            .tree
            .compute_child_layout(
                id,
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: taffy::SizingMode::InherentSize,
                    axis: taffy::RequestedAxis::Both,
                    known_dimensions: Size::NONE,
                    parent_size: available.map(|space| match space {
                        AvailableSpace::Definite(value) => Some(value),
                        _ => None,
                    }),
                    available_space: available,
                    ..self.input
                },
            )
            .size;
        if self.tree.state.failure.is_some() {
            return Err(LayoutError::ChildFailed(key));
        }
        #[cfg(feature = "geometry-census")]
        if let Some(p) = &mut self.pairs {
            if let Some((before, start)) = matched {
                p.totals[2] += u64::from(self.tree.state.computed != before);
                p.totals[3] += start.map_or(0, |t| t.elapsed().as_nanos() as u64);
            } else if self.placed.is_none() {
                p.measured(child, proposal);
            }
        }
        Ok(LayoutSize::new(size.width, size.height))
    }

    pub fn place_child(&mut self, child: usize, bounds: Bounds) -> Result<(), LayoutError> {
        let request = batch::Placement::new(self, child, bounds)?;
        request.run(&mut self.tree)?;
        self.placed.as_mut().unwrap()[child] = true;
        Ok(())
    }
}

fn space(value: Available) -> Result<AvailableSpace, LayoutError> {
    match value {
        Available::MinContent => Ok(AvailableSpace::MinContent),
        Available::Ideal => Ok(AvailableSpace::MaxContent),
        Available::Limit(value) if value.is_finite() && value >= 0.0 => {
            Ok(AvailableSpace::Definite(value))
        }
        _ => Err(LayoutError::InvalidGeometry),
    }
}

fn available(space: AvailableSpace) -> Available {
    match space {
        AvailableSpace::MinContent => Available::MinContent,
        AvailableSpace::MaxContent => Available::Ideal,
        AvailableSpace::Definite(value) => Available::Limit(value.max(0.0)),
    }
}

impl TraversePartialTree for Computation<'_> {
    type ChildIter<'a>
        = std::iter::Map<std::slice::Iter<'a, usize>, fn(&usize) -> NodeId>
    where
        Self: 'a;
    fn child_ids(&self, node: NodeId) -> Self::ChildIter<'_> {
        self.inputs[usize::from(node)]
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|id| (*id).into())
    }
    fn child_count(&self, node: NodeId) -> usize {
        self.inputs[usize::from(node)]
            .as_ref()
            .unwrap()
            .children
            .len()
    }
    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        self.inputs[usize::from(node)].as_ref().unwrap().children[index].into()
    }
}

impl TraverseTree for Computation<'_> {}

impl LayoutPartialTree for Computation<'_> {
    type CustomIdent = String;
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_core_container_style(&self, node: NodeId) -> &Style {
        &self.inputs[usize::from(node)].as_ref().unwrap().style
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        let slot = usize::from(node);
        let old = self.geometry_mut(slot);
        if old.layout.size != layout.size || old.layout.location != layout.location {
            self.state.changed.mark(slot, Work::GEOMETRY);
        }
        self.geometry_mut(slot).layout = *layout;
    }

    fn resolve_calc_value(&self, _: *const (), _: f32) -> f32 {
        0.0
    }

    fn compute_child_layout(&mut self, node: NodeId, input: LayoutInput) -> LayoutOutput {
        #[cfg(feature = "geometry-census")]
        let mut visit = crate::scene::census::solver::Visit::begin(input, || {
            let entry = self.inputs[usize::from(node)].as_ref().unwrap();
            match (entry.kind, entry.children.is_empty()) {
                (Kind::Flex | Kind::Grid, true) => "plain_leaf",
                (Kind::Flex, false) => "flex",
                (Kind::Grid, false) => "grid",
                (_, true) => "custom_leaf",
                (_, false) => "custom_container",
            }
        });
        let output = compute_cached_layout(self, node, input, |tree, node, input| {
            #[cfg(feature = "geometry-census")]
            if let Some(v) = &mut visit {
                v.miss = true;
            }
            tree.state.computed += 1;
            let slot = usize::from(node);
            if std::mem::take(&mut tree.geometry_mut(slot).reset_child_queries) {
                for &child in &tree.inputs[slot].as_ref().unwrap().children {
                    tree.geometry_mut(child).dependencies.reset_queries();
                }
            }
            if input.run_mode == RunMode::PerformHiddenLayout {
                return compute_hidden_layout(tree, node);
            }
            let entry = tree.inputs[slot].as_ref().unwrap();
            if entry.children.is_empty() && matches!(entry.kind, Kind::Flex | Kind::Grid) {
                return compute_leaf_layout(input, &*entry.style, |_, _| 0.0, |_, _| Size::ZERO);
            }
            match entry.kind {
                Kind::Flex => compute_flexbox_layout(tree, node, input),
                Kind::Grid => compute_grid_layout(tree, node, input),
                Kind::Shared | Kind::Owner => tree.custom(
                    slot,
                    input,
                    tree.algorithm(slot).unwrap(),
                    #[cfg(feature = "geometry-census")]
                    visit.as_mut(),
                ),
            }
        });
        let geometry = self.geometry_mut(usize::from(node));
        if !geometry.validating {
            geometry.dependencies.observe(input, output);
        }
        output
    }
}

impl CacheTree for Computation<'_> {
    fn cache_get(&self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.geometry(usize::from(node)).cache.get(input)
    }
    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.geometry_mut(usize::from(node))
            .cache
            .store(input, output);
    }
    fn cache_clear(&mut self, node: NodeId) {
        self.geometry_mut(usize::from(node)).cache.clear();
    }
}

impl LayoutFlexboxContainer for Computation<'_> {
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;
    fn get_flexbox_container_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
    fn get_flexbox_child_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
}

impl LayoutGridContainer for Computation<'_> {
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;
    fn get_grid_container_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
    fn get_grid_child_style(&self, node: NodeId) -> &Style {
        self.get_core_container_style(node)
    }
}
