use super::Lease;
use retinue_render::{
    DrawPacket, PacketError, PacketJob, RenderHost, Wake,
    surface::{RenderWork, Submission},
    work::{Executor, Graph, OwnedOutput, Priority},
};
#[cfg(feature = "benchmark")]
use std::time::Instant;
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{Arc, mpsc},
};

pub struct Rendered {
    pub lease: Lease,
    pub submission: Submission,
    #[cfg(feature = "benchmark")]
    pub finished: Instant,
}

struct Target {
    lease: Lease,
    work: RenderWork,
    background: [u8; 4],
}

struct Native {
    submission: retinue_render::work::Submission,
    result: OwnedOutput<Rendered>,
    finished: bool,
}

enum State {
    Preparing { packet: PacketJob, target: Target },
    Painting(Native),
    Discarded,
}

/// One frame continuation. Drop joins native jobs before mappings or code can unload.
pub struct RenderJob {
    state: Option<State>,
    executor: Arc<Executor>,
    threads: NonZeroU32,
    wake: Wake,
}

impl RenderJob {
    pub fn submit(
        host: &RenderHost,
        threads: NonZeroU32,
        lease: Lease,
        work: RenderWork,
        packet: impl Into<PacketJob>,
        background: [u8; 4],
        wake: Wake,
    ) -> Result<Self, String> {
        let mut job = Self {
            state: Some(State::Preparing {
                packet: packet.into(),
                target: Target {
                    lease,
                    work,
                    background,
                },
            }),
            executor: host.executor(),
            threads: host.task_threads(threads),
            wake,
        };
        // Pending preparation and native painting both own a completion wake.
        // A discarded frame has no worker to tell the host to reclaim its slot.
        job.start_if_ready()?;
        if matches!(job.state, Some(State::Discarded)) {
            (job.wake)();
        }
        Ok(job)
    }

    fn paint(&self, packet: DrawPacket, target: Target) -> Result<Native, String> {
        let Target {
            mut lease,
            work,
            background,
        } = target;
        let threads = self.threads;
        let mut graph = Graph::default();
        let result = graph.add_owned(
            &[],
            NonZeroUsize::new(threads.get() as usize).unwrap(),
            move |_| {
                let result = (|| {
                    let pixels = lease.pixels().ok_or("leased target is unavailable")?;
                    work.execute(
                        pixels.target,
                        pixels.source.map(|(_, source)| source),
                        threads,
                        |p, repair| {
                            if background[3] == 255 {
                                let r = repair.bounds();
                                p.fill_rounded_rect(r.x, r.y, r.width, r.height, 0, background);
                            } else {
                                p.clear();
                                if background[3] != 0 {
                                    return Err("unsupported translucent surface base".into());
                                }
                            }
                            packet.draw(p);
                            Ok(())
                        },
                    )
                })();
                match result {
                    Ok(submission) => Ok(Rendered {
                        lease,
                        submission,
                        #[cfg(feature = "benchmark")]
                        finished: Instant::now(),
                    }),
                    Err(error) => Err(retinue_render::work::Failure::Failed(error.into())),
                }
            },
        );
        let wake = self.wake.clone();
        let submission = self
            .executor
            .submit(graph, Priority::Foreground, move || wake())
            .map_err(|error| format!("native submission: {error:?}"))?;
        Ok(Native {
            submission,
            result,
            finished: false,
        })
    }

    fn start_if_ready(&mut self) -> Result<(), String> {
        let Some(State::Preparing { packet, .. }) = &mut self.state else {
            return Ok(());
        };
        let Some(result) = packet.poll() else {
            return Ok(());
        };
        let Some(State::Preparing { target, .. }) = self.state.take() else {
            unreachable!()
        };
        self.state = Some(match result {
            Ok(packet) => State::Painting(self.paint(packet, target)?),
            Err(PacketError::Invalidated) => State::Discarded,
            Err(PacketError::Failed(error)) => return Err(error),
        });
        Ok(())
    }

    /// `Ok(None)` discards a frame whose renderer was reset during preparation.
    pub fn poll(&mut self) -> Option<Result<Option<Rendered>, String>> {
        if let Err(error) = self.start_if_ready() {
            return Some(Err(error));
        }
        if matches!(self.state, Some(State::Discarded)) {
            self.state.take();
            return Some(Ok(None));
        }
        let Some(State::Painting(native)) = self.state.as_mut() else {
            return None;
        };
        match native.submission.try_complete() {
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                native.finished = true;
                return Some(Err("native executor disconnected".into()));
            }
            Ok(_) => native.finished = true,
        }
        let result = match native.result.take() {
            Some(Ok(result)) => Ok(Some(result)),
            Some(Err(error)) => Err(format!("native render: {error:?}")),
            None => Err("native completion preceded its result".into()),
        };
        self.state.take();
        Some(result)
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        if !self.finished {
            self.submission.cancel();
            let _ = self.submission.wait();
        }
    }
}
