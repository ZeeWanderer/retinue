use super::*;
use crate::{
    environment::EnvironmentKey,
    input::{Button, Event, EventHandler, InputOptions, PointerId, PointerPhase},
};
use std::cell::RefCell;

struct Width;
impl EnvironmentKey for Width {
    type Value = f32;
    fn default_value() -> f32 {
        10.0
    }
}

#[derive(PartialEq)]
struct Sample {
    index: usize,
    values: Rc<RefCell<Vec<(f32, u32)>>>,
}
#[view]
impl View for Sample {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let width = context.environment().get::<Width>();
        let model = context.environment().require::<State<u32>>().unwrap();
        self.values.borrow_mut()[self.index] = (width, model.get());
        Canvas(Fill).frame(Frame::new().width(width).height(10.0))
    }
}

#[test]
fn local_providers_inherit_override_and_update_without_a_second_frame() {
    let mut graph = InstanceGraph::default();
    let environment = Environment::default();
    let parent_model = State::new(1_u32);
    let local_model = State::new(2_u32);
    environment.provide(parent_model.clone());
    let width = State::new(20.0);
    let read = width.clone();
    let model = local_model.clone();
    let values = Rc::new(RefCell::new(vec![(0.0, 0); 3]));
    let output = values.clone();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    HStack(0.0) {
                        compose(Sample { index: 0, values: output.clone() })
                        compose(Sample { index: 1, values: output.clone() })
                            .environment::<Width>(read.get()).environment_model(model.clone())
                        compose(Sample { index: 2, values: output.clone() })
                    }
                }
            },
            environment.clone(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(*values.borrow(), [(10.0, 1), (20.0, 2), (10.0, 1)]);
    width.set(30.0);
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    assert!(!graph.has_pending_inputs());
    assert_eq!(advance(&mut graph).work.owners_evaluated, 0);
    environment.set::<Width>(40.0);
    local_model.set(3);
    advance(&mut graph);
    assert_eq!(*values.borrow(), [(40.0, 1), (30.0, 3), (40.0, 1)]);
    assert!(!graph.has_pending_inputs());
}

#[test]
fn scopes_cross_keyed_boundaries_and_remove_old_model_dependencies() {
    let mut graph = InstanceGraph::default();
    let model = State::new(State::new(1_u32));
    let old_model = model.get();
    let read = model.clone();
    let values = Rc::new(RefCell::new(vec![(0.0, 0); 2]));
    let output = values.clone();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    HStack(0.0) {
                        ForEach([0_u64, 1], |index| *index) { |index|
                            compose(Sample { index: index as usize, values: output.clone() })
                                .environment::<Width>(20.0 + index as f32)
                        }
                    }.environment_model(read.get())
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(*values.borrow(), [(20.0, 1), (21.0, 1)]);
    model.set(State::new(2));
    advance(&mut graph);
    assert_eq!(*values.borrow(), [(20.0, 2), (21.0, 2)]);
    assert!(!graph.has_pending_inputs());
    old_model.set(3);
    assert!(!graph.has_pending_inputs());
}

fn pointer(x: f32, y: f32) -> Event {
    Event::Pointer {
        id: PointerId::MOUSE,
        phase: PointerPhase::Down(Button::Primary),
        position: (x, y),
        modifiers: Default::default(),
    }
}

#[test]
fn input_updates_without_layout_and_uses_selected_branch_regions() {
    let mut graph = InstanceGraph::default();
    let value = State::new(1);
    let read = value.clone();
    let seen = Rc::new(Cell::new(0));
    let output = seen.clone();
    let selected = State::new(true);
    let branch = selected.clone();
    let enabled = State::new(true);
    let available = enabled.clone();
    let root = graph.mount_body(move |_| {
        ui! {
            Group() {
                if branch.get() {
                    Canvas(Fill).frame(Frame::new().width(40.0).height(20.0))
                }
            }.input(InputOptions::new(EventHandler::with(
                (read.get(), output.clone()),
                |(value, seen), event, _| {
                    if matches!(event, Event::Pointer { phase: PointerPhase::Down(_), .. }) {
                        seen.set(*value);
                    }
                },
            ))).enabled(available.get())
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let target = graph.input().targets()[0].id;
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 1);
    let frame = graph.scene().frame().generation();
    value.set(2);
    let update = advance(&mut graph);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(update.work.input_updates, 1);
    assert_eq!(graph.scene().frame().generation(), frame);
    assert_eq!(graph.input().targets()[0].id, target);
    graph.dispatch(&pointer(10.0, 10.0));
    assert_eq!(seen.get(), 2);
    enabled.set(false);
    advance(&mut graph);
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    enabled.set(true);
    selected.set(false);
    advance(&mut graph);
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    selected.set(true);
    advance(&mut graph);
    assert_eq!(graph.input().hit_test((10.0, 10.0)), Some(target));
}

#[derive(PartialEq)]
struct ClockSample {
    values: Rc<RefCell<Vec<(f32, u32)>>>,
}

#[view]
impl View for ClockSample {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        use crate::timeline::Schedule;
        let now = context.time(Schedule::WallClock {
            interval: std::time::Duration::from_secs(1),
        });
        let width = context.environment().get::<Width>();
        let seconds = now
            .wall_time
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.values.borrow_mut().push((width, seconds as u32));
        Text(seconds.to_string())
    }
}

#[test]
fn a_timeline_inside_an_environment_and_conditional_scope_keeps_its_deadline() {
    use crate::timeline::FrameTime;
    use std::time::{Duration, Instant, SystemTime};
    let time = FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH + Duration::from_secs(10),
    };
    let mut graph = InstanceGraph::default();
    let selected = State::new(true);
    let read = selected.clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    Group() {
                        if read.get() { compose(ClockSample { values: seen.clone() }) }
                    }.environment::<Width>(42.0)
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph.advance_at((400, 300), 1.0, time, None).unwrap();
    assert_eq!(*values.borrow(), [(42.0, 10)]);
    assert_eq!(
        graph.next_deadline(),
        Some(time.instant + Duration::from_secs(1))
    );
    let early = graph
        .advance_at(
            (400, 300),
            1.0,
            time.at(time.instant + Duration::from_millis(500)),
            None,
        )
        .unwrap();
    assert_eq!(early.work.owners_evaluated, 0);
    let next = time.at(time.instant + Duration::from_secs(1));
    graph.advance_at((400, 300), 1.0, next, None).unwrap();
    assert_eq!(*values.borrow(), [(42.0, 10), (42.0, 11)]);
    selected.set(false);
    graph.advance_at((400, 300), 1.0, next, None).unwrap();
    assert!(graph.next_deadline().is_none());
}
