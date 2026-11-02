use retinue_ui::{prelude::*, runtime::ViewTree};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

const SECOND: Duration = Duration::from_secs(1);

fn start() -> FrameTime {
    FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH + SECOND * 10,
    }
}

#[derive(PartialEq)]
struct Clock {
    schedule: State<Schedule>,
    evaluations: Rc<Cell<u32>>,
}

impl View for Clock {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        self.evaluations.set(self.evaluations.get() + 1);
        let time = cx.time(self.schedule.get());
        Text::new(
            time.wall_time
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
        )
    }
}

#[test]
fn deadlines_invalidate_only_ticking_views_without_busy_frames_or_missed_frame_catchup() {
    let time = start();
    let calls = Rc::new(Cell::new(0));
    let schedule = State::new(Schedule::Periodic {
        start: time.instant,
        interval: SECOND,
    });
    let mut tree = ViewTree::new(
        HStack::new(
            10.0,
            (
                Clock {
                    schedule: schedule.clone(),
                    evaluations: calls.clone(),
                },
                Text::new("Static"),
            ),
        ),
        || {},
    );
    tree.update_at((300, 80), 1.0, time).unwrap();
    assert_eq!(tree.next_deadline(), Some(time.instant + SECOND));
    for millis in [10, 100, 500, 999] {
        tree.update_at(
            (300, 80),
            1.0,
            time.at(time.instant + Duration::from_millis(millis)),
        )
        .unwrap();
        assert_eq!(tree.stats.bodies, 0);
    }
    tree.update_at((300, 80), 1.0, time.at(time.instant + SECOND * 10))
        .unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(tree.stats.bodies, 1);
    assert_eq!(tree.next_deadline(), Some(time.instant + SECOND * 11));
    schedule.set(Schedule::Paused);
    tree.update_at((300, 80), 1.0, time.at(time.instant + SECOND * 10))
        .unwrap();
    assert_eq!(tree.next_deadline(), None);
    let before = calls.get();
    tree.update_at((300, 80), 1.0, time.at(time.instant + SECOND * 20))
        .unwrap();
    assert_eq!(calls.get(), before);
    assert_eq!(tree.stats.bodies, 0);
}

#[test]
fn unmount_removes_deadlines_and_a_body_can_stop_using_a_timeline() {
    #[derive(PartialEq)]
    struct Conditional {
        ticking: State<bool>,
        visible: State<bool>,
        start: Instant,
    }
    impl View for Conditional {
        fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
            if self.ticking.get() {
                cx.time(Schedule::Periodic {
                    start: self.start,
                    interval: SECOND,
                });
            }
            if self.visible.get() {
                Text::new("Visible").into_element()
            } else {
                empty()
            }
        }
    }
    let time = start();
    let ticking = State::new(true);
    let mut tree = ViewTree::new(
        Conditional {
            ticking: ticking.clone(),
            visible: State::new(true),
            start: time.instant,
        },
        || {},
    );
    tree.update_at((200, 60), 1.0, time).unwrap();
    assert!(tree.next_deadline().is_some());
    ticking.set(false);
    tree.update_at((200, 60), 1.0, time).unwrap();
    assert_eq!(tree.next_deadline(), None);
    ticking.set(true);
    tree.update_at((200, 60), 1.0, time).unwrap();
    assert!(tree.next_deadline().is_some());
    tree.set_root(Text::new("Replacement"));
    tree.update_at((200, 60), 1.0, time).unwrap();
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn wall_clock_jump_only_invalidates_wall_clock_consumers_and_one_shot_stops() {
    let time = start();
    let monotonic = Rc::new(Cell::new(0));
    let wall = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(
        HStack::new(
            10.0,
            (
                Clock {
                    schedule: State::new(Schedule::At(time.instant + SECOND * 60)),
                    evaluations: monotonic.clone(),
                },
                Clock {
                    schedule: State::new(Schedule::AtDate(time.wall_time + SECOND * 60)),
                    evaluations: wall.clone(),
                },
            ),
        ),
        || {},
    );
    tree.update_at((300, 80), 1.0, time).unwrap();
    let jump = FrameTime {
        instant: time.instant + SECOND,
        wall_time: time.wall_time + SECOND * 120,
    };
    tree.update_at((300, 80), 1.0, jump).unwrap();
    assert_eq!(monotonic.get(), 1);
    assert_eq!(wall.get(), 2);
    assert_eq!(tree.next_deadline(), Some(time.instant + SECOND * 60));
    tree.update_at((300, 80), 1.0, jump.at(time.instant + SECOND * 60))
        .unwrap();
    assert_eq!(monotonic.get(), 2);
    assert_eq!(wall.get(), 2);
    assert_eq!(tree.next_deadline(), None);
    // A clock correction can make an absolute-date timer relevant again.
    tree.update_at(
        (300, 80),
        1.0,
        FrameTime {
            instant: time.instant + SECOND * 61,
            wall_time: time.wall_time + SECOND * 30,
        },
    )
    .unwrap();
    assert_eq!(wall.get(), 3);
    assert_eq!(tree.next_deadline(), Some(time.instant + SECOND * 91));
}

#[test]
fn invalid_custom_deadline_is_reported_not_polled() {
    #[derive(Clone)]
    struct Invalid;
    impl TimelineSchedule for Invalid {
        fn next(&self, time: FrameTime) -> Option<Instant> {
            Some(time.instant)
        }
    }
    #[derive(PartialEq)]
    struct BadClock;
    impl View for BadClock {
        fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
            cx.time(Invalid);
            Text::new("Static")
        }
    }
    let time = start();
    let mut tree = ViewTree::new(BadClock, || {});
    tree.update_at((200, 60), 1.0, time).unwrap();
    assert_eq!(tree.stats.invalid_deadlines, 1);
    assert_eq!(tree.next_deadline(), None);
    assert!(!tree.needs_update());
}
