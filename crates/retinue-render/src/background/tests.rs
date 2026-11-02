use super::*;
use crate::{Bounds, Color, Command};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) fn program(key: u64) -> PaintProgram {
    PaintProgram::new(
        key,
        (320.0, 320.0),
        1.0,
        Rc::new(
            (0..1024)
                .map(|i| Command::Fill {
                    bounds: Bounds::new((i % 32 * 10) as f32, (i / 32 * 10) as f32, 8.0, 8.0),
                    radius: 0.0,
                    color: Color::rgba(40, 120, 180, 160),
                })
                .collect(),
        ),
    )
    .unwrap()
}

fn complete(background: &mut Background) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while background.stats().jobs > 0 {
        background.collect();
        assert!(Instant::now() < deadline, "analysis did not complete");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn queue_is_bounded_deduplicated_and_publishes_only_original_live_programs() {
    let mut background = Background::new(AnalysisPolicy {
        jobs: 2,
        ..AnalysisPolicy::desktop()
    });
    let pause = background.pause();
    let old = program(1);
    let other = program(2);
    let replacement = program(1);
    assert!(background.submit(&old));
    assert!(background.submit(&old));
    assert!(background.submit(&other));
    assert!(!background.submit(&replacement));
    let stats = background.stats();
    assert_eq!(stats.jobs, 2);
    assert_eq!(stats.submitted, 2);
    assert!(stats.reserved_bytes > 0 && stats.reserved_bytes <= background.policy.bytes);
    assert!(!stats.running);
    let resources = Rc::downgrade(old.commands());
    drop(old);
    assert!(
        resources.upgrade().is_none(),
        "worker retained source commands"
    );
    drop(pause);
    complete(&mut background);
    assert!(!replacement.has_replay_index());
    assert!(other.has_replay_index());
    assert_eq!(background.stats().published, 1);
    assert_eq!(background.stats().discarded, 1);
    assert_eq!(background.stats().reserved_bytes, 0);
    assert!(background.submit(&replacement));
    complete(&mut background);
    assert!(replacement.has_replay_index());
    let (target, _) = replacement.analysis_target();
    assert!(!target.publish(other.id(), ReplayIndex::build(&[], || false).unwrap()));
}

#[test]
fn byte_admission_precedes_snapshot_allocation_and_teardown_joins_paused_worker() {
    let source = program(1);
    let mut small = Background::new(AnalysisPolicy {
        bytes: 1,
        ..AnalysisPolicy::desktop()
    });
    assert!(small.submit(&source));
    assert_eq!(small.stats().jobs, 0);
    assert!(small.worker.is_none());

    let mut background = Background::new(AnalysisPolicy::desktop());
    let pause = background.pause();
    assert!(background.submit(&source));
    let shared = Arc::downgrade(&background.shared);
    drop(background);
    assert!(!source.has_replay_index());
    drop(pause);
    assert!(shared.upgrade().is_none(), "worker outlived host teardown");
}

#[test]
fn snapshots_and_results_are_send_but_carry_no_native_resources() {
    fn send<T: Send>() {}
    send::<Job>();
    send::<Completed>();
}
