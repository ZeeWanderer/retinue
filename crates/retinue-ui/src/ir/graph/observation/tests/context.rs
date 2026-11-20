use super::*;
use crate::{
    motion::ReduceMotion,
    state::with_animation,
    timeline::{FrameTime, Schedule, TimelineSchedule},
    transition::{Animation, Easing},
};
use std::time::{Duration, Instant, SystemTime};

const SECOND: Duration = Duration::from_secs(1);

fn start() -> FrameTime {
    FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH + SECOND * 10,
    }
}

fn at(graph: &mut InstanceGraph, time: FrameTime, millis: u64) -> Update {
    let result = graph
        .advance_at(
            (100, 100),
            1.0,
            time.at(time.instant + Duration::from_millis(millis)),
            None,
        )
        .unwrap();
    assert!(result.completion.as_ref().unwrap().is_ready());
    result
}

fn x(graph: &InstanceGraph, root: Handle) -> f32 {
    graph
        .geometry(Address {
            instance: root,
            local: Local(0),
        })
        .unwrap()
        .layout
        .x
}

#[test]
fn retained_state_and_inherited_environment_use_existing_context_contract() {
    let time = start();
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let environment = Environment::default();
    environment.set::<Position>(2.0);
    let local = environment.child();
    let initialized = Rc::new(Cell::new(0));
    let count = initialized.clone();
    let state = Rc::new(RefCell::new(None));
    let retained = state.clone();
    let id = Rc::new(Cell::new(None));
    let identity = id.clone();
    graph
        .observe_in(root, local.clone(), move |context| {
            let model = context.state("position", || {
                count.set(count.get() + 1);
                1.0_f32
            });
            if let Some(id) = identity.replace(Some(context.id())) {
                assert_eq!(id, context.id());
            }
            *retained.borrow_mut() = Some(model.clone());
            model.get() + context.environment().get::<Position>()
        })
        .unwrap();
    at(&mut graph, time, 0);
    assert_eq!(x(&graph, root), 3.0);
    assert_eq!(initialized.get(), 1);
    state.borrow().as_ref().unwrap().set(4.0);
    assert_eq!(at(&mut graph, time, 0).work.owners_evaluated, 1);
    assert_eq!(x(&graph, root), 6.0);
    local.set::<Position>(8.0);
    at(&mut graph, time, 0);
    assert_eq!(x(&graph, root), 12.0);
    environment.set::<Position>(9.0);
    assert!(!graph.has_pending_inputs());
    assert_eq!(initialized.get(), 1);
    graph.set_root(None).unwrap();
    at(&mut graph, time, 0);
    state.borrow().as_ref().unwrap().set(3.0);
    local.set::<Position>(5.0);
    assert!(!graph.has_pending_inputs());
}

#[test]
fn deadlines_skip_idle_owners_and_missed_ticks_without_polling_or_catchup() {
    let time = start();
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let schedule = State::new(Schedule::Periodic {
        start: time.instant,
        interval: SECOND,
    });
    let read = schedule.clone();
    graph
        .observe_in(root, Environment::default(), move |context| {
            context
                .time(read.get())
                .instant
                .duration_since(time.instant)
                .as_secs_f32()
        })
        .unwrap();
    at(&mut graph, time, 0);
    assert_eq!(graph.next_deadline(), Some(time.instant + SECOND));
    for millis in [10, 100, 500, 999] {
        assert_eq!(at(&mut graph, time, millis).work.owners_evaluated, 0);
    }
    assert_eq!(at(&mut graph, time, 10_000).work.owners_evaluated, 1);
    assert_eq!(x(&graph, root), 10.0);
    assert_eq!(graph.next_deadline(), Some(time.instant + SECOND * 11));
    schedule.set(Schedule::Paused);
    at(&mut graph, time, 10_000);
    assert_eq!(graph.next_deadline(), None);
    assert_eq!(at(&mut graph, time, 20_000).work.owners_evaluated, 0);
}

