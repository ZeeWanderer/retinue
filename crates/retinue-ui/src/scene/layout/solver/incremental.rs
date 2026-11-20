use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

impl Computation<'_> {
    fn invalidate_node(&mut self, slot: usize) {
        self.geometry_mut(slot).invalidate();
        if let Some(owners) = &mut self.owners
            && let Some(cache) = &mut owners[slot].as_mut().unwrap().custom_cache
        {
            cache.dirty = true;
        }
    }

    fn invalidate_paths(&mut self, starts: &[usize], mut first: impl FnMut(usize) -> bool) {
        for &start in starts {
            let mut next = Some(start);
            while let Some(slot) = next {
                let Some(input) = self.inputs.get(slot).and_then(Option::as_ref) else {
                    break;
                };
                if !first(slot) {
                    break;
                }
                next = input.parent;
                self.invalidate_node(slot);
            }
        }
    }

    fn invalidate_full_layout(&mut self, starts: &[usize]) {
        if starts.len() < 2 {
            self.invalidate_paths(starts, |_| true);
            return;
        }
        let mut marks = std::mem::take(&mut self.state.ancestor_marks);
        // Marks belong to this walk only: the next repair can reuse or reparent slots.
        marks.clear();
        marks.resize(self.inputs.len().div_ceil(64), 0);
        self.invalidate_paths(starts, |slot| {
            let word = &mut marks[slot / 64];
            let bit = 1 << (slot % 64);
            let first = *word & bit == 0;
            *word |= bit;
            first
        });
        self.state.ancestor_marks = marks;
    }

    fn depth(&self, mut slot: usize) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.inputs[slot].as_ref().unwrap().parent {
            depth += 1;
            slot = parent;
        }
        depth
    }

    pub(super) fn repair_dependencies(&mut self, root: usize, size: Size<f32>) {
        let mut invalidated = std::mem::take(&mut self.state.invalidated);
        let same_viewport = self
            .geometry(root)
            .dependencies
            .placement
            .is_some_and(|(input, _)| input.parent_size == size.map(Some));
        if std::mem::take(&mut self.state.full_layout) || !same_viewport {
            self.invalidate_full_layout(&invalidated);
        } else {
            let mut pending = std::mem::take(&mut self.state.pending);
            for &slot in &invalidated {
                if self.inputs.get(slot).is_some_and(Option::is_some) {
                    pending.push((self.depth(slot), slot));
                }
            }
            while let Some((depth, slot)) = pending.pop() {
                while pending.peek() == Some(&(depth, slot)) {
                    pending.pop();
                }
                let changed = self.repair_node(slot);
                if self.state.failure.is_some() {
                    break;
                }
                if changed && let Some(parent) = self.inputs[slot].as_ref().unwrap().parent {
                    self.invalidate_node(parent);
                    pending.push((depth - 1, parent));
                }
            }
            pending.clear();
            self.state.pending = pending;
        }
        invalidated.clear();
        self.state.invalidated = invalidated;
    }

    fn repair_node(&mut self, slot: usize) -> bool {
        let geometry = self.geometry_mut(slot);
        let mut dependencies = std::mem::take(&mut geometry.dependencies);
        geometry.validating = true;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut changed = dependencies.overflowed || dependencies.placement.is_none();
            for query in &mut dependencies.measurements {
                let output = self.compute_child_layout(slot.into(), query.input);
                changed |= query.output != output;
                query.output = output;
            }
            for query in &mut dependencies.alignments {
                match self.compute_alignment(slot, query.guide, query.proposal, query.input) {
                    Ok(value) => {
                        changed |= query.value != value;
                        query.value = value;
                    }
                    Err(_) => return true,
                }
            }
            if let Some((input, previous)) = &mut dependencies.placement {
                let output = self.compute_child_layout(slot.into(), *input);
                changed |= *previous != output;
                *previous = output;
                let mut layout = self.geometry(slot).layout;
                layout.size = output.size;
                self.set_unrounded_layout(slot.into(), &layout);
            }
            changed
        }));
        let geometry = self.geometry_mut(slot);
        geometry.dependencies = dependencies;
        geometry.validating = false;
        match result {
            Ok(changed) => changed,
            Err(panic) => resume_unwind(panic),
        }
    }
}
