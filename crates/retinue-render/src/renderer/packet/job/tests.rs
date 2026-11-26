use super::*;
use crate::{
    Bounds, Command, HostPolicy, PaintInstance,
    work::{Graph, Priority},
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::mpsc,
    time::{Duration, Instant},
};

fn frame() -> PaintFrame {
    let mut frame = PaintFrame::default();
    frame
        .replace(
            (80, 40),
            1.0,
            vec![PaintInstance {
                cache: CachePolicy::Prefer,
                ..PaintInstance::new(
                    1.into(),
                    PaintProgram::new(
                        1,
                        (80.0, 40.0),
                        1.0,
                        Arc::new(vec![Command::Fill {
                            bounds: Bounds::new(0.0, 0.0, 80.0, 40.0),
                            radius: 0.0,
                            color: Color::rgb(0x123456),
                        }]),
                    )
                    .unwrap(),
                    (0, 0),
                    Rect {
                        width: 80,
                        height: 40,
                        ..Rect::default()
                    },
                )
            }],
            vec![],
        )
        .unwrap();
    frame
}

fn host(threads: u32) -> RenderHost {
    RenderHost::new(HostPolicy {
        threads,
        preparation_time: Duration::MAX,
        analysis: crate::AnalysisPolicy::disabled(),
        ..HostPolicy::desktop()
    })
}

fn finish(job: &mut PacketJob, wakes: &mpsc::Receiver<()>) -> Result<DrawPacket, PacketError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = job.poll() {
            return result;
        }
        wakes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn preparation_yields_and_retains_the_old_generation_without_copying_commands() {
    for participants in [1, 2] {
        let host = host(participants);
        let renderer = Rc::new(RefCell::new(Renderer::new(ResidencyPolicy::desktop(0))));
        renderer.borrow_mut().set_host(host.clone());
        let executor = host.executor();
        let mut gate = Graph::default();
        let (release, hold) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        gate.add(
            &[],
            NonZeroUsize::new(host.threads() as usize).unwrap(),
            move |_| {
                entered.send(()).unwrap();
                hold.recv().unwrap();
                Ok(())
            },
        );
        let gate = executor.submit(gate, Priority::Foreground, || {}).unwrap();
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut frame = frame();
        let chunks = frame.chunks.clone();
        let repair = Region::from(Rect {
            width: 80,
            height: 40,
            ..Rect::default()
        });
        let (wake, wakes) = mpsc::channel();
        let mut job = PacketJob::prepare(
            renderer.clone(),
            &frame,
            RasterMode::Explicit,
            &repair,
            Arc::new(move || {
                let _ = wake.send(());
            }),
            |_, _, _| unreachable!(),
        )
        .unwrap();
        assert!(job.poll().is_none());
        if let Some(Source::Renderer { frame: frozen, .. }) = &job.0 {
            assert!(Arc::ptr_eq(&chunks, &frozen.chunks));
        } else {
            unreachable!();
        }
        frame
            .patch_resolved(
                [],
                [crate::PlacementUpdate {
                    index: 0,
                    origin: (30, 0),
                    clip: repair.bounds(),
                }],
                [],
            )
            .unwrap();
        assert_eq!(chunks[0].origin(), (0, 0));
        assert_eq!(frame.chunks[0].origin(), (30, 0));
        release.send(()).unwrap();
        gate.wait().unwrap();
        let packet = finish(&mut job, &wakes).unwrap();
        assert!(!renderer.borrow().preparing);
        assert_eq!(host.stats().pending_bytes, 0);
        let pixels = std::thread::spawn(move || {
            let mut pixels = vec![0; 80 * 40 * 4];
            let mut painter = Painter::with_threads(&mut pixels, 80, 40, NonZeroU32::MIN).unwrap();
            packet.draw(&mut painter);
            painter.finish().unwrap();
            pixels
        })
        .join()
        .unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == Color::rgb(0x123456).bgra())
        );
        renderer.borrow_mut().prepare(&frame, RasterMode::Explicit);
        assert_eq!(
            renderer.borrow().prepared_for,
            Some((frame.identity, frame.generation))
        );
    }
}

