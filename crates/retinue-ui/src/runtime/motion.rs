use super::{ViewId, ViewTree};
use crate::state::Mutation;
use crate::transition::Interpolate;
use crate::{
    motion::{
        ReduceMotion,
        presentation::{Presentation, Visual},
    },
    state::untracked,
    transition::Tween,
};

impl ViewTree {
    pub fn set_motion_enabled(&mut self, enabled: bool) {
        if self.motion_enabled == enabled {
            return;
        }
        self.motion_enabled = enabled;
        for (&id, node) in &self.nodes {
            let mut node = node.borrow_mut();
            if node
                .body
                .as_ref()
                .is_some_and(|body| body.deadlines.next.is_some())
            {
                node.timed_update = true;
                self.queue.insert(node.depth, id);
            }
        }
        if !enabled && !self.presentations.is_empty() {
            self.changed.extend(self.presentations.keys().copied());
            self.queue.wake();
        }
    }

    pub fn animating(&self, id: ViewId) -> bool {
        self.presentations.contains_key(&id)
            || self.nodes.get(&id).is_some_and(|node| {
                node.borrow()
                    .body
                    .as_ref()
                    .is_some_and(|body| body.deadlines.motion)
            })
    }

    pub(super) fn retarget_presentation(
        &mut self,
        id: ViewId,
        before: Option<Visual>,
        after: Option<Visual>,
        cause: Option<Mutation>,
    ) {
        let Some(target) = after else {
            return;
        };
        let disabled = !self.motion_enabled
            || cause.is_some_and(|cause| cause.transaction.disables_animations)
            || untracked(|| self.nodes[&id].borrow().environment.get::<ReduceMotion>());
        if before.as_ref() == Some(&target) && !disabled {
            return;
        }
        let animation = cause
            .and_then(|cause| cause.transaction.animation)
            .filter(|_| !disabled);
        if let (Some(before), Some(animation)) = (before, animation) {
            let from = self
                .presentations
                .get(&id)
                .map_or(before, |track| track.tween.value(self.time.instant));
            let from = Visual::interpolate(&from, &target, 0.0);
            if from == target || animation.duration.is_zero() {
                self.presentations.remove(&id);
                self.changed.insert(id);
            } else {
                let mut tween = Tween::new(from.clone());
                tween.set(target, self.time.instant, animation);
                self.presentations
                    .insert(id, Presentation { tween, value: from });
            }
        } else if self.presentations.remove(&id).is_some() {
            self.changed.insert(id);
            self.nodes[&id].borrow_mut().revision += 1;
        }
    }

    pub(super) fn sample_presentations(&mut self) {
        let mut finished = Vec::new();
        for (&id, track) in &mut self.presentations {
            let mut node = self.nodes[&id].borrow_mut();
            if !self.motion_enabled || untracked(|| node.environment.get::<ReduceMotion>()) {
                track.tween.snap(track.tween.target().clone());
            }
            let value = track.tween.value(self.time.instant);
            if value != track.value {
                if value.rectangle != track.value.rectangle {
                    node.revision += 1;
                }
                track.value = value;
                self.changed.insert(id);
            }
            track.tween.advance(self.time.instant);
            self.stats.motion_samples += 1;
            if !track.tween.moving() {
                finished.push(id);
            }
        }
        for id in finished {
            self.presentations.remove(&id);
        }
    }

    pub(super) fn scoped_mutation(&self, id: ViewId, cause: Option<Mutation>) -> Option<Mutation> {
        let mut parent = self.nodes[&id].borrow().parent;
        while let Some(id) = parent {
            let node = self.nodes[&id].borrow();
            if let Some((epoch, transaction)) = node.animation_scope {
                if epoch == self.epoch {
                    let mut cause = cause.unwrap_or_else(Mutation::capture);
                    let disabled = cause.transaction.disables_animations;
                    cause.transaction = transaction;
                    cause.transaction.disables_animations |= disabled;
                    return Some(cause);
                }
            }
            parent = node.parent;
        }
        cause
    }
}
