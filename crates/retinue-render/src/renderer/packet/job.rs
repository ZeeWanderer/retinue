use super::*;
use crate::{execution::parallel::Running, renderer::preparation::Completed};
use std::{cell::RefCell, rc::Rc};

mod raster;
use raster::Raster;

pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug)]
pub enum PacketError {
    Invalidated,
    Failed(String),
}

enum Source {
    Ready(DrawPacket),
    Placed(Box<PacketJob>, (i32, i32)),
    Stack(Vec<PacketJob>),
    Raster(Box<Raster>),
    Draining(Box<PacketJob>, PacketError),
    Renderer {
        renderer: Rc<RefCell<Renderer>>,
        epoch: u64,
        frame: crate::frame::FrozenFrame,
        repair: Region,
        pending: Option<Pending>,
        running: Option<Running>,
        children: BTreeMap<Key, PacketJob>,
    },
}

/// Owner-thread continuation. Native jobs borrow only shared immutable paint inputs.
pub struct PacketJob(Option<Source>);

impl From<DrawPacket> for PacketJob {
    fn from(packet: DrawPacket) -> Self {
        Self::ready(packet)
    }
}

impl PacketJob {
    pub fn ready(packet: DrawPacket) -> Self {
        Self(Some(Source::Ready(packet)))
    }

    pub fn stack(jobs: impl IntoIterator<Item = Self>) -> Self {
        Self(Some(Source::Stack(jobs.into_iter().collect())))
    }

    /// Place the result without waiting for child preparation on the owner thread.
    pub fn placed(self, offset: (i32, i32)) -> Self {
        if offset == (0, 0) {
            self
        } else {
            Self(Some(Source::Placed(Box::new(self), offset)))
        }
    }

    pub fn rasterize(
        self,
        size: (u32, u32),
        cache: Arc<std::sync::OnceLock<Arc<RasterImage>>>,
        clip: Rect,
        host: &RenderHost,
        threads: std::num::NonZeroU32,
        wake: Wake,
    ) -> Self {
        Self(Some(Source::Raster(Box::new(Raster::new(
            self, size, cache, clip, host, threads, wake,
        )))))
    }

    pub fn prepare(
        renderer: Rc<RefCell<Renderer>>,
        frame: &PaintFrame,
        mode: RasterMode,
        repair: &Region,
        wake: Wake,
        mut child: impl FnMut(Key, &Region, Wake) -> Result<Self, String>,
    ) -> Result<Self, String> {
        let host = renderer.borrow().host().clone();
        let _pass = host.begin_pass(0);
        let mut children = BTreeMap::new();
        for (key, repair) in frame.external_repairs(repair) {
            children.insert(key, child(key, &repair, wake.clone())?);
        }
        let mut state = renderer.borrow_mut();
        let pending = state.plan_preparation(frame, mode, PaintDemand::Repair(repair));
        let running = pending.as_ref().map(|pending| {
            pending
                .plan
                .dispatch(&host, &state.text_fonts, move || wake())
                .expect("asynchronous preparation cannot start inside a native pass")
        });
        state.preparing = true;
        let epoch = state.epoch;
        drop(state);
        Ok(Self(Some(Source::Renderer {
            renderer,
            epoch,
            frame: frame.freeze(),
            repair: repair.clone(),
            pending,
            running,
            children,
        })))
    }

