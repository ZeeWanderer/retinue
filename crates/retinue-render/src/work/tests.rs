use super::*;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn pool(participants: usize, tasks: usize) -> Executor {
    Executor::new(Limits {
        participants: NonZeroUsize::new(participants).unwrap(),
        tasks,
    })
}

fn wait(submission: &Submission) -> Report {
    let start = Instant::now();
    loop {
        match submission.try_complete() {
            Ok(report) => return report,
            Err(mpsc::TryRecvError::Empty) if start.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(1));
            }
            other => panic!("completion missing: {other:?}"),
        }
    }
}

#[test]
fn independent_jobs_overlap_then_publish_dependency_order() {
    let pool = pool(2, 8);
    let barrier = Arc::new(Barrier::new(2));
    let mut graph = Graph::default();
    let outputs: Vec<_> = (1..=2)
        .map(|i| {
            let barrier = barrier.clone();
            graph.add(&[], NonZeroUsize::MIN, move |_| {
                barrier.wait();
                Ok(i)
            })
        })
        .collect();
    let dependencies: Vec<_> = outputs.iter().map(Output::id).collect();
    let sum = graph.add(&dependencies, NonZeroUsize::MIN, move |_| {
        Ok(outputs
            .iter()
            .map(|v| *v.read().unwrap().unwrap())
            .sum::<usize>())
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(
        wait(&submission),
        Report {
            completed: 3,
            ..Report::default()
        }
    );
    assert_eq!(sum.into_result().ok().unwrap(), Ok(3));
    assert_eq!(pool.stats().participants, 0);
}

#[test]
fn owned_jobs_move_non_sync_storage_through_dependencies_without_copying() {
    use std::cell::Cell;
    let pool = pool(2, 8);
    let mut graph = Graph::default();
    let storage = vec![0u8; 1024];
    let address = storage.as_ptr() as usize;
    let mut first = graph.add_owned(&[], NonZeroUsize::MIN, move |_| Ok(Cell::new(storage)));
    assert!(first.take().is_none());
    let mut second = graph.add_owned(&[first.id()], NonZeroUsize::MIN, move |_| {
        let storage = first.take().unwrap()?;
        let mut bytes = storage.into_inner();
        assert_eq!(bytes.as_ptr() as usize, address);
        bytes[513] = 42;
        Ok(Cell::new(bytes))
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).completed, 2);
    let bytes = second.take().unwrap().unwrap().into_inner();
    assert_eq!(bytes.as_ptr() as usize, address);
    assert_eq!(bytes[513], 42);
    assert!(second.take().is_none());
}

#[test]
fn cooperative_batches_use_the_reserved_width_and_include_the_calling_worker() {
    for width in [1, 2, 4] {
        let pool = pool(width, 8);
        let barrier = Arc::new(Barrier::new(width));
        let mut graph = Graph::default();
        let mut output = graph.add_parallel_owned(
            &[],
            NonZeroUsize::new(width).unwrap(),
            move |_, parallel| {
                let caller = std::thread::current().id();
                let mut values: Vec<_> = (0..width).map(|_| std::cell::Cell::new(None)).collect();
                let address = values.as_ptr() as usize;
                parallel.for_each(&mut values, |value| {
                    assert!(
                        std::thread::current()
                            .name()
                            .unwrap()
                            .starts_with("retinue-cpu-")
                    );
                    barrier.wait();
                    value.set(Some(std::thread::current().id()));
                });
                assert_eq!(address, values.as_ptr() as usize);
                let threads: std::collections::HashSet<_> =
                    values.iter().map(|v| v.get().unwrap()).collect();
                assert_eq!(threads.len(), width);
                assert!(threads.contains(&caller));
                Ok(values)
            },
        );
        let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
        assert_eq!(wait(&submission).completed, 1);
        assert_eq!(output.take().unwrap().unwrap().len(), width);
        assert_eq!(pool.stats().participants, 0);
    }
}

#[test]
fn ready_batches_balance_work_without_using_unreserved_workers() {
    let pool = pool(6, 8);
    let mut graph = Graph::default();
    let mut output = graph.add_parallel_owned(&[], NonZeroUsize::new(2).unwrap(), |_, parallel| {
        let barrier = Barrier::new(2);
        let (finished, wait) = std::sync::mpsc::channel();
        let wait = std::sync::Mutex::new(wait);
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let mut values: Vec<_> = (0..24)
            .map(|index| (index, std::cell::Cell::new(None)))
            .collect();
        parallel.for_each(&mut values, |(index, thread)| {
            let running = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(running, Ordering::SeqCst);
            thread.set(Some(std::thread::current().id()));
            if *index < 2 {
                barrier.wait();
            }
            if *index == 0 {
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            } else if *index == 23 {
                finished.send(()).unwrap();
            }
            active.fetch_sub(1, Ordering::SeqCst);
        });
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        let threads: std::collections::HashSet<_> = values
            .iter()
            .map(|(_, thread)| thread.get().unwrap())
            .collect();
        assert_eq!(threads.len(), 2);
        Ok(())
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).completed, 1);
    assert_eq!(output.take(), Some(Ok(())));
}

#[test]
fn cooperative_batch_panic_joins_siblings_before_publishing_failure() {
    let pool = pool(4, 8);
    let finished = Arc::new(AtomicUsize::new(0));
    let count = finished.clone();
    let mut graph = Graph::default();
    let mut output =
        graph.add_parallel_owned(&[], NonZeroUsize::new(4).unwrap(), move |_, parallel| {
            parallel.for_each(&mut [0, 1, 2, 3], |index| {
                if *index == 0 {
                    panic!("test batch failure");
                }
                std::thread::sleep(Duration::from_millis(10));
                count.fetch_add(1, Ordering::Relaxed);
            });
            Ok(())
        });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).failed, 1);
    assert_eq!(output.take().unwrap(), Err(Failure::Panicked));
    assert_eq!(finished.load(Ordering::Relaxed), 3);
    assert_eq!(pool.stats().participants, 0);
}

