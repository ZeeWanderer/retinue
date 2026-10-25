use super::*;

#[test]
fn search_capsules_clear_without_changing_other_controls_or_losing_focus() {
    for scale in [1.0, 1.25, 2.0] {
        let mut ui = Ui::new().unwrap();
        ui.begin((1200, 800), scale);
        let available = ui.rect(10.0, 10.0, 400.0, 32.0);
        let collapsed = ui.search_field(
            "query",
            available,
            "",
            "Search items",
            crate::search::Anchor::Leading,
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
            crate::search::Anchor::Leading,
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
        let mut ui = Ui::new().unwrap();
        let size = (1600, 900);
        ui.begin(size, scale);
        let available = ui.rect(25.0, 10.0, 400.0, 32.0);
        let collapsed = ui.search_field(
            "query",
            available,
            "",
            "Search",
            crate::search::Anchor::Trailing,
        );
        ui.finish();
        let text = "Ash Prime Neuroptics Blueprint";
        ui.begin(size, scale);
        let expanded = ui.search_field(
            "query",
            available,
            text,
            "Search",
            crate::search::Anchor::Trailing,
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
    let mut ui = Ui::new().unwrap();
    ui.begin((800, 600), 1.25);
    ui.button(
        "item",
        ui.rect(20.0, 20.0, 120.0, 40.0),
        "Item",
        true,
        false,
    );
    ui.open_context("item".into());
    ui.context_menu(
        Rect {
            x: 795,
            y: 590,
            ..Rect::default()
        },
        "Part name",
        &[],
        &[
            ("first".into(), "Show in Foundry".into(), true),
            ("disabled".into(), "View market listings".into(), false),
            ("last".into(), "Show in Relic Planner".into(), true),
        ],
        "close",
    );
    ui.finish();
    let bounds = ui.bounds("page.menu").unwrap();
    assert!(bounds.x + bounds.width <= ui.size.0);
    assert!(bounds.y + bounds.height <= ui.size.1);
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
    let mut ui = Ui::new().unwrap();
    ui.begin((800, 600), 1.0);
    ui.button(
        "item",
        ui.rect(10.0, 10.0, 100.0, 32.0),
        "Item",
        true,
        false,
    );
    ui.open_context("item".into());
    ui.context_menu(
        ui.rect(10.0, 42.0, 0.0, 0.0),
        "Unavailable item",
        &[],
        &[("market".into(), "View market listings".into(), false)],
        "close",
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
