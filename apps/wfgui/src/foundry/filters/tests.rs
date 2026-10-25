use super::*;

fn view(scale: f32) -> Foundry {
    let mut view = Foundry::new(Default::default()).unwrap();
    view.resize((1000, 850), scale);
    view.update(model::fixture());
    view
}

#[test]
fn each_filter_supports_yes_no_and_any() {
    let prefs = crate::preferences::Preferences::default();
    let yes = Card {
        id: "test".into(),
        is_prime: true,
        mastered: true,
        owned: true,
        vaulted: true,
        ready_to_build: true,
        used_for_crafting: true,
        has_archon_shards: true,
        has_incarnon: true,
        mastery_requirement: 10,
        ..Default::default()
    };
    prefs.toggle_favorite("test");
    for filter in Filter::ALL {
        let mut filters = Filters::default();
        assert!(filters.matches(&yes, Some(20), &prefs));
        filters.toggle(filter, true);
        assert_eq!(filters.count(), 1);
        assert!(filters.matches(&yes, Some(20), &prefs), "{filter:?}");
        filters.toggle(filter, false);
        assert_eq!(filters.count(), 1);
        assert!(!filters.matches(&yes, Some(20), &prefs), "{filter:?}");
        let no = Card {
            is_prime: filter == Filter::Vaulted,
            mastery_requirement: 21,
            ..Default::default()
        };
        assert!(filters.matches(&no, Some(20), &prefs), "{filter:?}");
        filters.toggle(filter, false);
        assert_eq!(filters.count(), 0);
        assert!(filters.matches(&yes, None, &prefs));
    }
}

#[test]
fn filters_compose_with_search_and_category_without_changing_toolbar_or_fetching() {
    let mut view = view(1.25);
    let fixed_controls = |view: &Foundry| {
        view.controls()
            .into_iter()
            .filter(|(a, _)| !matches!(a, Action::Search | Action::ClearSearch))
            .collect::<Vec<_>>()
    };
    let toolbar = fixed_controls(&view);
    let viewport = view.viewport;
    let canonical = view.snapshot.items.clone();
    assert_eq!(view.count(), 120);
    view.activate(Action::FilterChoice(Filter::Owned, true));
    assert_eq!(view.count(), 40);
    view.activate(Action::FilterChoice(Filter::Mastered, true));
    assert_eq!(view.count(), 20);
    view.set_search("Ash Prime");
    assert!(view.count() > 0 && view.count() < 20);
    assert!(
        view.filtered
            .iter()
            .all(|&i| view.snapshot.items[i].owned && view.snapshot.items[i].mastered)
    );
    view.activate(Action::ClearFilters);
    assert!(
        view.filtered
            .iter()
            .all(|&i| view.snapshot.items[i].name.contains("Ash Prime"))
    );
    assert_eq!(view.refresh, 0);
    assert_eq!(fixed_controls(&view), toolbar);
    assert_eq!(view.viewport, viewport);
    assert_eq!(view.snapshot.items, canonical);
}

#[test]
fn unknown_rank_is_not_rank_zero_and_vaulted_does_not_match_normal_items() {
    let mut view = view(1.0);
    view.snapshot.profile = serde_json::Value::Null;
    assert!(!view.filter_enabled(Action::FilterChoice(Filter::Mastery, false)));
    view.activate(Action::FilterChoice(Filter::Mastery, false));
    assert_eq!(view.filters.count(), 0);
    let mut filters = Filters::default();
    for value in [true, false] {
        filters.toggle(Filter::Mastery, value);
        assert!(!filters.matches(&Card::default(), None, &view.preferences));
    }
    filters = Filters::default();
    filters.toggle(Filter::Vaulted, false);
    assert!(!filters.matches(&Card::default(), Some(10), &view.preferences));
    assert!(filters.matches(
        &Card {
            is_prime: true,
            ..Default::default()
        },
        Some(10),
        &view.preferences
    ));
}

#[test]
fn favorites_include_components_and_remain_filtered_after_refresh() {
    let mut view = view(1.0);
    let component = view.snapshot.items[0].components[0].id.clone();
    view.preferences.toggle_favorite(&component);
    view.activate(Action::Favorite(1));
    view.activate(Action::FilterChoice(Filter::Favorite, true));
    assert_eq!(view.count(), 2);
    view.activate(Action::Favorite(1));
    assert_eq!(view.count(), 1);
    let id = view.snapshot.items[0].id.clone();
    let mut snapshot = view.snapshot.clone();
    snapshot.items.reverse();
    view.update(snapshot);
    assert_eq!(view.count(), 1);
    assert_eq!(view.snapshot.items[view.filtered[0]].id, id);
}