#[test]
fn owned_failures_and_abandoned_results_release_resources() {
    let pool = pool(1, 8);
    let mut graph = Graph::default();
    let mut failure = graph.add_owned::<()>(&[], NonZeroUsize::MIN, |_| panic!("test failure"));
    let mut dependent = graph.add_owned(&[failure.id()], NonZeroUsize::MIN, |_| Ok(1));
    let token = Arc::new(());
    let resource = token.clone();
    let abandoned = graph.add_owned(&[], NonZeroUsize::MIN, move |_| Ok(resource));
    drop(abandoned);
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).failed, 2);
    assert_eq!(failure.take(), Some(Err(Failure::Panicked)));
    assert_eq!(dependent.take(), Some(Err(Failure::Dependency)));
    assert_eq!(Arc::strong_count(&token), 1);

    let mut graph = Graph::default();
    let mut cancelled = graph.add_owned(&[], NonZeroUsize::MIN, |_| Ok(1));
    drop(graph);
    assert_eq!(cancelled.take(), Some(Err(Failure::Cancelled)));

    let mut graph = Graph::default();
    let mut rejected = graph.add_owned(&[], NonZeroUsize::new(2).unwrap(), |_| Ok(1));
    assert!(pool.submit(graph, Priority::Foreground, || {}).is_err());
    assert_eq!(rejected.take(), Some(Err(Failure::Rejected)));
}

#[test]
fn duplicate_edges_are_one_dependency_and_foreign_edges_are_rejected() {
    let pool = pool(1, 4);
    let mut graph = Graph::default();
    let first = graph.add(&[], NonZeroUsize::MIN, |_| Ok(1));
    let second = graph.add(&[first.id(), first.id()], NonZeroUsize::MIN, |_| Ok(2));
    let mut other = Graph::default();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            other.add(&[first.id()], NonZeroUsize::MIN, |_| Ok(()));
        }))
        .is_err()
    );
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).completed, 2);
    assert_eq!(second.read(), Some(Ok(&2)));
}

