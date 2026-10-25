use super::*;

fn pages() -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    pages.configure((1000, 900), (400, 900), 1.0);
    pages
}

#[test]
fn page_layout_contracts_hold_at_fractional_scales() {
    let mut pages = pages();
    for scale in [1.0, 1.2, 1.25, 2.0] {
        pages.configure(
            ((1200.0 * scale) as u32, (900.0 * scale) as u32),
            ((400.0 * scale) as u32, (900.0 * scale) as u32),
            scale,
        );
        pages.set_page(Page::Mastery);
        pages.layout();
        let report = pages.ui.layout_report();
        let regions = &report["regions"];
        let a = &regions["mastery.summary.game"];
        let b = &regions["mastery.summary.star"];
        let c = &regions["mastery.summary.intrinsics"];
        let left = a["x"].as_f64().unwrap() - 12.0;
        let gap =
            b["x"].as_f64().unwrap() - a["x"].as_f64().unwrap() - a["width"].as_f64().unwrap();
        assert!((gap - 2.0 * left).abs() < 2.0, "space-around {report}");
        assert_eq!(a["y"], b["y"]);
        assert_eq!(b["y"], c["y"]);
        assert!((regions["mastery.card.image"]["width"].as_f64().unwrap() - 72.0).abs() < 1.0);
        let elements = report["elements"].as_array().unwrap();
        for expected in ["360/700", "220/230", "40/50"] {
            assert!(
                elements.iter().any(|e| e["text"] == expected),
                "Untruncated summary {expected}"
            );
        }
        assert!(elements.iter().any(|e| e["color"] == "#313a58ff"));
        pages.set_page(Page::Relics);
        pages.layout();
        let report = pages.ui.layout_report();
        let reward = &report["regions"]["relic.card.reward"];
        assert!((reward["width"].as_f64().unwrap() - 37.0).abs() < 1.0);
        assert!((report["regions"]["page.card"]["height"].as_f64().unwrap() - 100.0).abs() < 1.0);
        let elements = report["elements"].as_array().unwrap();
        assert!(
            elements
                .iter()
                .any(|e| e["kind"] == "component_background" && e["color"] == "#6e9b6878")
        );
        assert!(
            elements
                .iter()
                .any(|e| e["kind"] == "component_glow" && e["color"] == "#46d234ff")
        );
        pages.set_page(Page::Builds);
        pages.layout();
        let group = pages.ui.bounds("build:tab:groups").unwrap();
        let equipment = pages.ui.bounds("build:tab:equipment").unwrap();
        let discover = pages.ui.bounds("build:tab:discover").unwrap();
        assert!(group.x < equipment.x && equipment.x < discover.x);
    }
}

#[test]
fn page_embedded_artwork_decodes_in_the_thumbnail_pipeline() {
    let mut pages = pages();
    for page in [
        Page::Inventory,
        Page::Mastery,
        Page::Relics,
        Page::Market,
        Page::Builds,
    ] {
        pages.set_page(page);
        pages.layout();
        for ui in [&pages.ui, &pages.rail] {
            for request in ui.requests(&Default::default()) {
                if let retinue_ui::thumbnails::Source::Embedded(bytes) = request.source {
                    assert!(
                        retinue_ui::image::load_from_memory(bytes).is_ok(),
                        "{page:?}: embedded image {:?}",
                        request.key
                    );
                }
            }
        }
    }
}
fn signal(key: &str, value: Option<&str>) -> Signal {
    Signal {
        key: key.into(),
        value: value.map(str::to_owned),
    }
}

#[test]
fn window_capture_waits_for_the_visible_rail_to_settle() {
    let mut pages = pages();
    assert!(pages.rail_capture_ready());
    pages.fixture = false;
    pages.connected = false;
    assert!(!pages.rail_capture_ready());
    pages.update(Update::Connected);
    assert!(!pages.rail_capture_ready());
    for key in ["activity", "notifications"] {
        let request = pages
            .requests
            .iter()
            .find(|r| r.key == key)
            .unwrap()
            .clone();
        pages.update(Update::Reply {
            request,
            result: Ok(json!({})),
        });
    }
    assert!(pages.rail_capture_ready());
    pages.queue(
        "market:quote",
        json!({"op":"market_quote","items":["Ember Prime Set"],"ttl":60}),
    );
    assert!(
        pages.rail_capture_ready(),
        "hidden market does not delay events capture"
    );
    pages.rail_tab = true;
    assert!(!pages.rail_capture_ready());
    pages.update(Update::Disconnected("offline".into()));
    assert!(pages.rail_capture_ready(), "an error state can be captured");
}

