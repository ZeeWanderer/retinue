use super::{LayoutExecution, ViewTree, profile::timed};
use crate::timeline::FrameTime;
use retinue_render::damage::Damage;
use std::task::Poll;

#[cfg(test)]
mod tests;

impl ViewTree {
    pub fn is_layout_pending(&self) -> bool {
        self.scene.pending_layout().is_some()
    }

    /// Advances one update without waiting for layout workers. Until Ready, input
    /// dispatch and painting use the last committed geometry and commands.
    pub fn advance_at(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: FrameTime,
        execution: &LayoutExecution,
    ) -> Result<Poll<Damage>, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("view scale must be positive and finite");
        }
        if let Some(viewport) = self.scene.pending_layout() {
            let current = viewport == (size, scale) && !self.needs_evaluation();
            match self.scene.poll_layout(current) {
                Ok(Poll::Pending) => return Ok(Poll::Pending),
                Ok(Poll::Ready(Some(damage))) => {
                    timed!(self, input, self.sync_input());
                    return Ok(Poll::Ready(damage));
                }
                Ok(Poll::Ready(None)) => self.full_projection = true,
                Err(error) => {
                    self.full_projection = true;
                    return Err(error);
                }
            }
        }
        self.perform_update(size, scale, time, Some(execution))
    }
}
