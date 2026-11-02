use retinue_ui::{Bounds, prelude::*, runtime::ViewTree};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

fn start() -> FrameTime {
    FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH,
    }
}
fn linear(ms: u64) -> Animation {
    Animation::new(Duration::from_millis(ms)).easing(Easing::Linear)
}
fn at(tree: &mut ViewTree, time: FrameTime, millis: u64) {
    tree.update_at(
        (400, 160),
        1.0,
        time.at(time.instant + Duration::from_millis(millis)),
    )
    .unwrap();
}
fn target(tree: &ViewTree) -> Bounds {
    tree.input().targets()[0].bounds
}

#[derive(PartialEq)]
struct Moving {
    x: State<f32>,
    width: State<f32>,
    calls: Rc<Cell<u32>>,
}
impl View for Moving {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        self.calls.set(self.calls.get() + 1);
        Rectangle::new(Color::rgb(0x33aa77))
            .frame(self.width.get(), 20.0)
            .input(InputOptions::default())
            .offset(self.x.get(), 0.0)
    }
}

#[test]
fn transaction_animates_presented_geometry_without_rebuilding_parent_or_mutating_target() {
    let time = start();
    let x = State::new(0.0);
    let width = State::new(40.0);
    let calls = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(
        Moving {
            x: x.clone(),
            width: width.clone(),
            calls: calls.clone(),
        },
        || {},
    );
    at(&mut tree, time, 0);
    with_animation(linear(1000), || {
        x.set(100.0);
    });
    at(&mut tree, time, 0);
    assert_eq!(x.get(), 100.0);
    assert_eq!(target(&tree).x, 0.0);
    assert!(tree.next_deadline().is_some());
    at(&mut tree, time, 500);
    assert_eq!(target(&tree).x, 50.0);
    assert_eq!(calls.get(), 2);
    assert_eq!(tree.stats.bodies, 0);
    assert_eq!(tree.scene().stats.recordings, 0);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert!(tree.input().hit_test((55.0, 5.0)).is_some());
    assert!(tree.input().hit_test((5.0, 5.0)).is_none());
    with_animation(linear(1000), || {
        x.set(0.0);
        width.set(80.0);
    });
    at(&mut tree, time, 500);
    assert_eq!(target(&tree).x, 50.0);
    at(&mut tree, time, 1000);
    assert_eq!(target(&tree).x, 25.0);
    assert_eq!(target(&tree).width, 60.0);
    at(&mut tree, time, 1500);
    assert_eq!(target(&tree), Bounds::new(0.0, 0.0, 80.0, 20.0));
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn scoped_trigger_reaches_an_independently_observed_descendant_and_nearest_policy_wins() {
    #[derive(PartialEq)]
    struct Outer {
        x: State<f32>,
        width: State<f32>,
        calls: Rc<Cell<u32>>,
    }
    impl View for Outer {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            let x = self.x.get();
            let moving = Moving {
                x: self.x.clone(),
                width: self.width.clone(),
                calls: self.calls.clone(),
            };
            VStack::new(0.0, (moving.animation(linear(500), x),)).animation(linear(1000), x)
        }
    }
    let time = start();
    let x = State::new(0.0);
    let mut tree = ViewTree::new(
        Outer {
            x: x.clone(),
            width: State::new(40.0),
            calls: Rc::default(),
        },
        || {},
    );
    at(&mut tree, time, 0);
    x.set(100.0);
    at(&mut tree, time, 0);
    at(&mut tree, time, 250);
    assert_eq!(target(&tree).x, 50.0);
    at(&mut tree, time, 500);
    assert_eq!(target(&tree).x, 100.0);
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn global_disable_reduced_motion_and_unmount_end_tracks() {
    let time = start();
    let x = State::new(0.0);
    let mut tree = ViewTree::new(
        Moving {
            x: x.clone(),
            width: State::new(40.0),
            calls: Rc::default(),
        },
        || {},
    );
    at(&mut tree, time, 0);
    with_animation(linear(1000), || {
        x.set(100.0);
    });
    at(&mut tree, time, 0);
    at(&mut tree, time, 200);
    tree.set_motion_enabled(false);
    assert!(tree.needs_update());
    at(&mut tree, time, 200);
    assert_eq!(target(&tree).x, 100.0);
    assert_eq!(tree.next_deadline(), None);
    tree.set_motion_enabled(true);
    tree.environment().set::<ReduceMotion>(true);
    with_animation(linear(1000), || {
        x.set(0.0);
    });
    at(&mut tree, time, 300);
    assert_eq!(target(&tree).x, 0.0);
    assert_eq!(tree.next_deadline(), None);
    tree.environment().set::<ReduceMotion>(false);
    with_animation(linear(1000), || {
        x.set(100.0);
    });
    at(&mut tree, time, 400);
    assert!(tree.next_deadline().is_some());
    tree.set_root(Text::new("Gone"));
    at(&mut tree, time, 500);
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn discrete_style_changes_do_not_schedule_a_fake_animation() {
    let time = start();
    let mut tree = ViewTree::new(Text::new("Hello").width(100.0), || {});
    at(&mut tree, time, 0);
    with_animation(linear(1000), || {
        tree.set_root(Text::new("Hello").fill_width());
    });
    at(&mut tree, time, 0);
    assert_eq!(tree.next_deadline(), None);
}

#[derive(PartialEq)]
struct Sweep {
    progress: f32,
}
impl Animatable for Sweep {
    type Data = f32;
    fn animatable_data(&self) -> f32 {
        self.progress
    }
}
impl AnimatedDrawing for Sweep {
    fn draw(&self, progress: &f32, cx: &mut DrawingContext) {
        cx.fill(
            Bounds::new(0.0, 0.0, 100.0 * progress, 20.0),
            0.0,
            Color::rgb(0x77aaee),
        );
    }
}

#[test]
fn animated_canvas_ticks_drawing_not_its_parent_and_uses_target_snapshots() {
    #[derive(PartialEq)]
    struct Body {
        target: State<f32>,
        calls: Rc<Cell<u32>>,
    }
    impl View for Body {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            self.calls.set(self.calls.get() + 1);
            AnimatedCanvas::new(Sweep {
                progress: self.target.get(),
            })
            .frame(100.0, 20.0)
        }
    }
    let time = start();
    let target = State::new(0.0);
    let calls = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(
        Body {
            target: target.clone(),
            calls: calls.clone(),
        },
        || {},
    );
    at(&mut tree, time, 0);
    with_animation(linear(1000), || {
        target.set(1.0);
    });
    at(&mut tree, time, 0);
    at(&mut tree, time, 500);
    assert_eq!(calls.get(), 2);
    assert_eq!(tree.stats.bodies, 0);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert_eq!(tree.scene().stats.recordings, 1);
    let painted = tree
        .scene()
        .geometry()
        .iter()
        .find(|g| g.paint.width == 50 && g.paint.height == 20);
    assert!(painted.is_some());
    assert_eq!(target.get(), 1.0);
    at(&mut tree, time, 1000);
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn named_custom_value_retargets_without_reset_and_ignores_unrelated_body_changes() {
    #[derive(PartialEq)]
    struct Custom {
        target: State<f32>,
        text: State<String>,
    }
    impl View for Custom {
        fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
            let x = cx.animated_with("x", self.target.get(), linear(1000));
            Text::new(self.text.get())
                .frame(50.0, 20.0)
                .input(InputOptions::default())
                .offset(x, 0.0)
        }
    }
    let time = start();
    let x = State::new(0.0);
    let text = State::new("A".to_owned());
    let mut tree = ViewTree::new(
        Custom {
            target: x.clone(),
            text: text.clone(),
        },
        || {},
    );
    at(&mut tree, time, 0);
    x.set(100.0);
    at(&mut tree, time, 0);
    at(&mut tree, time, 500);
    assert_eq!(target(&tree).x, 50.0);
    text.set("B".to_owned());
    at(&mut tree, time, 750);
    assert_eq!(target(&tree).x, 75.0);
    x.set(0.0);
    at(&mut tree, time, 750);
    assert_eq!(target(&tree).x, 75.0);
    at(&mut tree, time, 1250);
    assert_eq!(target(&tree).x, 37.5);
    tree.environment().set::<ReduceMotion>(true);
    at(&mut tree, time, 1250);
    assert_eq!(target(&tree).x, 0.0);
    assert_eq!(tree.next_deadline(), None);
}
