use super::{LayoutExecution, RecordingMode, Scene, layout};
use retinue_render::damage::Damage;
use std::task::Poll;
use taffy::Size;

pub(crate) type Update = Result<Poll<Damage>, &'static str>;

pub(super) struct PendingLayout {
    work: layout::Pending,
    size: (u32, u32),
    scale: f32,
    mode: RecordingMode,
}

pub(super) fn inline(result: Update) -> Result<Damage, &'static str> {
    match result? {
        Poll::Ready(damage) => Ok(damage),
        Poll::Pending => Err("layout update pending"),
    }
}

impl Scene {
    pub(crate) fn pending_layout(&self) -> Option<((u32, u32), f32)> {
        self.pending_layout
            .as_ref()
            .map(|pending| (pending.size, pending.scale))
    }

    pub(super) fn ensure_idle(&self) -> Result<(), &'static str> {
        if self.pending_layout.is_some() {
            Err("layout update pending")
        } else {
            Ok(())
        }
    }

    pub(crate) fn poll_layout(
        &mut self,
        publish: bool,
    ) -> Result<Poll<Option<Damage>>, &'static str> {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Layout);
        let pending = self.pending_layout.as_mut().expect("pending layout");
        let Some(result) = pending.work.poll(&mut self.tree) else {
            return Ok(Poll::Pending);
        };
        #[cfg(feature = "profiling")]
        {
            self.profile.layout += pending.work.elapsed;
        }
        let pending = self.pending_layout.take().unwrap();
        if let Err(error) = result {
            self.layout_unpublished = true;
            return Err(error);
        }
        if publish {
            Ok(Poll::Ready(Some(self.publish_layout(
                pending.size,
                pending.scale,
                pending.mode,
            ))))
        } else {
            self.layout_unpublished = true;
            Ok(Poll::Ready(None))
        }
    }

    pub(super) fn resolve_with(
        &mut self,
        root: usize,
        size: (u32, u32),
        scale: f32,
        mode: RecordingMode,
        execution: Option<&LayoutExecution>,
    ) -> Update {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Layout);
        let logical = Size {
            width: size.0 as f32 / scale,
            height: size.1 as f32 / scale,
        };
        if let Some(execution) = execution
            && let Some(work) = self.tree.begin_layout(root, logical, execution)
        {
            self.pending_layout = Some(PendingLayout {
                work,
                size,
                scale,
                mode,
            });
            return Ok(Poll::Pending);
        }
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        if let Err(error) = self.tree.compute(root, logical) {
            self.layout_unpublished = true;
            return Err(error);
        }
        #[cfg(feature = "profiling")]
        {
            self.profile.layout += started.elapsed();
        }
        Ok(Poll::Ready(self.publish_layout(size, scale, mode)))
    }

    fn publish_layout(&mut self, size: (u32, u32), scale: f32, mode: RecordingMode) -> Damage {
        self.tree.topology_changed |= self.layout_unpublished;
        let damage = self.project(mode, size, scale);
        self.layout_unpublished = false;
        damage
    }
}
