use retinue_ui::{
    Bounds,
    input::{
        Button, Event, FocusPolicy, HitShape, Key, Phase, PointerId, PointerPhase, Router, Target,
    },
    prelude::*,
    runtime::{ViewId, ViewTree},
};
use std::{cell::RefCell, rc::Rc};

fn pointer(id: u64, phase: PointerPhase, x: f32, y: f32) -> Event {
    Event::Pointer {
        id: PointerId(id),
        phase,
        position: (x, y),
        modifiers: Default::default(),
    }
}
fn key(key: Key, shift: bool) -> Event {
    Event::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: retinue_ui::input::Modifiers {
            shift,
            ..Default::default()
        },
    }
}
fn target(id: u64, x: f32, input: InputOptions) -> Target {
    Target {
        id: ViewId(id),
        parent: None,
        bounds: Bounds::new(x, 0.0, 100.0, 40.0),
        clip: Bounds::new(x, 0.0, 100.0, 40.0),
        enabled: true,
        input,
    }
}

#[test]
fn retained_input_uses_logical_geometry_and_observes_only_consumed_interaction() {
    #[derive(PartialEq)]
    struct Control {
        status: Interaction,
        count: State<u32>,
    }
    impl View for Control {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            Rectangle::new(if self.status.hovered() {
                Color::rgb(0xeeeeee)
            } else {
                Color::rgb(0x333333)
            })
            .frame(100.0, 40.0)
            .input(
                InputOptions::new(EventHandler::with(self.count.clone(), |count, event, _| {
                    if matches!(
                        event,
                        Event::Pointer {
                            phase: PointerPhase::Down(Button::Primary),
                            ..
                        }
                    ) {
                        count.update(|n| *n += 1);
                    }
                }))
                .interaction(self.status.clone())
                .focusable(FocusPolicy::Tab),
            )
        }
    }
    for scale in [1.0, 1.25, 2.0] {
        let status = Interaction::default();
        let count = State::new(0);
        let mut tree = ViewTree::new(
            Control {
                status: status.clone(),
                count: count.clone(),
            }
            .padding(10.0),
            || {},
        );
        let size = ((180.0 * scale) as u32, (90.0 * scale) as u32);
        tree.update(size, scale).unwrap();
        assert_eq!(tree.input().targets().len(), 1);
        assert_eq!(
            tree.input().targets()[0].bounds,
            Bounds::new(10.0, 10.0, 100.0, 40.0)
        );
        tree.dispatch(&pointer(0, PointerPhase::Move, 15.0, 15.0));
        assert!(status.hovered());
        assert!(tree.needs_update());
        tree.update(size, scale).unwrap();
        assert_eq!(tree.stats.bodies, 1);
        tree.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 15.0, 15.0));
        assert_eq!(count.get(), 1);
        assert!(status.focused());
        assert!(!status.focus_visible());
        // Neither focus nor count is read by the body.
        assert!(!tree.needs_update());
        tree.dispatch(&key(Key::Space, false));
        assert!(status.focus_visible());
        tree.dispatch(&pointer(0, PointerPhase::Leave, 15.0, 15.0));
        tree.update(size, scale).unwrap();
        assert!(!status.hovered());
        assert_eq!(tree.stats.bodies, 1);
    }
}

#[test]
fn propagation_visits_ancestors_once_per_phase_with_local_coordinates() {
    let visits = Rc::new(RefCell::new(Vec::new()));
    let handler = EventHandler::new({
        let visits = visits.clone();
        move |event, context| {
            if matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(_),
                    ..
                }
            ) {
                visits.borrow_mut().push((
                    context.current,
                    context.phase,
                    context.local_position(),
                ));
            }
        }
    });
    let mut router = Router::default();
    let root = target(1, 0.0, InputOptions::new(handler.clone()));
    let mut child = target(2, 20.0, InputOptions::new(handler));
    child.parent = Some(root.id);
    router.update(vec![root, child]);
    router.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 25.0, 10.0));
    assert_eq!(
        *visits.borrow(),
        vec![
            (ViewId(1), Phase::Capture, Some((25.0, 10.0))),
            (ViewId(2), Phase::Target, Some((5.0, 10.0))),
            (ViewId(1), Phase::Bubble, Some((25.0, 10.0))),
        ]
    );
}