#[test]
fn reset_or_drop_during_preparation_releases_reservations_and_allows_new_work() {
    for reset in [false, true] {
        let host = host(2);
        let renderer = Rc::new(RefCell::new(Renderer::new(ResidencyPolicy::desktop(0))));
        renderer.borrow_mut().set_host(host.clone());
        let frame = frame();
        let repair = Region::from(Rect {
            width: 80,
            height: 40,
            ..Rect::default()
        });
        let (wake, wakes) = mpsc::channel();
        let mut job = PacketJob::prepare(
            renderer.clone(),
            &frame,
            RasterMode::Explicit,
            &repair,
            Arc::new(move || {
                let _ = wake.send(());
            }),
            |_, _, _| unreachable!(),
        )
        .unwrap();
        if reset {
            renderer.borrow_mut().clear_cache();
            assert!(matches!(
                finish(&mut job, &wakes),
                Err(PacketError::Invalidated)
            ));
        }
        drop(job);
        assert_eq!(host.stats().pending_bytes, 0);
        assert!(!renderer.borrow().preparing);
        renderer.borrow_mut().prepare(&frame, RasterMode::Explicit);
        assert!(
            renderer
                .borrow_mut()
                .freeze(&frame, &repair, |_, _| unreachable!())
                .is_ok()
        );
    }
}

#[test]
fn reset_yields_with_an_occupied_worker_and_keeps_nested_reservations() {
    let host = host(1);
    let renderer = Rc::new(RefCell::new(Renderer::new(ResidencyPolicy::desktop(0))));
    renderer.borrow_mut().set_host(host.clone());
    let (release, hold) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    let mut gate = Graph::default();
    gate.add(&[], NonZeroUsize::MIN, move |_| {
        entered.send(()).unwrap();
        let _ = hold.recv_timeout(Duration::from_secs(5));
        Ok(())
    });
    let gate = host
        .executor()
        .submit(gate, Priority::Foreground, || {})
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let (notify, wakes) = mpsc::channel();
    let wake: Wake = Arc::new(move || {
        let _ = notify.send(());
    });
    let clip = Rect {
        width: 80,
        height: 40,
        ..Rect::default()
    };
    let cache = Arc::new(std::sync::OnceLock::new());
    let mut raster = PacketJob::ready(DrawPacket::stack([])).rasterize(
        (80, 40),
        cache.clone(),
        clip,
        &host,
        NonZeroU32::MIN,
        wake.clone(),
    );
    assert!(raster.poll().is_none());
    let make_job = |mode| {
        PacketJob::prepare(
            renderer.clone(),
            &frame(),
            mode,
            &Region::from(clip),
            wake.clone(),
            |_, _, _| unreachable!(),
        )
        .unwrap()
    };
    let mut parent = make_job(RasterMode::Explicit);
    let Some(Source::Renderer { children, .. }) = parent.0.as_mut() else {
        unreachable!()
    };
    children.insert(2.into(), PacketJob::stack([raster.placed((-8, 3))]));
    renderer.borrow_mut().clear_cache();
    assert!(parent.poll().is_none());
    assert!(host.stats().pending_bytes > 0);
    let mut replacement = make_job(RasterMode::Replay);
    release.send(()).unwrap();
    gate.wait().unwrap();
    assert!(matches!(
        finish(&mut parent, &wakes),
        Err(PacketError::Invalidated)
    ));
    assert!(cache.get().is_none());
    assert_eq!(host.stats().pending_bytes, 0);
    assert!(renderer.borrow().preparing);
    assert!(finish(&mut replacement, &wakes).is_ok());
    assert!(!renderer.borrow().preparing);
    assert!(parent.poll().is_none());
}
