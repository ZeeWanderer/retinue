use retinue_ui::{
    Bounds,
    input::{Button, ControlRegion, Controls, Event, HitShape, InputOptions, PointerPhase},
    prelude::*,
    runtime::ViewTree,
};

fn region(key: &'static str, x: f32) -> ControlRegion<&'static str> {
    ControlRegion::new(key, Bounds::new(x, 0.0, 100.0, 40.0))
}

#[test]
fn reflow_rehits_without_pointer_motion_and_release_requires_the_same_target() {
    let mut controls = Controls::default();
    controls.update([region("a", 0.0), region("b", 150.0)]);
    controls.pointer(PointerPhase::Move, (20.0, 20.0));
    controls.update([region("a", 150.0), region("b", 0.0)]);
    assert_eq!(controls.button(true, (20.0, 20.0)).target, Some("b"));
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, Some("b"));
    controls.button(true, (20.0, 20.0));
    controls.update([region("a", 0.0), region("b", 150.0)]);
    assert_eq!(controls.captured(), Some(&"b"));
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
    assert_eq!(controls.captured(), None);
}

#[test]
fn disabled_controls_block_hits_but_cancel_capture() {
    let mut controls = Controls::default();
    controls.update([region("a", 0.0)]);
    controls.button(true, (20.0, 20.0));
    let mut disabled = region("a", 0.0);
    disabled.enabled = false;
    controls.update([region("behind", 0.0), disabled]);
    assert_eq!(controls.hit((20.0, 20.0)), Some(&"a"));
    assert_eq!(controls.captured(), None);
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
    controls.button(true, (20.0, 20.0));
    assert_eq!(controls.captured(), None);
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
}

#[test]
fn reordering_preserves_identity_but_changing_enabled_state_cancels_capture() {
    let mut controls = Controls::default();
    controls.update([region("a", 0.0), region("b", 150.0)]);
    controls.button(true, (20.0, 20.0));
    controls.update([region("b", 0.0), region("a", 150.0)]);
    assert_eq!(controls.captured(), Some(&"a"));
    controls.pointer(PointerPhase::Move, (170.0, 20.0));
    assert_eq!(controls.button(false, (170.0, 20.0)).activated, Some("a"));

    controls.button(true, (170.0, 20.0));
    let mut disabled = region("a", 150.0);
    disabled.enabled = false;
    controls.update([region("b", 0.0), disabled]);
    assert_eq!(controls.captured(), None);
    assert_eq!(controls.button(false, (170.0, 20.0)).activated, None);
}

#[test]
fn capture_survives_leaving_but_not_removal_or_focus_loss() {
    let mut controls = Controls::default();
    controls.update([region("a", 0.0)]);
    controls.button(true, (20.0, 20.0));
    controls.pointer(PointerPhase::Leave, (120.0, 20.0));
    assert_eq!(controls.captured(), Some(&"a"));
    controls.update([region("a", 0.0)]);
    assert_eq!(controls.button(false, (120.0, 20.0)).activated, None);
    controls.button(true, (20.0, 20.0));
    controls.pointer(PointerPhase::Leave, (20.0, 20.0));
    assert_eq!(
        controls.button(false, (20.0, 20.0)).activated,
        None,
        "release with stale native coordinates after leave"
    );
    controls.button(true, (20.0, 20.0));
    controls.update([]);
    controls.update([region("a", 0.0)]);
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
    controls.button(true, (20.0, 20.0));
    controls.dispatch(&Event::WindowFocused(false));
    controls.dispatch(&Event::WindowFocused(true));
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
}

#[test]
fn rounded_regions_and_clips_use_the_same_coordinate_space() {
    let mut controls = Controls::default();
    let mut rounded = region("front", 0.0);
    rounded.input.shape = HitShape::Rounded(10.0);
    rounded.clip = Bounds::new(0.0, 0.0, 50.0, 40.0);
    controls.update([region("back", 0.0), rounded]);
    assert_eq!(controls.hit((0.0, 0.0)), Some(&"back"));
    assert_eq!(controls.hit((10.0, 10.0)), Some(&"front"));
    assert_eq!(controls.hit((50.0, 10.0)), Some(&"back"));
}

#[test]
fn secondary_buttons_do_not_activate_or_steal_primary_capture() {
    let mut controls = Controls::default();
    controls.update([region("a", 0.0)]);
    assert_eq!(
        controls
            .pointer(PointerPhase::Down(Button::Secondary), (20.0, 20.0))
            .activated,
        None
    );
    assert_eq!(controls.captured(), None);
    assert_eq!(
        controls
            .pointer(PointerPhase::Up(Button::Secondary), (20.0, 20.0))
            .activated,
        None
    );
    controls.button(true, (20.0, 20.0));
    controls.pointer(PointerPhase::Down(Button::Secondary), (20.0, 20.0));
    controls.pointer(PointerPhase::Up(Button::Secondary), (20.0, 20.0));
    assert_eq!(controls.captured(), Some(&"a"));
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, Some("a"));
}

#[test]
fn modal_regions_block_the_background_and_removal_cancels_the_press() {
    let mut controls = Controls::default();
    controls.update([region("background", 0.0)]);
    controls.button(true, (20.0, 20.0));
    controls.update([region("background", 0.0), region("modal", 0.0)]);
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
    assert_eq!(controls.button(true, (20.0, 20.0)).target, Some("modal"));
    controls.update([region("background", 0.0)]);
    assert_eq!(controls.button(false, (20.0, 20.0)).activated, None);
}

#[test]
fn fractional_adjacent_targets_have_no_dead_zone_at_any_scale() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let mut tree = ViewTree::new(
            HStack::new(
                0.0,
                (
                    Rectangle::new(Color::rgb(0))
                        .frame(10.4, 20.0)
                        .input(InputOptions::default()),
                    Rectangle::new(Color::rgb(1))
                        .frame(10.4, 20.0)
                        .input(InputOptions::default()),
                ),
            ),
            || {},
        );
        tree.update((100, 40), scale).unwrap();
        let targets = tree.input().targets();
        let edge = targets[1].bounds.x;
        for (x, expected) in [
            (edge - 0.1, targets[0].id),
            (edge, targets[1].id),
            (edge + 0.1, targets[1].id),
        ] {
            assert_eq!(
                tree.input().hit_test((x, 10.0)),
                Some(expected),
                "scale={scale} x={x}"
            );
            let hit = tree
                .scene()
                .hit_test(((x * scale) as f64, (10.0 * scale) as f64))
                .unwrap();
            assert_eq!(tree.geometry(expected).unwrap().key, hit);
        }
    }
}
