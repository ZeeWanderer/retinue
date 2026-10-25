use super::tests::view;
use super::*;

#[test]
fn component_market_and_relic_navigation_use_canonical_reward_name() {
    let mut v = Foundry::new(Default::default()).unwrap();
    v.update(model::fixture());
    v.resize((1000, 800), 1.25);
    let c = &mut v.snapshot.items[0].components[0];
    c.market_name = Some("Ash Prime Neuroptics Blueprint".into());
    c.relic_drop = true;
    c.owned = 0;
    v.activate(Action::Component(0, 0));
    assert!(
        matches!(v.destination.take(), Some(Destination::Market {item,buyers:false}) if item=="Ash Prime Neuroptics Blueprint")
    );
    assert!(!v.component_open());
    v.snapshot.items[0].components[0].owned = 1;
    v.activate(Action::Component(0, 0));
    assert!(matches!(
        v.destination.take(),
        Some(Destination::Market { buyers: true, .. })
    ));
    v.open_component(0, 0);
    assert!(
        v.component_controls()
            .iter()
            .any(|(a, _)| *a == Action::ComponentRelics)
    );
    v.activate(Action::ComponentRelics);
    assert!(
        matches!(v.destination.take(), Some(Destination::Relics(item)) if item=="Ash Prime Neuroptics Blueprint")
    );
}

#[test]
fn component_search_uses_daemon_full_name_not_abbreviated_label() {
    let mut v = view();
    let component: model::Component = serde_json::from_value(serde_json::json!({
        "id": "ash-blueprint", "name": "Blueprint", "full_name": "Ash Prime Blueprint",
        "required": 1, "owned": 0
    }))
    .unwrap();
    v.snapshot.items[0].components[0] = component;
    v.open_component(0, 0);
    assert!(v.component_summary().contains("Ash Prime Blueprint"));
    v.activate(Action::ComponentSearch);
    assert_eq!(v.query(), "Ash Prime Blueprint");
    assert_eq!(v.count(), 1);
    assert_eq!(v.snapshot.items[v.filtered[0]].id, "fixture-0");
}

#[test]
fn search_expansion_grows_with_query_without_moving_grid_or_right_edge() {
    for scale in [1.0, 1.25, 2.0] {
        for width in [600, 1300, 2200] {
            let mut v = view();
            v.resize((width, 1100), scale);
            let grid = v.viewport;
            let closed = v.search_bounds();
            assert_eq!(closed.width, v.px(32.0));
            v.focus_action(Some(Action::Search));
            let now = Instant::now();
            v.tick(now + Duration::from_millis(100));
            let midway = v.search_bounds();
            v.tick(now + Duration::from_millis(250));
            let open = v.search_bounds();
            assert!(closed.width < midway.width && midway.width <= open.width);
            v.set_search("Ash Prime Neuroptics Blueprint");
            let query = v.search_bounds();
            let clear = v
                .controls()
                .into_iter()
                .find(|(a, _)| *a == Action::ClearSearch)
                .unwrap()
                .1;
            assert_eq!(
                v.hit((
                    (clear.x + clear.width / 2) as f64,
                    (clear.y + clear.height / 2) as f64
                )),
                Some(Action::ClearSearch)
            );
            assert!(query.width >= open.width);
            for r in [midway, open, query] {
                assert_eq!(r.y, closed.y);
                assert_eq!(r.x + r.width, closed.x + closed.width);
                assert!(r.x + r.width <= width);
            }
            for (action, r) in v.controls() {
                if matches!(action, Action::Category(_)) && r.y == query.y {
                    assert!(r.x + r.width <= query.x);
                }
            }
            assert_eq!(v.viewport, grid);
            v.focus_action(None);
            v.tick(Instant::now() + Duration::from_millis(300));
            assert_eq!(v.search_bounds().width, query.width, "nonempty stays open");
            v.activate(Action::ClearSearch);
            assert_eq!(v.focus, Some(Action::Search));
            assert_eq!(v.count(), 120);
            v.key(InputKey::Named(NamedKey::Escape), ModifiersState::empty());
            v.tick(Instant::now() + Duration::from_millis(300));
            assert_eq!(v.search_bounds(), closed);
            assert_eq!(v.viewport, grid);
        }
    }
}