#[test]
fn capture_survives_reordering_moves_outside_and_ends_on_last_button_release() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let status = Interaction::default();
    let input = InputOptions::new(EventHandler::new({
        let events = events.clone();
        move |event, context| {
            events.borrow_mut().push(event.clone());
            if matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(Button::Primary),
                    ..
                }
            ) {
                context.capture_pointer();
            }
        }
    }))
    .interaction(status.clone());
    let a = target(1, 0.0, input);
    let b = target(2, 120.0, InputOptions::default());
    let mut router = Router::default();
    router.update(vec![a.clone(), b.clone()]);
    router.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 5.0, 5.0));
    assert!(status.captured());
    router.update(vec![b, a]);
    router.dispatch(&pointer(
        0,
        PointerPhase::Down(Button::Secondary),
        150.0,
        5.0,
    ));
    router.dispatch(&pointer(0, PointerPhase::Up(Button::Secondary), 150.0, 5.0));
    assert_eq!(router.captured(PointerId(0)), Some(ViewId(1)));
    router.dispatch(&pointer(0, PointerPhase::Move, 400.0, 100.0));
    router.dispatch(&pointer(0, PointerPhase::Leave, 400.0, 100.0));
    let dispatch = router.dispatch(&pointer(0, PointerPhase::Up(Button::Primary), 400.0, 100.0));
    assert!(dispatch.capture_changed);
    assert!(!status.captured());
    assert!(!status.hovered());
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        Event::Pointer {
            phase: PointerPhase::Up(Button::Primary),
            position: (400.0, 100.0),
            ..
        }
    )));
    assert_eq!(
        events.borrow().last(),
        Some(&Event::CaptureLost(PointerId(0)))
    );
}

#[test]
fn focus_traversal_skips_disabled_targets_preserves_alt_tab_and_cancels_drag() {
    let a = Interaction::default();
    let b = Interaction::default();
    let cancels = Rc::new(RefCell::new(Vec::new()));
    let first = target(
        1,
        0.0,
        InputOptions::new(EventHandler::new({
            let cancels = cancels.clone();
            move |event, context| {
                if matches!(
                    event,
                    Event::Pointer {
                        phase: PointerPhase::Down(_),
                        ..
                    }
                ) {
                    context.capture_pointer();
                }
                if matches!(
                    event,
                    Event::Pointer {
                        phase: PointerPhase::Cancel,
                        ..
                    } | Event::CaptureLost(_)
                        | Event::Focused(_)
                ) {
                    cancels.borrow_mut().push(event.clone());
                }
            }
        }))
        .focusable(FocusPolicy::Tab)
        .interaction(a.clone()),
    );
    let second = target(
        2,
        120.0,
        InputOptions::default()
            .focusable(FocusPolicy::Text)
            .interaction(b.clone()),
    );
    let mut disabled = target(
        3,
        240.0,
        InputOptions::default().focusable(FocusPolicy::Tab),
    );
    disabled.enabled = false;
    let mut router = Router::default();
    router.update(vec![first.clone(), second.clone(), disabled]);
    router.dispatch(&key(Key::Tab, false));
    assert!(a.focused() && a.focus_visible());
    router.dispatch(&key(Key::Tab, false));
    assert!(b.focused() && b.focus_visible());
    router.dispatch(&key(Key::Tab, false));
    assert_eq!(router.focused(), Some(ViewId(1)));
    router.dispatch(&key(Key::Tab, true));
    assert_eq!(router.focused(), Some(ViewId(2)));
    router.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 5.0, 5.0));
    assert!(a.focused() && !a.focus_visible() && a.captured());
    router.dispatch(&Event::WindowFocused(false));
    assert!(!a.focused() && !a.focus_visible() && !a.captured());
    assert_eq!(router.focused(), Some(ViewId(1)));
    assert!(cancels.borrow().iter().any(|e| matches!(
        e,
        Event::Pointer {
            phase: PointerPhase::Cancel,
            ..
        }
    )));
    router.dispatch(&key(Key::Tab, false));
    assert_eq!(router.focused(), Some(ViewId(1)));
    router.dispatch(&Event::WindowFocused(true));
    assert!(a.focused() && !a.focus_visible());
    router.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 125.0, 5.0));
    assert!(b.focused() && b.focus_visible());
    router.update(vec![first]);
    assert_eq!(router.focused(), None);
    assert!(!b.focused() && !b.focus_visible());
}

