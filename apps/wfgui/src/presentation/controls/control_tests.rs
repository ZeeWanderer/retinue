use super::*;

fn reflow_buttons(ui: &mut ControlHost, swap: bool) {
    ui.begin((300, 100), 1.0);
    for (name, x) in [
        ("a", if swap { 150 } else { 0 }),
        ("b", if swap { 0 } else { 150 }),
    ] {
        ui.button(
            name,
            Rect {
                x,
                y: 0,
                width: 100,
                height: 40,
            },
            name,
            false,
            true,
        );
    }
    ui.finish();
}

#[test]
fn stationary_pointer_reflow_selects_the_new_control() {
    let mut ui = ControlHost::new().unwrap();
    reflow_buttons(&mut ui, false);
    ui.pointer_move((20.0, 20.0));
    reflow_buttons(&mut ui, true);
    assert_eq!(ui.hover.as_deref(), Some("b"));
    ui.pointer_button(true);
    ui.pointer_button(false);
    assert_eq!(ui.signals.last().unwrap().key, "b");
}

#[test]
fn moved_control_does_not_activate_but_unchanged_rebuild_retains_capture() {
    let mut ui = ControlHost::new().unwrap();
    reflow_buttons(&mut ui, false);
    ui.pointer_move((20.0, 20.0));
    ui.pointer_button(true);
    reflow_buttons(&mut ui, false);
    assert_eq!(ui.pressed.as_deref(), Some("a"));
    reflow_buttons(&mut ui, true);
    ui.pointer_button(false);
    assert!(ui.signals.is_empty());
}

#[test]
fn multiple_regions_can_share_an_action_without_duplicate_focus_stops() {
    let mut ui = ControlHost::new().unwrap();
    ui.begin((300, 100), 1.0);
    for x in [0, 150] {
        ui.hit(
            "item",
            Rect {
                x,
                y: 0,
                width: 100,
                height: 40,
            },
            "Item",
            true,
        );
    }
    ui.finish();
    for x in [20.0, 170.0] {
        ui.pointer_move((x, 20.0));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert_eq!(ui.signals.last().unwrap().key, "item");
    }
    ui.focus_next(false);
    assert_eq!(ui.focus.as_deref(), Some("item"));
}

#[test]
fn nested_viewports_intersect_clips_and_restore_parent_coordinates() {
    for scale in [1.0, 1.25, 2.0] {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((1200, 1000), scale);
        ui.begin_scroll(ui.rect(20.0, 30.0, 200.0, 100.0), ui.px(10.0));
        let parent = ui.viewport;
        ui.begin_scroll(ui.rect(5.0, 0.0, 50.0, 40.0), ui.px(3.0));
        ui.hit("nested", ui.rect(0.0, 0.0, 50.0, 50.0), "", true);
        let hit = ui.bounds("nested").unwrap();
        let (clip, _) = parent.unwrap();
        assert_eq!(hit.x, ui.px(20.0) + ui.px(5.0));
        assert_eq!(hit.y, clip.y);
        assert!(hit.y + hit.height <= clip.y + clip.height);
        ui.end_scroll();
        assert_eq!(ui.viewport, parent);
        ui.hit("sibling", ui.rect(0.0, 10.0, 10.0, 10.0), "", true);
        assert_eq!(ui.bounds("sibling").unwrap().y, ui.px(30.0));
        ui.begin_scroll(ui.rect(500.0, 500.0, 20.0, 20.0), 0);
        ui.hit("outside", ui.rect(0.0, 0.0, 10.0, 10.0), "", true);
        assert!(ui.bounds("outside").is_none());
        ui.end_scroll();
        ui.end_scroll();
        assert!(ui.viewport.is_none());
        ui.finish();
    }
}

