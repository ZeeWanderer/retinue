use super::*;
use crate::scene::layout::invalidation::Alignment;

impl Computation<'_> {
    pub(super) fn compute_alignment(
        &mut self,
        slot: usize,
        guide: AlignmentGuide,
        proposal: Proposal,
        input: LayoutInput,
    ) -> Result<f32, LayoutError> {
        let key = self.inputs[slot].as_ref().unwrap().key;
        let result = self.alignment_value(slot, guide, proposal, input);
        match result {
            Ok(value) if value.is_finite() => {
                let geometry = self.geometry_mut(slot);
                if !geometry.validating {
                    geometry.dependencies.observe_alignment(Alignment {
                        guide,
                        proposal,
                        input,
                        value,
                    });
                }
                Ok(value)
            }
            result => {
                self.state.failure.get_or_insert(LayoutFailure {
                    container: key,
                    error: result.err().unwrap_or(LayoutError::InvalidGeometry),
                });
                Err(LayoutError::ChildFailed(key))
            }
        }
    }

    fn alignment_value(
        &mut self,
        slot: usize,
        guide: AlignmentGuide,
        proposal: Proposal,
        parent_input: LayoutInput,
    ) -> Result<f32, LayoutError> {
        let available_space = Size {
            width: space(proposal.width)?,
            height: space(proposal.height)?,
        };
        let measured = self
            .compute_child_layout(
                slot.into(),
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: taffy::SizingMode::InherentSize,
                    axis: taffy::RequestedAxis::Both,
                    known_dimensions: Size::NONE,
                    parent_size: available_space.map(|space| match space {
                        AvailableSpace::Definite(value) => Some(value),
                        _ => None,
                    }),
                    available_space,
                    ..parent_input
                },
            )
            .size;
        let size = LayoutSize::new(measured.width, measured.height);
        if let Some(failure) = &self.state.failure {
            return Err(LayoutError::ChildFailed(failure.container));
        }
        let input = self.inputs[slot].as_ref().unwrap();
        if let Some(value) = guide.explicit(&input.layout_values, size) {
            return Ok(value);
        }
        let Some(algorithm) = self.algorithm(slot) else {
            return Ok(guide.fallback(size));
        };
        let parent_width = match proposal.width {
            Available::Limit(width) => Some(width),
            _ => None,
        };
        let insets = input
            .style
            .padding
            .resolve_or_zero(parent_width, &|_, _| 0.0)
            + input
                .style
                .border
                .resolve_or_zero(parent_width, &|_, _| 0.0);
        let bounds = Bounds::new(
            insets.left,
            insets.top,
            (size.width - insets.left - insets.right).max(0.0),
            (size.height - insets.top - insets.bottom).max(0.0),
        );
        let mut cache = algorithm.cache(self.take_cache(slot));
        let mut context = LayoutContext {
            tree: self.reborrow(),
            node: slot,
            input: LayoutInput {
                run_mode: RunMode::ComputeSize,
                ..parent_input
            },
            parent_size: Size {
                width: Some(bounds.width),
                height: Some(bounds.height),
            },
            placed: None,
            #[cfg(feature = "geometry-census")]
            pairs: None,
        };
        let result = algorithm
            .refresh(&mut cache, &mut context)
            .and_then(|()| algorithm.explicit_alignment(guide, bounds, &mut context, &mut cache));
        context.tree.put_cache(slot, cache);
        Ok(result?.unwrap_or_else(|| guide.fallback(size)))
    }
}