#[test]
fn typing_grows_search_immediately_without_retiming_click_expansion() {
    for scale in [1.0, 1.25, 2.0] {
        let mut v = view();
        v.resize(((1800.0 * scale) as u32, 1100), scale);
        let grid = v.viewport;
        v.focus_action(Some(Action::Search));
        assert!(v.search_motion.deadline(Instant::now()).is_some());
        v.key(InputKey::Character("W"), ModifiersState::empty());
        assert!(v.search_motion.deadline(Instant::now()).is_none());
        assert_eq!(v.search_bounds().width, v.px(208.0));
        let right = v.search_bounds().x + v.search_bounds().width;
        for _ in 0..36 {
            v.key(InputKey::Character("W"), ModifiersState::empty());
            let required = text_width(&v.regular, &v.query(), 14.0) + 76.0;
            if required > 208.0 {
                assert!(v.search_motion.deadline(Instant::now()).is_none());
                assert_eq!(v.search_bounds().width, v.px(required));
                assert_eq!(v.search_bounds().x + v.search_bounds().width, right);
            }
            assert_eq!(v.viewport, grid);
        }
        v.key(
            InputKey::Named(NamedKey::Backspace),
            ModifiersState::empty(),
        );
        assert!(v.search_motion.deadline(Instant::now()).is_some());
        v.activate(Action::ClearSearch);
        v.key(InputKey::Named(NamedKey::Escape), ModifiersState::empty());
        assert!(v.search_motion.deadline(Instant::now()).is_some());
    }
}

#[test]
fn clipboard_preserves_selection_on_failure_and_rejects_late_reply() {
    let mut v = view();
    v.set_search("Ash");
    v.key(InputKey::Character("f"), ModifiersState::CONTROL);
    assert_eq!(v.search.borrow().selected_text().as_deref(), Some("Ash"));
    v.key(InputKey::Character("x"), ModifiersState::CONTROL);
    let (token, _) = v.clipboard_request.take().unwrap();
    v.clipboard_reply(token, Err("unavailable".into()));
    assert_eq!(v.search.borrow().text(), "Ash");
    assert!(v.input_error.is_some());
    v.key(InputKey::Character("v"), ModifiersState::CONTROL);
    let (token, _) = v.clipboard_request.take().unwrap();
    v.clipboard_reply(token, Ok(Some("Ember Prime".into())));
    assert_eq!(v.search.borrow().text(), "Ember Prime");
    v.key(InputKey::Character("v"), ModifiersState::CONTROL);
    let (token, _) = v.clipboard_request.take().unwrap();
    v.key(InputKey::Named(NamedKey::Home), ModifiersState::empty());
    v.clipboard_reply(token, Ok(Some("wrong location".into())));
    assert_eq!(v.search.borrow().text(), "Ember Prime");
    v.key(InputKey::Character("f"), ModifiersState::CONTROL);
    v.key(InputKey::Character("v"), ModifiersState::CONTROL);
    let (token, _) = v.clipboard_request.take().unwrap();
    v.clipboard_reply(token, Ok(Some("one\ntwo".into())));
    assert_eq!(v.search.borrow().text(), "Ember Prime");
    v.key(InputKey::Character("x"), ModifiersState::CONTROL);
    let (token, _) = v.clipboard_request.take().unwrap();
    v.clipboard_reply(token, Ok(None));
    assert_eq!(v.search.borrow().text(), "");
}

#[test]
fn component_uses_stable_identity_and_favorites_refilter_all_references() {
    let mut v = view();
    let id = v.snapshot.items[0].components[0].id.clone();
    let action = Action::Component(0, 0);
    let (_, rect) = v
        .item_controls()
        .into_iter()
        .find(|(a, _)| *a == action)
        .unwrap();
    let p = (
        (rect.x + rect.width / 2) as f64,
        (rect.y + rect.height / 2) as f64,
    );
    assert_eq!(v.hit(p), Some(action));
    v.pointer_button(p, true);
    v.pointer_button(p, false);
    assert!(v.component_open());
    let mut snapshot = v.snapshot.clone();
    snapshot.items.reverse();
    assert_eq!(v.update(snapshot), Damage::Full);
    v.activate(Action::ComponentFavorite);
    assert!(v.preferences.is_favorite(&id));
    v.activate(Action::ComponentSearch);
    assert!(!v.component_open());
    assert_eq!(v.category, 0);
    assert_eq!(v.filters.count(), 0);
    assert!(!v.search.borrow().text().is_empty());
    assert!(
        v.filtered
            .iter()
            .any(|&r| v.snapshot.items[r].components.iter().any(|p| p.id == id))
    );
}

