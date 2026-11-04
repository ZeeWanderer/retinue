use super::{Renderer, coverage::SpatialIndex};
use crate::{CachePolicy, PaintFrame, PaintInstance, PaintProgram, RasterMode, Rect, fragments};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(crate) struct Work {
    pub bounds: Rect,
    pub density: crate::cost::Density,
}

/// Read-only projection of materializations already available for this frame.
pub(crate) struct Estimate<'a> {
    renderer: Option<&'a Renderer>,
    mode: RasterMode,
    coverage: Option<SpatialIndex>,
    pub layers: Vec<Rect>,
}

impl<'a> Estimate<'a> {
    pub fn new(renderer: &'a Renderer, frame: &PaintFrame, mode: RasterMode) -> Self {
        let renderer = (renderer.optimizations
            && renderer.materializations
            && renderer.mode == mode
            && renderer
                .prepared_for
                .as_ref()
                .is_some_and(|(id, _)| Rc::ptr_eq(id, &frame.identity)))
        .then_some(renderer);
        let regions = renderer.map_or_else(Vec::new, |r| r.layers.resident_coverage(frame));
        let layers = regions
            .iter()
            .flat_map(|(_, rects)| rects.iter().copied())
            .collect();
        let coverage = (!regions.is_empty()).then(|| {
            let mut index = SpatialIndex::default();
            index.update(regions);
            index
        });
        Self {
            renderer,
            mode,
            coverage,
            layers,
        }
    }

    pub fn instance(
        &self,
        index: usize,
        instance: &PaintInstance,
        emit: &mut impl FnMut(Work),
    ) -> crate::cost::Density {
        let cached = self.mode == RasterMode::Explicit
            && instance.cache == CachePolicy::Prefer
            && self.renderer.is_some_and(|r| {
                (r.prepared.get(index).is_some_and(Option::is_some)
                    && r.dependencies.has_program(index, instance.program.id()))
                    || r.residency.has_item(instance.program.id())
            });
        if let Some(coverage) = &self.coverage {
            for clip in coverage.remaining(index, instance.visible_bounds()) {
                self.program(&instance.program, instance.origin(), clip, cached, emit);
            }
            return Default::default();
        }
        if cached {
            return crate::cost::Density::image(crate::cost::area(instance.program.bounds()));
        }
        self.calls(&instance.program, instance.origin(), instance.clip(), emit);
        instance.program.cost(true)
    }

    fn program(
        &self,
        program: &PaintProgram,
        origin: (i32, i32),
        clip: Rect,
        cached: bool,
        emit: &mut impl FnMut(Work),
    ) {
        let bounds = program.bounds().visible(origin, clip);
        if bounds.is_empty() {
            return;
        }
        if cached {
            emit(Work {
                bounds,
                density: crate::cost::Density::image(crate::cost::area(program.bounds())),
            });
            return;
        }
        // Calls have their own bounds; do not charge a small cached fragment
        // as though its original commands cover the entire parent program.
        let own_work = program.summary().direct_work;
        if own_work != 0 {
            emit(Work {
                bounds,
                density: program.cost(true),
            });
        }
        self.calls(program, origin, clip, emit);
    }

    fn calls(
        &self,
        program: &PaintProgram,
        origin: (i32, i32),
        clip: Rect,
        emit: &mut impl FnMut(Work),
    ) {
        for call in program.calls() {
            let Some(x) = origin.0.checked_add(call.origin.0) else {
                continue;
            };
            let Some(y) = origin.1.checked_add(call.origin.1) else {
                continue;
            };
            let clip = call.clip.map_or(clip, |c| c.visible(origin, clip));
            let cached = fragments::allowed(&call.context)
                && self
                    .renderer
                    .is_some_and(|r| r.fragments.selected.contains_key(&call.program.id()));
            self.program(&call.program, (x, y), clip, cached, emit);
        }
    }
}

#[cfg(test)]
mod tests;