#[test]
fn all_pages_have_bounded_controls_at_fractional_scales() {
    let mut pages = pages();
    for scale in [1.0, 1.25, 2.0] {
        pages.configure(
            ((1000.0 * scale) as u32, (900.0 * scale) as u32),
            ((400.0 * scale) as u32, (900.0 * scale) as u32),
            scale,
        );
        for page in [
            Page::Inventory,
            Page::Mastery,
            Page::Relics,
            Page::Market,
            Page::Builds,
        ] {
            pages.set_page(page);
            pages.layout();
            for ui in [&pages.ui, &pages.rail] {
                for button in &ui.buttons {
                    assert!(
                        button.rect.x + button.rect.width <= ui.size.0
                            && button.rect.y + button.rect.height <= ui.size.1,
                        "{page:?} {scale}: {} {:?} outside {:?}",
                        button.key,
                        button.rect,
                        ui.size
                    );
                }
            }
        }
    }
}

#[test]
fn market_actions_use_ids_and_preserve_order_variants() {
    let mut pages = pages();
    pages.fixture = false;
    pages.market_signal(signal("market:order:example:visibility", None));
    let request = pages.requests.pop_front().unwrap();
    assert_eq!(
        request.body,
        json!({"op":"market_order_update","order_id":"example","patch":{"visible":false}})
    );
    pages.market_signal(signal("market:order:example:add", None));
    assert!(
        pages.requests.is_empty(),
        "a write in flight blocks duplicate mutation"
    );
    pages.pending.clear();
    pages.market_signal(signal("market:order:example:add", None));
    assert_eq!(
        pages.requests.pop_front().unwrap().body["patch"]["quantity"],
        4
    );
    pages.pending.clear();
    pages.market_signal(signal("market:order:example:edit", None));
    pages.market_signal(signal("market:edit:quantity", Some("5")));
    pages.market_signal(signal("market:confirm", None));
    assert_eq!(
        pages.requests.pop_front().unwrap().body,
        json!({"op":"market_order_update","order_id":"example","patch":{"platinum":15,"quantity":5,"visible":true}})
    );
}

#[test]
fn failed_quotes_do_not_loop_and_loading_inventory_is_not_missing_stock() {
    let mut pages = pages();
    pages.fixture = false;
    pages.set_page(Page::Market);
    pages.layout();
    let request = pages
        .requests
        .iter()
        .find(|r| r.key == "market:comparison:example")
        .unwrap()
        .clone();
    pages.requests.clear();
    pages.update(Update::Reply {
        request,
        result: Err("Market unavailable".into()),
    });
    pages.layout();
    assert!(
        !pages
            .requests
            .iter()
            .any(|r| r.key == "market:comparison:example")
    );
    pages.inventory.ready = false;
    pages.market_signal(signal("market:bulk", Some("reconcile")));
    assert!(pages.market.dialog.is_none());
    assert!(
        pages
            .requests
            .iter()
            .all(|r| !r.key.starts_with("market:write"))
    );
}

#[test]
fn adding_a_source_keeps_the_current_tab_and_uses_revision_guard() {
    let mut pages = pages();
    pages.fixture = false;
    pages.set_page(Page::Builds);
    pages.build_signal(signal("build:tab:discover", None));
    pages.build_signal(signal("build:add-source", None));
    pages.build_signal(signal("build:confirm", None));
    let request = pages.requests.pop_front().unwrap();
    assert_eq!(
        request.body,
        json!({"op":"build_group_add_source","group_id":"fixture-group","revision":3,"source":"overframe","external_id":100,"fingerprint":"fixture-source"})
    );
    assert!(pages.ui.bounds("build:group").is_none());
    pages.capture_reply(
        request,
        Ok(json!({"id":"fixture-group","revision":4,"members":[]})),
    );
    pages.layout();
    assert!(pages.ui.bounds("build:open-added").is_some());
    assert!(pages.ui.bounds("build:add-source").is_some());
}

#[test]
fn stale_replies_cannot_replace_new_search_results() {
    let mut pages = pages();
    pages.fixture = false;
    pages.queue(
        "market:resolve",
        json!({"op":"market_resolve","labels":["old"]}),
    );
    let old = pages.requests.pop_front().unwrap();
    pages.queue(
        "market:resolve",
        json!({"op":"market_resolve","labels":["new"]}),
    );
    pages.update(Update::Reply {
        request: old,
        result: Ok(json!({"resolutions":[{"matches":[{"name":"Old"}]}]})),
    });
    assert!(pages.market.matches.is_null());
}

#[test]
fn market_dialog_captures_do_not_mutate_the_account() {
    let mut pages = pages();
    pages.set_page(Page::Market);
    pages.fixture = false;
    for view in ["edit-order", "delete-order"] {
        pages.capture_state(view, "example", "", None).unwrap();
        assert!(pages.ui.bounds("market.dialog").is_some());
        assert!(pages.ui.modal_open());
        assert!(
            pages
                .requests
                .iter()
                .all(|r| !r.key.starts_with("market:write"))
        );
        pages.market.dialog = None;
    }
    for view in ["close-order", "visibility", "add"] {
        assert!(pages.capture_state(view, "example", "", None).is_err());
    }
}
