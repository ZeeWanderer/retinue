use super::*;
use crate::{
    Bounds,
    input::{InputOptions, Interaction},
};

fn target(id: u64, x: f32) -> Target {
    let bounds = Bounds::new(x, 0.0, 40.0, 40.0);
    Target {
        id: ViewId(id),
        parent: None,
        bounds,
        clip: bounds,
        enabled: true,
        input: InputOptions {
            interaction: Some(Interaction::default()),
            focus: FocusPolicy::Tab,
            activation: Activation::Release,
            ..Default::default()
        },
    }
}

fn pointer(id: u64, phase: PointerPhase, x: f32) -> Event {
    Event::Pointer {
        id: PointerId(id),
        phase,
        position: (x, 10.0),
        modifiers: Default::default(),
    }
}

fn check_status(router: &Router) {
    for target in &router.targets {
        let status = target.input.interaction.as_ref().unwrap();
        let focused = router.focus == Some(target.id);
        assert_eq!(status.hovered(), router.hovered.contains(&target.id));
        assert_eq!(status.focused(), focused && router.active);
        assert_eq!(
            status.focus_visible(),
            router
                .modality
                .shows(focused, target.input.focus == FocusPolicy::Text)
        );
        assert_eq!(
            status.captured(),
            router.capture.values().any(|id| *id == target.id)
        );
    }
}

#[test]
fn motion_scans_once_and_only_publishes_changed_interactions() {
    let mut router = Router::default();
    router.update(
        (1..=1024)
            .map(|id| target(id, (id - 1) as f32 * 40.0))
            .collect(),
    );
    router.publications.set(0);
    router.dispatch(&pointer(0, PointerPhase::Move, 10.0));
    assert_eq!(router.scans.get(), 1);
    assert_eq!(router.publications.get(), 1);
    for _ in 0..32 {
        router.dispatch(&pointer(0, PointerPhase::Move, 10.0));
        assert_eq!(router.hit_test((10.0, 10.0)), Some(ViewId(1)));
    }
    assert_eq!(router.scans.get(), 1);
    assert_eq!(router.publications.get(), 1);
    router.dispatch(&pointer(0, PointerPhase::Move, 11.0));
    assert_eq!(router.scans.get(), 2);
    assert_eq!(router.publications.get(), 1);
    router.dispatch(&pointer(0, PointerPhase::Move, 51.0));
    assert_eq!(router.scans.get(), 3);
    assert_eq!(router.publications.get(), 3);
    check_status(&router);
}

#[test]
fn reflow_rechecks_hits_and_topology_refreshes_ancestors_and_replaced_status() {
    let mut a = target(1, 100.0);
    let b = target(2, 200.0);
    let mut child = target(3, 0.0);
    child.parent = Some(a.id);
    let mut router = Router::default();
    router.update(vec![a.clone(), b.clone(), child.clone()]);
    router.dispatch(&pointer(0, PointerPhase::Move, 10.0));
    assert!(a.input.interaction.as_ref().unwrap().hovered());
    child.parent = Some(b.id);
    router.update(vec![a.clone(), b.clone(), child.clone()]);
    assert!(!a.input.interaction.as_ref().unwrap().hovered());
    assert!(b.input.interaction.as_ref().unwrap().hovered());
    check_status(&router);
    let old = child
        .input
        .interaction
        .replace(Interaction::default())
        .unwrap();
    router.update(vec![a.clone(), b.clone(), child.clone()]);
    assert!(!old.hovered());
    assert!(child.input.interaction.as_ref().unwrap().hovered());
    check_status(&router);
    child.bounds.x = 50.0;
    a.bounds.x = 0.0;
    a.clip = a.bounds;
    router.update(vec![a.clone(), b.clone(), child.clone()]);
    assert_eq!(router.hit_test((10.0, 10.0)), Some(a.id));
    assert!(!child.input.interaction.as_ref().unwrap().hovered());
    check_status(&router);
    router.update(vec![a.clone(), b, target(4, 0.0)]);
    assert_eq!(router.hit_test((10.0, 10.0)), Some(ViewId(4)));
    assert!(!a.input.interaction.as_ref().unwrap().hovered());
    check_status(&router);
}

#[test]
fn shared_hover_and_capture_survive_another_pointers_leave_or_release() {
    let a = target(1, 0.0);
    let status = a.input.interaction.clone().unwrap();
    let mut router = Router::default();
    router.update(vec![a]);
    for id in [1, 2] {
        assert!(
            router
                .dispatch(&pointer(id, PointerPhase::Down(Button::Primary), 10.0))
                .capture_changed
        );
        check_status(&router);
    }
    assert!(
        router
            .dispatch(&pointer(1, PointerPhase::Up(Button::Primary), 10.0))
            .capture_changed
    );
    assert!(status.captured());
    router.dispatch(&pointer(1, PointerPhase::Leave, 10.0));
    assert!(status.hovered());
    check_status(&router);
    router.dispatch(&pointer(2, PointerPhase::Leave, 10.0));
    assert!(!status.hovered() && status.captured());
    check_status(&router);
    let result = router.dispatch(&pointer(2, PointerPhase::Up(Button::Primary), 10.0));
    assert!(result.capture_changed);
    assert!(result.activated.is_none());
    assert!(!status.hovered() && !status.captured());
    check_status(&router);
}

#[test]
fn capture_transfer_reports_a_change_even_when_capture_count_is_unchanged() {
    let mut root = target(1, 0.0);
    root.input.handler = Some(crate::input::EventHandler::new(|event, context| {
        if context.phase == Phase::Capture
            && matches!(
                event,
                Event::Pointer {
                    phase: PointerPhase::Move,
                    ..
                }
            )
        {
            context.capture_pointer();
        }
    }));
    let mut child = target(2, 50.0);
    child.parent = Some(root.id);
    let mut router = Router::default();
    router.update(vec![root, child]);
    router.dispatch(&pointer(0, PointerPhase::Down(Button::Primary), 60.0));
    assert_eq!(router.captured(PointerId::MOUSE), Some(ViewId(2)));
    let transferred = router.dispatch(&pointer(0, PointerPhase::Move, 60.0));
    assert!(transferred.capture_changed);
    assert_eq!(router.captured(PointerId::MOUSE), Some(ViewId(1)));
    check_status(&router);
    assert!(
        !router
            .dispatch(&pointer(0, PointerPhase::Move, 60.0))
            .capture_changed
    );
    check_status(&router);
}

#[test]
fn focus_modality_and_window_lifecycle_republish_the_affected_status() {
    let mut router = Router::default();
    router.update(vec![target(1, 0.0), target(2, 50.0)]);
    for event in [
        pointer(0, PointerPhase::Down(Button::Primary), 10.0),
        Event::Key {
            key: Key::Space,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        },
        pointer(0, PointerPhase::Up(Button::Primary), 10.0),
        Event::Key {
            key: Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        },
        pointer(0, PointerPhase::Down(Button::Primary), 60.0),
        Event::WindowFocused(false),
        Event::WindowFocused(true),
    ] {
        router.dispatch(&event);
        check_status(&router);
    }
    router.focus(Some(ViewId(1)));
    check_status(&router);
    router.focus(None);
    check_status(&router);
}