#[test]
fn captures_are_pointer_specific_and_unmount_resets_retained_status() {
    let a = Interaction::default();
    let b = Interaction::default();
    let input = |status| {
        InputOptions::new(EventHandler::new(|event, context| {
            if matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(_),
                    ..
                }
            ) {
                context.capture_pointer();
            }
        }))
        .interaction(status)
    };
    let first = target(1, 0.0, input(a.clone()));
    let second = target(2, 120.0, input(b.clone()));
    let mut router = Router::default();
    router.update(vec![first, second.clone()]);
    router.dispatch(&pointer(1, PointerPhase::Down(Button::Primary), 5.0, 5.0));
    router.dispatch(&pointer(2, PointerPhase::Down(Button::Primary), 125.0, 5.0));
    assert!(a.captured() && b.captured());
    router.update(vec![second]);
    assert!(!a.captured() && !a.hovered());
    assert!(b.captured());
    router.dispatch(&pointer(2, PointerPhase::Cancel, 125.0, 5.0));
    assert!(!b.captured());
}

#[test]
fn rounded_hit_shape_clipping_and_disabled_ancestors_match_paint_placement() {
    let mut tree = ViewTree::new(
        ZStack::new((
            Rectangle::new(Color::rgb(0))
                .frame(100.0, 40.0)
                .input(InputOptions::default())
                .id("back"),
            Rectangle::new(Color::rgb(1))
                .frame(100.0, 40.0)
                .input(InputOptions::default().shape(HitShape::Rounded(20.0)))
                .id("front"),
        )),
        || {},
    );
    tree.update((200, 100), 1.0).unwrap();
    let targets = tree.input().targets();
    let (back, front) = (targets[0].id, targets[1].id);
    assert_eq!(tree.input().hit_test((0.0, 0.0)), Some(back));
    assert_eq!(tree.input().hit_test((20.0, 20.0)), Some(front));
    tree.set_root(
        Rectangle::new(Color::rgb(0))
            .frame(100.0, 40.0)
            .input(InputOptions::default())
            .offset(50.0, 0.0)
            .frame(100.0, 40.0)
            .clipped(),
    );
    tree.update((200, 100), 1.0).unwrap();
    assert!(tree.input().hit_test((90.0, 10.0)).is_some());
    assert!(tree.input().hit_test((120.0, 10.0)).is_none());
    tree.set_root(
        Rectangle::new(Color::rgb(0))
            .frame(100.0, 40.0)
            .input(InputOptions::default().focusable(FocusPolicy::Tab))
            .hit_testing(false),
    );
    tree.update((200, 100), 1.0).unwrap();
    assert!(tree.input().hit_test((20.0, 20.0)).is_none());
    tree.dispatch(&key(Key::Tab, false));
    assert_eq!(tree.input().focused(), None);
}

#[test]
fn input_handler_replacement_does_not_record_or_relayout_and_capture_gets_latest_handler() {
    fn handler(value: &State<u32>, event: &Event, cx: &mut EventContext) {
        if matches!(
            event,
            Event::Pointer {
                phase: PointerPhase::Down(_),
                ..
            }
        ) {
            cx.capture_pointer();
        }
        if matches!(
            event,
            Event::Pointer {
                phase: PointerPhase::Up(_),
                ..
            }
        ) {
            value.update(|n| *n += 1);
        }
    }
    let a = State::new(0);
    let b = State::new(0);
    let view = |model| {
        Rectangle::new(Color::rgb(0x334455))
            .frame(100.0, 40.0)
            .on_event(EventHandler::with(model, handler))
    };
    let mut tree = ViewTree::new(view(a.clone()), || {});
    tree.update((200, 100), 1.0).unwrap();
    tree.set_root(view(a.clone()));
    assert!(!tree.needs_update());
    tree.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 5.0, 5.0));
    let capture = tree.input().captured(PointerId(0));
    tree.set_root(view(b.clone()));
    assert_eq!(
        tree.update((200, 100), 1.0).unwrap(),
        retinue_render::damage::Damage::None
    );
    assert_eq!(tree.stats.projected, 0);
    assert_eq!(tree.stats.input_updates, 1);
    assert_eq!(tree.input().captured(PointerId(0)), capture);
    tree.dispatch(&pointer(0, PointerPhase::Up(Button::Primary), 400.0, 100.0));
    assert_eq!(a.get(), 0);
    assert_eq!(b.get(), 1);
}

#[test]
fn paint_only_updates_retain_input_without_rebuilding_targets() {
    #[derive(PartialEq)]
    struct Painted(State<Color>);
    impl View for Painted {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            Rectangle::new(self.0.get())
                .frame(100.0, 40.0)
                .interactive()
        }
    }
    let color = State::new(Color::rgb(0));
    let mut tree = ViewTree::new(Painted(color.clone()), || {});
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(tree.stats.input_updates, 1);
    let down = tree.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 15.0, 15.0));
    color.set(Color::rgb(0x445566));
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(tree.stats.input_updates, 0);
    assert_eq!(tree.input().captured(PointerId(0)), down.target);
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(tree.stats.input_updates, 0);
    assert_eq!(
        tree.dispatch(&pointer(0, PointerPhase::Up(Button::Primary), 15.0, 15.0))
            .activated,
        down.target
    );
}