#[test]
fn search_focus_matches_the_control_shape() {
    use crate::{presentation::widgets::search::Anchor, presentation::widgets::search::Style};
    use retinue_render::Command;
    use retinue_ui::drawing::PixelCanvas as Canvas;
    for scale in [1.0, 1.25, 2.0] {
        for style in [Style::Fill(Anchor::Trailing), Style::Field] {
            let mut ui = ControlHost::new().unwrap();
            ui.begin((1600, 900), scale);
            ui.styled_search_field(
                "query",
                ui.rect(10.0, 10.0, 400.0, 32.0),
                "sa",
                "Search",
                style,
            );
            ui.finish();
            ui.focus = Some("query".into());
            ui.focus_visible.keyboard_input();
            let mut canvas = Canvas::new(1.0);
            ui.paint_overlays(&mut canvas);
            let radius = canvas
                .commands
                .iter()
                .find_map(|command| match command {
                    Command::Outline { radius, .. } => Some(*radius),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                radius,
                ui.px(if matches!(style, Style::Field) {
                    6.0
                } else {
                    16.0
                }) as f32
            );
        }
    }
}

#[test]
fn scroll_padding_uses_the_same_translation_for_content_and_hits() {
    use retinue_ui::{scroll::Padding, scroll::Viewport};
    for scale in [1.0, 1.25, 2.0] {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((800, 600), scale);
        let viewport = Viewport {
            bounds: ui.rect(10.0, 20.0, 200.0, 150.0),
            padding: Padding {
                top: ui.px(12.0),
                right: ui.px(8.0),
                bottom: ui.px(16.0),
                left: ui.px(6.0),
            },
        };
        ui.begin_scroll_viewport(viewport, ui.px(5.0));
        ui.button("padded", ui.rect(0.0, 0.0, 30.0, 30.0), "Test", false, true);
        ui.end_scroll();
        ui.finish();
        let button = ui.bounds("padded").unwrap();
        let (x, y) = viewport.translation(ui.px(5.0));
        assert_eq!((button.x, button.y), (x as u32, y as u32));
        ui.pointer_move((
            (button.x + button.width / 2) as f64,
            (button.y + button.height / 2) as f64,
        ));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert_eq!(ui.signals.pop().unwrap().key, "padded");
    }
}

#[test]
fn search_ticks_preserve_fill_and_field_sizing() {
    use crate::{presentation::widgets::search::Anchor, presentation::widgets::search::Style};
    for scale in [1.0, 1.25, 2.0] {
        for style in [Style::Fill(Anchor::Trailing), Style::Field] {
            let mut ui = ControlHost::new().unwrap();
            ui.begin((1600, 900), scale);
            let available = ui.rect(10.0, 10.0, 400.0, 32.0);
            ui.styled_search_field("query", available, "sa", "Search", style);
            ui.finish();
            let now = Instant::now();
            for millis in [250, 500, 750] {
                let tick = now + Duration::from_millis(millis);
                ui.tick(tick);
                let search = &ui.searches["query"];
                assert_eq!(search.motion.width, 400.0);
                assert!(search.motion.deadline(tick).is_none());
            }
        }
    }
}

#[test]
fn search_capsules_clear_without_changing_other_controls_or_losing_focus() {
    for scale in [1.0, 1.25, 2.0] {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((1200, 800), scale);
        let available = ui.rect(10.0, 10.0, 400.0, 32.0);
        let collapsed = ui.search_field(
            "query",
            available,
            "",
            "Search items",
            crate::presentation::widgets::search::Anchor::Leading,
        );
        ui.finish();
        assert_eq!(collapsed.width, ui.px(32.0));
        ui.key(Key::Character("f"), ModifiersState::CONTROL);
        assert_eq!(ui.focus.as_deref(), Some("query"));
        let text = "Ash Prime Neuroptics Blueprint";
        ui.key(Key::Character(text), ModifiersState::empty());
        assert_eq!(ui.signals.pop().unwrap().value.as_deref(), Some(text));
        ui.begin((1200, 800), scale);
        let expanded = ui.search_field(
            "query",
            available,
            text,
            "Search items",
            crate::presentation::widgets::search::Anchor::Leading,
        );
        ui.finish();
        assert!(expanded.width > ui.px(208.0));
        assert!(expanded.width <= available.width);
        assert_eq!(expanded.y, collapsed.y);
        let clear = ui
            .buttons
            .iter()
            .find(|b| b.key == "clear:query")
            .unwrap()
            .rect;
        assert!(
            ui.field_rect(expanded, "query").x + ui.field_rect(expanded, "query").width <= clear.x
        );
        ui.pointer_move((
            (clear.x + clear.width / 2) as f64,
            (clear.y + clear.height / 2) as f64,
        ));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert_eq!(ui.fields["query"].borrow().text(), "");
        assert_eq!(ui.focus.as_deref(), Some("query"));
        assert_eq!(ui.signals.pop().unwrap().value.as_deref(), Some(""));
    }
}

#[test]
fn trailing_search_keeps_its_edge_and_clear_hit_target_when_expanding() {
    for scale in [1.0, 1.25, 2.0] {
        let mut ui = ControlHost::new().unwrap();
        let size = (1600, 900);
        ui.begin(size, scale);
        let available = ui.rect(25.0, 10.0, 400.0, 32.0);
        let collapsed = ui.search_field(
            "query",
            available,
            "",
            "Search",
            crate::presentation::widgets::search::Anchor::Trailing,
        );
        ui.finish();
        let text = "Ash Prime Neuroptics Blueprint";
        ui.begin(size, scale);
        let expanded = ui.search_field(
            "query",
            available,
            text,
            "Search",
            crate::presentation::widgets::search::Anchor::Trailing,
        );
        ui.finish();
        assert_eq!(collapsed.x + collapsed.width, available.x + available.width);
        assert_eq!(expanded.x + expanded.width, collapsed.x + collapsed.width);
        assert!(expanded.x < collapsed.x);
        assert!(expanded.x >= available.x);
        let clear = ui.bounds("clear:query").unwrap();
        assert_eq!(clear.x + clear.width, expanded.x + expanded.width);
        ui.pointer_move((
            (clear.x + clear.width / 2) as f64,
            (clear.y + clear.height / 2) as f64,
        ));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert_eq!(ui.fields["query"].borrow().text(), "");
        assert_eq!(ui.focus.as_deref(), Some("query"));
    }
}

#[test]
fn context_menu_navigation_skips_disabled_entries_and_restores_invoker() {
    let mut ui = ControlHost::new().unwrap();
    ui.begin((800, 600), 1.25);
    ui.button(
        "item",
        ui.rect(20.0, 20.0, 120.0, 40.0),
        "Item",
        true,
        false,
    );
    ui.open_context("item".into());
    ui.begin_context(
        &[
            ("first".into(), "Show in Foundry".into(), true),
            ("disabled".into(), "View market listings".into(), false),
            ("last".into(), "Show in Relic Planner".into(), true),
        ],
        "close",
    );
    ui.region("page.menu", ui.rect(20.0, 60.0, 200.0, 100.0));
    for (i, (id, enabled)) in [("first", true), ("disabled", false), ("last", true)]
        .into_iter()
        .enumerate()
    {
        ui.hit(
            id,
            ui.rect(20.0, 60.0 + i as f32 * 30.0, 200.0, 30.0),
            id,
            enabled,
        );
    }
    ui.finish();
    assert_eq!(ui.focus.as_deref(), Some("first"));
    ui.key(Key::Named(NamedKey::ArrowDown), ModifiersState::empty());
    assert_eq!(ui.focus.as_deref(), Some("last"));
    ui.key(Key::Named(NamedKey::Home), ModifiersState::empty());
    assert_eq!(ui.focus.as_deref(), Some("first"));
    ui.key(Key::Named(NamedKey::Escape), ModifiersState::empty());
    assert_eq!(ui.signals.pop().unwrap().key, "close");
    assert_eq!(ui.focus.as_deref(), Some("item"));
    assert!(ui.context_keys.is_empty());
}

#[test]
fn menu_with_only_disabled_actions_still_dismisses_and_has_menu_semantics() {
    let mut ui = ControlHost::new().unwrap();
    ui.begin((800, 600), 1.0);
    ui.button(
        "item",
        ui.rect(10.0, 10.0, 100.0, 32.0),
        "Item",
        true,
        false,
    );
    ui.open_context("item".into());
    ui.begin_context(
        &[("market".into(), "View market listings".into(), false)],
        "close",
    );
    ui.region("page.menu", ui.rect(10.0, 42.0, 200.0, 40.0));
    ui.hit(
        "market",
        ui.rect(10.0, 42.0, 200.0, 40.0),
        "View market listings",
        false,
    );
    ui.finish();
    let tree = ui.accessibility(10);
    assert_eq!(
        tree.nodes.last().unwrap().1.role(),
        retinue_ui::desktop::accesskit::Role::Menu
    );
    ui.key(Key::Named(NamedKey::ArrowDown), ModifiersState::empty());
    ui.key(Key::Named(NamedKey::Enter), ModifiersState::empty());
    assert!(ui.signals.is_empty());
    ui.key(Key::Named(NamedKey::Escape), ModifiersState::empty());
    assert_eq!(ui.signals.pop().unwrap().key, "close");
    assert_eq!(ui.focus.as_deref(), Some("item"));
}

#[test]
fn layout_report_includes_declared_text_without_duplicating_bound_controls() {
    use crate::{
        presentation::widgets::fitted_label, presentation::widgets::place,
        presentation::widgets::slot,
    };
    use retinue_render::{Bounds, Key};
    use retinue_ui::prelude::*;
    let mut ui = ControlHost::new().unwrap();
    let control = Key::new("report.control", 0);
    ui.begin((500, 300), 1.25);
    ui.install_view(retinue_ui::ui! {
        ZStack() {
            place(fitted_label("Declared", ui.font_resource(Weight::Regular), 16.0, TEXT, false, 1.25), Bounds::new(20.0, 30.0, 200.0, 30.0))
            place(slot(control), Bounds::new(20.0, 70.0, 200.0, 30.0))
        }.alignment(Alignment::Start)
    });
    ui.update_view(FrameTime::now());
    ui.view_control(control, |ui, r| ui.text(r, "Bound", 16.0, TEXT));
    ui.finish();
    for painted in [false, true] {
        if painted {
            let mut pixels = vec![0; 500 * 300 * 4];
            let mut p = Painter::new(&mut pixels, 500, 300).unwrap();
            ui.paint(&mut p, &Media::default());
            p.finish().unwrap();
        }
        let report = ui.layout_report();
        let elements = report["elements"].as_array().unwrap();
        for text in ["Declared", "Bound"] {
            assert_eq!(elements.iter().filter(|e| e["text"] == text).count(), 1);
        }
        let text = elements.iter().find(|e| e["text"] == "Declared").unwrap();
        assert_eq!(text["bounds"]["x"], 20.0);
        assert!((text["bounds"]["y"].as_f64().unwrap() - 30.4).abs() < 0.01);
    }
}
