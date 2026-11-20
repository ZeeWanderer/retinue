use super::*;
use crate::work::{Executor, Graph, Output, Priority, Submission};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{OnceLock, mpsc},
};

pub(super) struct Raster {
    source: Option<PacketJob>,
    size: (u32, u32),
    cache: Arc<OnceLock<Arc<RasterImage>>>,
    clip: Rect,
    executor: Arc<Executor>,
    threads: NonZeroU32,
    wake: Wake,
    task: Option<Task>,
}

struct Task {
    submission: Submission,
    output: Output<Arc<RasterImage>>,
    ready: bool,
}

impl Raster {
    pub(super) fn cancel_and_poll(&mut self) -> bool {
        if let Some(source) = &mut self.source {
            return source.cancel_and_poll();
        }
        self.task.as_mut().is_none_or(|task| {
            if !task.ready {
                task.submission.cancel();
            }
            task.poll_ready()
        })
    }

    pub(super) fn new(
        source: PacketJob,
        size: (u32, u32),
        cache: Arc<OnceLock<Arc<RasterImage>>>,
        clip: Rect,
        host: &RenderHost,
        threads: NonZeroU32,
        wake: Wake,
    ) -> Self {
        Self {
            source: Some(source),
            size,
            cache,
            clip,
            executor: host.executor(),
            threads: host.task_threads(threads),
            wake,
            task: None,
        }
    }

    pub(super) fn poll(&mut self) -> Option<Result<DrawPacket, PacketError>> {
        if let Some(source) = &mut self.source {
            let packet = match source.poll()? {
                Ok(packet) => packet,
                Err(error) => return Some(Err(error)),
            };
            self.source.take();
            let (width, height) = self.size;
            let cache = self.cache.clone();
            let threads = self.threads;
            let mut graph = Graph::default();
            let output = graph.add(
                &[],
                NonZeroUsize::new(threads.get() as usize).unwrap(),
                move |_| {
                    let render = || -> Result<Arc<RasterImage>, String> {
                        if let Some(image) = cache.get() {
                            return Ok(image.clone());
                        }
                        let length = (width as usize)
                            .checked_mul(height as usize)
                            .and_then(|pixels| pixels.checked_mul(4))
                            .ok_or("raster dimensions overflow")?;
                        let mut pixels = vec![0; length];
                        let mut painter =
                            Painter::with_threads(&mut pixels, width, height, threads)
                                .map_err(|error| error.to_string())?;
                        packet.draw(&mut painter);
                        painter.finish().map_err(|error| error.to_string())?;
                        let image = Arc::new(
                            RasterImage::from_bgra(pixels, width, height)
                                .map_err(|error| error.to_string())?,
                        );
                        let _ = cache.set(image.clone());
                        Ok(image)
                    };
                    render().map_err(|error| crate::work::Failure::Failed(error.into()))
                },
            );
            let wake = self.wake.clone();
            match self
                .executor
                .submit(graph, Priority::Foreground, move || wake())
            {
                Ok(submission) => {
                    self.task = Some(Task {
                        submission,
                        output,
                        ready: false,
                    })
                }
                Err(error) => {
                    return Some(Err(PacketError::Failed(format!(
                        "raster submission: {error:?}"
                    ))));
                }
            }
        }
        let task = self
            .task
            .as_mut()
            .expect("raster submitted after source completion");
        if !task.poll_ready() {
            return None;
        }
        Some(match task.output.read() {
            Some(Ok(image)) => Ok(DrawPacket::image(image.clone(), (0, 0), self.clip)),
            Some(Err(error)) => Err(PacketError::Failed(format!("raster execution: {error:?}"))),
            None => Err(PacketError::Failed(
                "raster completion preceded its output".into(),
            )),
        })
    }
}

impl Task {
    fn poll_ready(&mut self) -> bool {
        self.ready |= !matches!(
            self.submission.try_complete(),
            Err(mpsc::TryRecvError::Empty)
        );
        self.ready
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if !self.ready {
            self.submission.cancel();
            let _ = self.submission.wait();
        }
    }
}