#[test]
fn wall_clock_changes_and_due_deadlines_evaluate_each_owner_once() {
    let time = start();
    let mut graph = InstanceGraph::default();
    let root = graph.mount(crate::ir::graph_tests::outlet(), ());
    let mono = graph.mount(template(), 0.0);
    let wall = graph.mount(template(), 0.0);
    graph
        .children(root, Local(0), vec![(0.into(), mono), (1.into(), wall)])
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph
        .observe_in(mono, Environment::default(), move |context| {
            context.time(Schedule::At(time.instant + SECOND * 60));
            0.0_f32
        })
        .unwrap();
    graph
        .observe_in(wall, Environment::default(), move |context| {
            context
                .time(Schedule::WallClock { interval: SECOND })
                .wall_time
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs_f32()
        })
        .unwrap();
    assert_eq!(at(&mut graph, time, 0).work.owners_evaluated, 2);
    let jump = FrameTime {
        instant: time.instant + SECOND,
        wall_time: time.wall_time + SECOND * 20,
    };
    let result = graph.advance_at((100, 100), 1.0, jump, None).unwrap();
    assert_eq!(result.work.owners_evaluated, 1);
    assert_eq!(x(&graph, wall), 30.0);
    assert_eq!(graph.next_deadline(), Some(jump.instant + SECOND));
}

#[test]
fn animation_sampling_retains_tracks_and_respects_environment_and_host_policy() {
    let time = start();
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let source = State::new(0.0_f32);
    let read = source.clone();
    let environment = Environment::default();
    graph
        .observe_in(root, environment.clone(), move |context| {
            context.animated("position", read.get())
        })
        .unwrap();
    at(&mut graph, time, 0);
    let linear = Animation::new(SECOND).easing(Easing::Linear);
    with_animation(linear, || source.set(100.0));
    at(&mut graph, time, 0);
    assert_eq!(x(&graph, root), 0.0);
    graph.set_frame_interval(Duration::from_millis(20));
    at(&mut graph, time, 0);
    assert_eq!(
        graph.next_deadline(),
        Some(time.instant + Duration::from_millis(20))
    );
    at(&mut graph, time, 500);
    assert_eq!(x(&graph, root), 50.0);
    environment.set::<ReduceMotion>(true);
    at(&mut graph, time, 500);
    assert_eq!(x(&graph, root), 100.0);
    assert_eq!(graph.next_deadline(), None);
    environment.set::<ReduceMotion>(false);
    at(&mut graph, time, 500);
    with_animation(linear, || source.set(0.0));
    at(&mut graph, time, 500);
    at(&mut graph, time, 1000);
    assert_eq!(x(&graph, root), 50.0);
    graph.set_motion_enabled(false);
    assert!(graph.has_pending_inputs());
    at(&mut graph, time, 1000);
    assert_eq!(x(&graph, root), 0.0);
    assert_eq!(graph.next_deadline(), None);
    assert_eq!(source.get(), 0.0);
}

#[test]
fn a_panicking_timer_is_retried_and_invalid_deadlines_do_not_spin() {
    let time = start();
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let fail = Rc::new(Cell::new(true));
    graph
        .observe_in(root, Environment::default(), move |context| {
            let tick = context.time(Schedule::Periodic {
                start: time.instant,
                interval: SECOND,
            });
            if tick.instant > time.instant {
                assert!(!fail.replace(false), "test timer panic");
            }
            1.0_f32
        })
        .unwrap();
    at(&mut graph, time, 0);
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| at(&mut graph, time, 1000))).is_err());
    assert!(graph.has_pending_inputs());
    assert_eq!(at(&mut graph, time, 1000).work.owners_evaluated, 1);
    assert_eq!(graph.next_deadline(), Some(time.instant + SECOND * 2));
    #[derive(Clone)]
    struct Invalid;
    impl TimelineSchedule for Invalid {
        fn next(&self, time: FrameTime) -> Option<Instant> {
            Some(time.instant)
        }
        fn uses_wall_clock(&self) -> bool {
            false
        }
    }
    graph
        .observe_in(root, Environment::default(), |context| {
            context.time(Invalid);
            1.0_f32
        })
        .unwrap();
    assert_eq!(at(&mut graph, time, 1000).work.invalid_deadlines, 1);
    assert_eq!(graph.next_deadline(), None);
    assert_eq!(at(&mut graph, time, 2000).work.owners_evaluated, 0);
}