#[test]
fn duplicate_recipe_slots_keep_their_own_availability() {
    let mut v = view();
    let first = v.snapshot.items[0].components[0].clone();
    v.snapshot.items[0].components[0].owned = 1;
    v.snapshot.items[0].components[1] = model::Component { owned: 0, ..first };
    v.open_component(0, 1);
    assert!(v.component_summary().contains("0 owned"));
    let mut snapshot = v.snapshot.clone();
    snapshot.items.swap(0, 1);
    v.update(snapshot);
    assert!(v.component_summary().contains("0 owned"));
    v.close_component();
    assert_eq!(v.focus, Some(Action::Component(1, 1)));
    let mut snapshot = v.snapshot.clone();
    snapshot.items.swap(0, 1);
    v.update(snapshot);
    assert_eq!(v.focus, Some(Action::Component(0, 1)));
}

#[test]
fn component_dismissal_keyboard_and_refresh_removal_are_safe() {
    let mut v = view();
    v.open_component(0, 0);
    let refresh = v.refresh;
    let r = v
        .controls()
        .into_iter()
        .find(|(a, _)| *a == Action::Refresh)
        .unwrap()
        .1;
    let p = ((r.x + 4) as f64, (r.y + 4) as f64);
    v.pointer_button(p, true);
    v.pointer_button(p, false);
    assert!(!v.component_open());
    assert_eq!(
        v.refresh, refresh,
        "outside press must not activate refresh"
    );
    v.open_component(0, 0);
    v.key(InputKey::Named(NamedKey::Tab), ModifiersState::empty());
    assert_eq!(v.focus, Some(Action::ComponentSearch));
    v.key(InputKey::Named(NamedKey::Escape), ModifiersState::empty());
    assert_eq!(v.focus, Some(Action::Component(0, 0)));
    v.activate(Action::Component(0, 0));
    let mut snapshot = v.snapshot.clone();
    snapshot.items.remove(0);
    v.update(snapshot);
    assert!(!v.component_open());
    assert_eq!(v.focus, None);
}

#[test]
fn component_focus_tracks_updates_and_removing_last_favorite_closes_popup() {
    let mut v = view();
    v.focus_action(Some(Action::Component(0, 1)));
    let mut snapshot = v.snapshot.clone();
    snapshot.items.swap(0, 1);
    v.update(snapshot);
    assert_eq!(v.focus, Some(Action::Component(1, 1)));
    let id = v.snapshot.items[1].components[1].id.clone();
    v.preferences.toggle_favorite(&id);
    v.activate(Action::FilterChoice(Filter::Favorite, true));
    assert_eq!(v.count(), 1);
    v.open_component(1, 1);
    v.activate(Action::ComponentFavorite);
    assert_eq!(v.count(), 0);
    assert!(!v.popup_open());
    let tree = v.accessibility();
    assert!(tree.nodes.iter().any(|(id, _)| *id == tree.focus));
}

#[test]
fn interaction_previews() {
    let Some(directory) = std::env::var_os("GUI_INTERACTION_CAPTURES") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let mut v = view();
        v.resize(((1300.0 * scale) as u32, (700.0 * scale) as u32), scale);
        let mut cache = images::cache(None);
        let images = v
            .image_requests()
            .into_iter()
            .map(|r| (r.key, cache.prepare(&r).unwrap()))
            .collect();
        v.accept_images(images, vec![]);
        for state in ["idle", "open", "query", "component"] {
            match state {
                "open" => {
                    v.focus_action(Some(Action::Search));
                    v.tick(Instant::now() + Duration::from_millis(300));
                }
                "query" => {
                    v.set_search("Ash Prime Neuroptics Blueprint");
                }
                "component" => {
                    v.set_search("");
                    v.snapshot.items[0].components[0].blueprint_owned = 2;
                    v.snapshot.items[0].components[0].owned_relic = true;
                    v.open_component(0, 0);
                }
                _ => (),
            }
            let r = v
                .capture_bounds(if state == "component" {
                    "foundry.component"
                } else {
                    "foundry.toolbar"
                })
                .unwrap();
            let mut pixels = vec![0; (v.size.0 * v.size.1 * 4) as usize];
            let mut p = Painter::new(&mut pixels, v.size.0, v.size.1).unwrap();
            v.paint(&mut p, Default::default());
            p.finish().unwrap();
            retinue_ui::image::RgbImage::from_fn(r.width, r.height, |x, y| {
                let i = (((r.y + y) * v.size.0 + r.x + x) * 4) as usize;
                retinue_ui::image::Rgb([pixels[i + 2], pixels[i + 1], pixels[i]])
            })
            .save(directory.join(format!("{state}-{}.png", (scale * 100.0) as u32)))
            .unwrap();
        }
    }
}