#[test]
fn named_interactive_children_share_routing_with_explicit_input_ancestors() {
    let phases = Rc::new(RefCell::new(Vec::new()));
    let handler = EventHandler::new({
        let phases = phases.clone();
        move |event, context| {
            if matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(_),
                    ..
                }
            ) {
                phases.borrow_mut().push(context.phase);
            }
        }
    });
    let mut tree = ViewTree::new(
        Rectangle::new(Color::rgb(0))
            .frame(80.0, 40.0)
            .interactive()
            .id("child")
            .padding(10.0)
            .on_event(handler),
        || {},
    );
    tree.update((200, 100), 1.25).unwrap();
    let hit = tree.hit_test((15.0, 15.0)).unwrap();
    assert_eq!(hit, retinue_ui::Key::from("child"));
    let down = tree.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 15.0, 15.0));
    assert_eq!(down.target.and_then(|id| tree.input_key(id)), Some(hit));
    assert_eq!(*phases.borrow(), vec![Phase::Capture, Phase::Bubble]);
    let up = tree.dispatch(&pointer(0, PointerPhase::Up(Button::Primary), 15.0, 15.0));
    assert_eq!(up.activated, down.target);
}

#[test]
fn input_clips_follow_incremental_placement_without_painting() {
    #[derive(PartialEq)]
    struct Clipped(State<f32>);
    impl View for Clipped {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            Rectangle::new(Color::rgb(0))
                .frame(100.4, 40.0)
                .interactive()
                .id("child")
                .offset(50.2, 0.0)
                .frame(100.4, 40.0)
                .clipped()
                .offset(self.0.get(), 0.0)
        }
    }
    for scale in [1.0, 1.25, 2.0] {
        let x = State::new(0.0);
        let mut tree = ViewTree::new(Clipped(x.clone()), || {});
        tree.update((500, 200), scale).unwrap();
        let hit = tree.hit_test((90.0, 10.0));
        assert!(hit.is_some());
        assert_eq!(tree.hit_test((100.5, 10.0)), None);
        let down = tree.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 90.0, 10.0));
        x.set(40.3);
        tree.update((500, 200), scale).unwrap();
        assert_eq!(tree.hit_test((90.0, 10.0)), None);
        assert_eq!(tree.hit_test((130.0, 10.0)), hit);
        assert_eq!(tree.hit_test((140.8, 10.0)), None);
        assert_eq!(tree.input().captured(PointerId(0)), down.target);
        assert_eq!(
            tree.dispatch(&pointer(0, PointerPhase::Up(Button::Primary), 90.0, 10.0))
                .activated,
            None
        );
        let mut fresh = ViewTree::new(Clipped(State::new(40.3)), || {});
        fresh.update((500, 200), scale).unwrap();
        for point in [(90.0, 10.0), (130.0, 10.0), (140.8, 10.0)] {
            assert_eq!(tree.hit_test(point), fresh.hit_test(point));
        }
    }
}

#[test]
fn capture_handler_can_intercept_and_notifications_cannot_steal_focus_recursively() {
    let hits = State::new(0);
    let parent = InputOptions::new(EventHandler::new(|event, cx| {
        if cx.phase == Phase::Capture
            && matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(_),
                    ..
                }
            )
        {
            cx.handle();
            cx.stop_propagation();
        }
        if matches!(event, Event::Focused(false)) {
            cx.focus();
        }
    }))
    .focusable(FocusPolicy::Tab);
    let mut a = target(1, 0.0, parent);
    a.bounds.width = 220.0;
    a.clip.width = 220.0;
    let mut b = target(
        2,
        120.0,
        InputOptions::new(EventHandler::with(hits.clone(), |hits, event, _| {
            if matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Down(_),
                    ..
                }
            ) {
                hits.update(|n| *n += 1);
            }
        }))
        .focusable(FocusPolicy::Tab),
    );
    b.parent = Some(a.id);
    let mut router = Router::default();
    router.update(vec![a, b]);
    router.focus(Some(ViewId(1)));
    assert!(
        router
            .dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 125.0, 5.0))
            .handled
    );
    assert_eq!(hits.get(), 0);
    assert_eq!(router.focused(), Some(ViewId(1)));
    router.focus(Some(ViewId(2)));
    assert_eq!(router.focused(), Some(ViewId(2)));
}