#[test]
fn popup_is_bounded_keyboard_scrolls_and_escape_restores_trigger() {
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view(scale);
        view.open_filters();
        let bounds = view.filter_panel.popup.bounds;
        let viewport = Rect {
            x: 0,
            y: 0,
            width: view.size.0,
            height: view.size.1,
        };
        assert_eq!(bounds.intersection(viewport), bounds);
        let offset = view.offset;
        view.key(InputKey::Named(NamedKey::End), ModifiersState::empty());
        assert_eq!(
            view.focus,
            Some(Action::FilterChoice(Filter::Incarnon, false))
        );
        assert!(view.filter_panel.offset > 0);
        let (_, r) = view
            .filter_controls()
            .into_iter()
            .find(|(a, _)| Some(*a) == view.focus)
            .unwrap();
        assert_eq!(r.height, view.px(32.0));
        assert_eq!(view.offset, offset);
        view.key(InputKey::Named(NamedKey::Space), ModifiersState::empty());
        assert_eq!(view.filters.get(Filter::Incarnon), Some(false));
        view.key(
            InputKey::Named(NamedKey::ArrowLeft),
            ModifiersState::empty(),
        );
        assert_eq!(
            view.focus,
            Some(Action::FilterChoice(Filter::Incarnon, true))
        );
        view.key(InputKey::Named(NamedKey::Escape), ModifiersState::empty());
        assert_eq!(view.focus, Some(Action::Filters));
        assert!(!view.filters_open());
    }
}

#[test]
fn dismissing_click_never_activates_card_beneath() {
    let mut view = view(1.0);
    let card = view.card_rect(0);
    let star = view.title_rects(&view.snapshot.items[0]).0;
    let point = (
        (card.x + star.x + star.width / 2) as f64,
        (card.y + star.y + star.height / 2) as f64,
    );
    assert_eq!(view.hit(point), Some(Action::Favorite(0)));
    view.open_filters();
    assert!(!view.filter_panel.popup.bounds.contains(point.0, point.1));
    view.pointer_button(point, true);
    view.pointer_button(point, false);
    assert!(!view.preferences.is_favorite(&view.snapshot.items[0].id));
    assert_eq!(view.focus, Some(Action::Filters));
}

#[test]
fn accessibility_exposes_toggle_state_and_reveals_offscreen_options() {
    let mut view = view(2.0);
    view.open_filters();
    let first = NodeId(301);
    view.access(accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_node: first,
        target_tree: accesskit::TreeId::ROOT,
        data: None,
    });
    let tree = view.accessibility();
    let node = &tree.nodes.iter().find(|(id, _)| *id == first).unwrap().1;
    assert_eq!(node.toggled(), Some(accesskit::Toggled::True));
    assert_eq!(tree.focus, first);
    assert!(!tree.nodes.iter().any(|(id, _)| id.0 >= 1000));
    view.access(accesskit::ActionRequest {
        action: accesskit::Action::Focus,
        target_node: NodeId(320),
        target_tree: accesskit::TreeId::ROOT,
        data: None,
    });
    assert!(view.filter_panel.offset > 0);
}

#[test]
fn filter_scroll_and_live_card_update_damage_match_full_paint() {
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view(scale);
        view.open_filters();
        let full = Rect {
            x: 0,
            y: 0,
            width: view.size.0,
            height: view.size.1,
        };
        let paint = |view: &Foundry, pixels: &mut [u8], damage: Damage| {
            let bounds = damage.bounds(view.size).unwrap();
            let mut p = Painter::new(pixels, view.size.0, view.size.1).unwrap();
            p.clip_to_rect(bounds);
            p.fill_rounded_rect(bounds.x, bounds.y, bounds.width, bounds.height, 0, PANEL);
            view.paint(&mut p, Default::default());
            p.finish().unwrap();
        };
        let mut pixels = vec![0; (view.size.0 * view.size.1 * 4) as usize];
        paint(&view, &mut pixels, Damage::Full);
        for stage in 0..3 {
            let damage = match stage {
                0 => {
                    view.wheel(200.0, false);
                    view.tick(Instant::now())
                }
                1 => {
                    let mut snapshot = view.snapshot.clone();
                    snapshot.items[0].owned = !snapshot.items[0].owned;
                    view.update(snapshot)
                }
                _ => {
                    view.close_filters();
                    Damage::Full
                }
            };
            paint(&view, &mut pixels, damage);
            let mut expected = vec![0; pixels.len()];
            paint(&view, &mut expected, Damage::rect(full));
            assert!(pixels == expected, "scale={scale} stage={stage}");
        }
    }
}

#[test]
fn selected_filter_previews() {
    let Some(directory) = std::env::var_os("GUI_FILTER_CAPTURES") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let mut view = view(scale);
        view.open_filters();
        view.activate(Action::FilterChoice(Filter::Prime, true));
        view.activate(Action::FilterChoice(Filter::Mastered, false));
        for position in ["top", "bottom"] {
            if position == "bottom" {
                view.focus_filter(Action::FilterChoice(Filter::Incarnon, true));
            }
            let mut pixels = vec![0; (view.size.0 * view.size.1 * 4) as usize];
            let mut p = Painter::new(&mut pixels, view.size.0, view.size.1).unwrap();
            view.paint(&mut p, Default::default());
            p.finish().unwrap();
            let r = view.capture_bounds("foundry.filters").unwrap();
            retinue_ui::image::RgbImage::from_fn(r.width, r.height, |x, y| {
                let i = (((r.y + y) * view.size.0 + r.x + x) * 4) as usize;
                retinue_ui::image::Rgb([pixels[i + 2], pixels[i + 1], pixels[i]])
            })
            .save(directory.join(format!("filters-{position}-{}.png", (scale * 100.0) as u32)))
            .unwrap();
        }
    }
}