#[test]
fn panics_fail_dependents_but_not_independent_jobs_or_pool() {
    let pool = pool(2, 8);
    let mut graph = Graph::default();
    let failure = graph.add::<()>(&[], NonZeroUsize::MIN, |_| panic!("test job"));
    let skipped = graph.add::<()>(&[failure.id()], NonZeroUsize::MIN, |_| {
        panic!("must not run")
    });
    let independent = graph.add(&[], NonZeroUsize::MIN, |_| Ok(7));
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(
        wait(&submission),
        Report {
            completed: 1,
            failed: 2,
            cancelled: 0
        }
    );
    assert_eq!(failure.read(), Some(Err(&Failure::Panicked)));
    assert_eq!(skipped.read(), Some(Err(&Failure::Dependency)));
    assert_eq!(independent.read(), Some(Ok(&7)));
    let submission = pool
        .submit(Graph::default(), Priority::Foreground, || {})
        .unwrap();
    assert_eq!(wait(&submission), Report::default());
}

#[test]
fn weighted_jobs_reserve_native_participants_before_their_caller_runs() {
    let pool = pool(4, 32);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::default();
    for width in [3, 2, 1, 4, 1, 2, 2, 4] {
        let (active, peak) = (active.clone(), peak.clone());
        graph.add(&[], NonZeroUsize::new(width).unwrap(), move |_| {
            let n = active.fetch_add(width, Ordering::SeqCst) + width;
            peak.fetch_max(n, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            active.fetch_sub(width, Ordering::SeqCst);
            Ok(())
        });
    }
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).completed, 8);
    assert!(peak.load(Ordering::SeqCst) <= 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[test]
fn background_leaves_owner_capacity_and_pause_is_nonblocking() {
    let pool = pool(3, 8);
    let pause = pool.pause_background();
    let mut graph = Graph::default();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    for _ in 0..8 {
        let (active, peak) = (active.clone(), peak.clone());
        graph.add(&[], NonZeroUsize::MIN, move |_| {
            peak.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        });
    }
    let submission = pool.submit(graph, Priority::Background, || {}).unwrap();
    assert_eq!(pool.stats().queued, 8);
    assert_eq!(pool.stats().running, 0);
    drop(pause);
    assert_eq!(wait(&submission).completed, 8);
    assert!(peak.load(Ordering::SeqCst) <= 2);
}

#[test]
fn cancellation_rejection_and_drop_release_result_leases() {
    let pool = pool(2, 1);
    let pause = pool.pause_background();
    let mut graph = Graph::default();
    let output = graph.add(&[], NonZeroUsize::MIN, |_| Ok(42));
    let submission = pool.submit(graph, Priority::Background, || {}).unwrap();
    let mut rejected = Graph::default();
    let extra = rejected.add(&[], NonZeroUsize::MIN, |_| Ok(99));
    assert!(matches!(
        pool.submit(rejected, Priority::Foreground, || {}),
        Err(SubmitError::Full)
    ));
    assert_eq!(extra.read(), Some(Err(&Failure::Rejected)));
    submission.cancel();
    drop(pause);
    assert_eq!(wait(&submission).cancelled, 1);
    assert_eq!(output.read(), Some(Err(&Failure::Cancelled)));

    let pause = pool.pause_background();
    let mut graph = Graph::default();
    let output = graph.add(&[], NonZeroUsize::MIN, |_| Ok(1));
    let submission = pool.submit(graph, Priority::Background, || {}).unwrap();
    drop(pool);
    assert_eq!(wait(&submission).cancelled, 1);
    assert_eq!(output.read(), Some(Err(&Failure::Cancelled)));
    drop(pause);
}

#[test]
fn native_rasters_are_independent_and_match_serial_pixels() {
    use crate::painter::RasterImage;
    fn tile(index: u32) -> RasterImage {
        RasterImage::render(40, 30, |painter| {
            painter.fill_rounded_rect(index, index, 30, 20, 4, [21, 170, 240, 255]);
        })
        .unwrap()
    }
    let pool = pool(3, 8);
    let mut graph = Graph::default();
    let tiles: Vec<_> = (0..4)
        .map(|index| graph.add(&[], NonZeroUsize::MIN, move |_| Ok(tile(index))))
        .collect();
    let deps: Vec<_> = tiles.iter().map(Output::id).collect();
    let row = graph.add(&deps, NonZeroUsize::new(3).unwrap(), move |_| {
        Ok(RasterImage::render_with_threads(
            160,
            30,
            std::num::NonZeroU32::new(3).unwrap(),
            |painter| {
                for (index, tile) in tiles.iter().enumerate() {
                    painter.draw_image(tile.read().unwrap().unwrap(), index as u32 * 40, 0, 40, 30);
                }
            },
        )
        .unwrap())
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    assert_eq!(wait(&submission).completed, 5);
    let expected = RasterImage::render(160, 30, |painter| {
        for index in 0..4 {
            painter.draw_image(&tile(index), index * 40, 0, 40, 30);
        }
    })
    .unwrap();
    assert_eq!(
        row.read().unwrap().unwrap().to_rgba().unwrap(),
        expected.to_rgba().unwrap()
    );
}

#[test]
fn drop_waits_for_native_work_and_worker_thread_destructors() {
    struct Marker(Arc<AtomicUsize>);
    impl Drop for Marker {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    thread_local! { static MARKER: std::cell::RefCell<Option<Marker>> = const { std::cell::RefCell::new(None) }; }
    let markers = Arc::new(AtomicUsize::new(0));
    let pool = pool(2, 4);
    let (send, receive) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut graph = Graph::default();
    let worker_markers = markers.clone();
    graph.add(&[], NonZeroUsize::MIN, move |_| {
        MARKER.with(|m| *m.borrow_mut() = Some(Marker(worker_markers)));
        send.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok(())
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    let dropper = std::thread::spawn(move || drop(pool));
    release.send(()).unwrap();
    dropper.join().unwrap();
    assert_eq!(wait(&submission).completed, 1);
    assert_eq!(markers.load(Ordering::SeqCst), 1);
}

#[test]
fn foreground_native_pass_is_not_starved_by_background_refills() {
    let pool = pool(3, 8);
    let (started, start) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let mut first = Graph::default();
    first.add(&[], NonZeroUsize::MIN, move |_| {
        started.send(()).unwrap();
        wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok(())
    });
    let first = pool.submit(first, Priority::Background, || {}).unwrap();
    start.recv_timeout(Duration::from_secs(5)).unwrap();
    let pass_finished = Arc::new(AtomicUsize::new(0));
    let finished = pass_finished.clone();
    let mut frame = Graph::default();
    frame.add(&[], NonZeroUsize::new(3).unwrap(), move |_| {
        finished.store(1, Ordering::Release);
        Ok(())
    });
    let frame = pool.submit(frame, Priority::Foreground, || {}).unwrap();
    let mut refill = Graph::default();
    refill.add(&[], NonZeroUsize::MIN, move |_| {
        assert_eq!(pass_finished.load(Ordering::Acquire), 1);
        Ok(())
    });
    let refill = pool.submit(refill, Priority::Background, || {}).unwrap();
    assert_eq!(pool.stats().queued, 2);
    release.send(()).unwrap();
    assert_eq!(wait(&first).completed, 1);
    assert_eq!(wait(&frame).completed, 1);
    assert_eq!(wait(&refill).completed, 1);
}

#[test]
fn abandoned_graph_completes_its_output_without_running() {
    let mut graph = Graph::default();
    let output = graph.add::<()>(&[], NonZeroUsize::MIN, |_| panic!("abandoned work ran"));
    drop(graph);
    assert_eq!(output.read(), Some(Err(&Failure::Cancelled)));
}

#[test]
fn admission_accounts_for_completed_nodes_retained_by_a_running_graph() {
    let pool = pool(2, 3);
    let (started, start) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let mut graph = Graph::default();
    let a = graph.add(&[], NonZeroUsize::MIN, |_| Ok(()));
    let b = graph.add(&[a.id()], NonZeroUsize::MIN, |_| Ok(()));
    graph.add(&[b.id()], NonZeroUsize::MIN, move |_| {
        started.send(()).unwrap();
        wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok(())
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    start.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(pool.stats().retained, 3);
    let mut next = Graph::default();
    next.add(&[], NonZeroUsize::MIN, |_| Ok(()));
    assert!(matches!(
        pool.submit(next, Priority::Foreground, || {}),
        Err(SubmitError::Full)
    ));
    release.send(()).unwrap();
    assert_eq!(wait(&submission).completed, 3);
    assert_eq!(pool.stats().retained, 0);
}
