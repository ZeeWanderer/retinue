use super::{super::MOTION, *};

fn layout() -> Layout {
    Layout {
        size: (1280, 800),
        scale: 1.25,
        left: 1.0,
        right: 1.0,
        right_fits: true,
        compact_left: false,
        right_available: true,
        tooltip: false,
        page: Page::Foundry,
    }
}
fn snap(c: &mut Composition, layout: Layout) {
    c.set_motion_enabled(false);
    c.update_at(layout, Instant::now());
    c.set_motion_enabled(true);
}

#[test]
fn title_actions_use_declared_regions_and_controls_block_dragging() {
    let mut c = Composition::default();
    for scale in [1.0, 1.25, 2.0] {
        snap(&mut c, Layout { scale, ..layout() });
        assert_eq!(
            c.window_action(
                (200.0 * scale as f64, 20.0 * scale as f64),
                WindowCapabilities::DESKTOP
            ),
            Some((TITLE, WindowAction::Move))
        );
        for (control, action) in [
            (Control::Left, None),
            (Control::Larger, None),
            (Control::Close, Some(WindowAction::Close)),
            (Control::Minimize, Some(WindowAction::Minimize)),
            (Control::Maximize, Some(WindowAction::ToggleMaximized)),
        ] {
            let rect = c.rect(control_key(control));
            assert!(!rect.is_empty(), "{control:?}");
            assert_eq!(
                c.window_action(
                    (
                        (rect.x + rect.width / 2) as f64,
                        (rect.y + rect.height / 2) as f64
                    ),
                    WindowCapabilities::DESKTOP
                ),
                action.map(|action| (control_key(control), action)),
                "{control:?}"
            );
        }
    }
}

#[test]
fn inactive_tooltip_is_not_instantiated() {
    let mut c = Composition::default();
    snap(&mut c, layout());
    assert!(c.tree.borrow().unique(TOOLTIP).unwrap().is_none());
    snap(
        &mut c,
        Layout {
            tooltip: true,
            ..layout()
        },
    );
    assert_eq!(
        c.rect(TOOLTIP),
        Rect {
            width: 1280,
            height: 800,
            ..Rect::default()
        }
    );
    snap(&mut c, layout());
    assert!(c.tree.borrow().unique(TOOLTIP).unwrap().is_none());
}

#[test]
fn rail_motion_preserves_controls_and_only_evaluates_affected_views() {
    let mut c = Composition::default();
    let mut now = Instant::now();
    c.update_at(layout(), now);
    let close = c.rect(control_key(Control::Close));
    for left in [0.75, 0.25, 0.6, 0.0, 1.0] {
        c.update_at(Layout { left, ..layout() }, now);
        now += MOTION / 2;
        c.update_at(Layout { left, ..layout() }, now);
        assert_eq!(c.rect(control_key(Control::Close)), close);
        let tree = c.tree.borrow();
        assert_eq!(tree.stats.mounted, 0);
        assert_eq!(tree.stats.unmounted, 0);
        assert!(tree.stats.bodies < CONTROLS.len());
    }
    now += MOTION;
    c.update_at(layout(), now);
    assert_eq!(c.deadline(now, Duration::from_millis(8)), None);
    assert_eq!(c.update_at(layout(), now), Damage::None);
    assert_eq!(c.tree.borrow().stats.bodies, 0);
}

#[test]
fn rail_arrangements_switch_without_remounting() {
    let mut c = Composition::default();
    snap(&mut c, layout());
    let close = c.rect(control_key(Control::Close));
    let docked = c.rect(CENTER);
    for fits in [false, true, false, true] {
        snap(
            &mut c,
            Layout {
                right_fits: fits,
                ..layout()
            },
        );
        assert_eq!(c.tree.borrow().stats.mounted, 0);
        assert_eq!(c.tree.borrow().stats.unmounted, 0);
        assert_eq!(c.rect(control_key(Control::Close)), close);
        assert_eq!(c.rect(CENTER).width > docked.width, !fits);
    }
}

#[test]
fn only_selected_page_is_instantiated() {
    let mut c = Composition::default();
    snap(&mut c, layout());
    let geometry = c.geometry();
    for page in PAGES {
        snap(&mut c, Layout { page, ..layout() });
        assert_eq!(c.geometry(), geometry);
        for candidate in PAGES {
            assert_eq!(
                c.tree
                    .borrow()
                    .unique(page_key(candidate))
                    .unwrap()
                    .is_some(),
                candidate == page
            );
        }
        assert_eq!(c.rect(page_key(page)), geometry.center);
    }
}