    pub fn poll(&mut self) -> Option<Result<DrawPacket, PacketError>> {
        match self.0.as_mut()? {
            Source::Draining(job, _) => {
                if !job.cancel_and_poll() {
                    return None;
                }
                let Some(Source::Draining(_, error)) = self.0.take() else {
                    unreachable!()
                };
                return Some(Err(error));
            }
            Source::Ready(_) => {}
            Source::Placed(job, offset) => {
                let result = job
                    .poll()?
                    .map(|packet| DrawPacket::placed(Arc::new(packet), *offset));
                self.0.take();
                return Some(result);
            }
            Source::Raster(raster) => {
                let result = raster.poll()?;
                self.0.take();
                return Some(result);
            }
            Source::Stack(jobs) => {
                let mut ready = true;
                for job in jobs {
                    match job.settle() {
                        Ok(done) => ready &= done,
                        Err(error) => {
                            return self.abort(error);
                        }
                    }
                }
                if !ready {
                    return None;
                }
            }
            Source::Renderer {
                renderer,
                epoch,
                running,
                children,
                ..
            } => {
                if renderer.borrow().epoch != *epoch {
                    return self.abort(PacketError::Invalidated);
                }
                let mut ready = running.as_mut().is_none_or(Running::ready);
                for child in children.values_mut() {
                    match child.settle() {
                        Ok(done) => ready &= done,
                        Err(error) => {
                            return self.abort(error);
                        }
                    }
                }
                if !ready {
                    return None;
                }
            }
        }
        let packet = match self.0.take().unwrap() {
            Source::Ready(packet) => Ok(packet),
            Source::Raster(_) | Source::Draining(..) | Source::Placed(..) => unreachable!(),
            Source::Stack(mut jobs) => Ok(DrawPacket::stack(jobs.iter_mut().map(|job| {
                job.poll()
                    .expect("settled packet")
                    .expect("settled success")
            }))),
            Source::Renderer {
                renderer,
                frame,
                repair,
                pending,
                running,
                children,
                ..
            } => {
                let mut state = renderer.borrow_mut();
                state.preparing = false;
                if let Some(pending) = pending {
                    let results = running
                        .expect("planned work has a completion")
                        .wait(state.host());
                    state.publish_preparation(Completed { pending, results });
                }
                state
                    .freeze_frame(&frame, &repair, |_| {
                        children
                            .into_iter()
                            .map(|(key, mut job)| {
                                job.poll()
                                    .expect("settled child")
                                    .map(|packet| (key, packet))
                                    .map_err(|e| format!("child preparation: {e:?}"))
                            })
                            .collect()
                    })
                    .map_err(PacketError::Failed)
            }
        };
        Some(packet)
    }

    fn abort(&mut self, error: PacketError) -> Option<Result<DrawPacket, PacketError>> {
        if self.cancel_and_poll() {
            self.cancel();
            Some(Err(error))
        } else {
            let job = Self(self.0.take());
            self.0 = Some(Source::Draining(Box::new(job), error));
            None
        }
    }

    // Keep reservations alive until every worker has finished; never join from poll.
    fn cancel_and_poll(&mut self) -> bool {
        match self.0.as_mut() {
            Some(Source::Renderer {
                running, children, ..
            }) => {
                let mut done = running.as_mut().is_none_or(Running::cancel_and_poll);
                for child in children.values_mut() {
                    done &= child.cancel_and_poll();
                }
                done
            }
            Some(Source::Stack(jobs)) => {
                let mut done = true;
                for job in jobs {
                    done &= job.cancel_and_poll();
                }
                done
            }
            Some(Source::Raster(raster)) => raster.cancel_and_poll(),
            Some(Source::Draining(job, _)) => job.cancel_and_poll(),
            Some(Source::Placed(job, _)) => job.cancel_and_poll(),
            Some(Source::Ready(_)) | None => true,
        }
    }

    fn settle(&mut self) -> Result<bool, PacketError> {
        if matches!(self.0, Some(Source::Ready(_))) {
            return Ok(true);
        }
        match self.poll() {
            None => Ok(false),
            Some(Ok(packet)) => {
                self.0 = Some(Source::Ready(packet));
                Ok(true)
            }
            Some(Err(error)) => Err(error),
        }
    }

    fn cancel(&mut self) {
        if let Some(Source::Renderer {
            renderer,
            epoch,
            running,
            pending,
            ..
        }) = self.0.take()
        {
            drop(running);
            drop(pending);
            let mut renderer = renderer.borrow_mut();
            if renderer.epoch == epoch {
                renderer.clear_cache();
            }
        }
    }
}

impl Drop for PacketJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests;
