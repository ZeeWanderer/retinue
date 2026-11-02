use super::*;

pub(super) struct TestImages {
    embedded: retinue_render::thumbnails::Thumbnails,
    artwork: retinue_render::resources::ResourceCache<Key, RasterImage>,
}

impl TestImages {
    pub(super) fn new() -> Self {
        Self {
            embedded: retinue_render::thumbnails::Thumbnails::new(0, 16 * 1024 * 1024),
            artwork: retinue_render::resources::ResourceCache::new(1024 * 1024),
        }
    }

    pub(super) fn load(&mut self, pages: &mut Pages) {
        use retinue_render::{image, thumbnails::Source};
        pages.media.assets(
            pages
                .ui
                .assets()
                .into_iter()
                .map(|asset| Descriptor {
                    id: asset.id,
                    path: "test-artwork.png".into(),
                    digest: "35".repeat(32),
                })
                .collect(),
        );
        let requests = pages.ui.requests(&pages.media);
        let ready = requests
            .iter()
            .map(|r| {
                let image = match &r.source {
                    Source::Embedded(_) => self.embedded.prepare(r).unwrap(),
                    Source::File(_) => self.artwork.get_or_insert_with(r.key, || {
                        RasterImage::from_rgba(image::RgbaImage::from_fn(16, 16, |x, y| {
                            image::Rgba([40 + x as u8 * 12, 60 + y as u8 * 10, 220, 255])
                        }))
                        .unwrap()
                    }),
                };
                (r.key, image)
            })
            .collect::<Vec<_>>();
        pages.media.accept(&ready, &requests);
    }
}

pub(super) fn assert_pixels_match(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len());
    let maximum = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    let changed = actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
        .count();
    assert!(
        maximum <= 2,
        "{label}: {changed} changed pixels, max delta {maximum}"
    );
}

fn pages() -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    pages.configure((1000, 900), (400, 900), 1.0);
    pages
}

#[test]
fn hovered_mod_capture_has_stable_expanded_geometry() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages();
        pages.configure(
            ((1000.0 * scale) as u32, (900.0 * scale) as u32),
            (400, 900),
            scale,
        );
        pages.set_page(Page::Builds);
        pages.layout();
        pages.capture_hover("build:mod:mod-1").unwrap();
        let preview = pages.ui.bounds("build:mod-preview").unwrap();
        assert!(preview.height > (260.0 * scale) as u32);
        pages.settle_capture();
        assert_eq!(pages.ui.bounds("build:mod-preview"), Some(preview));
    }
}

#[test]
fn listing_filters_are_capsule_popovers_without_grid_reflow_or_scroll_reset() {
    use retinue_ui::{
        desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
        desktop::winit::keyboard::NamedKey,
    };
    for page in [Page::Inventory, Page::Mastery] {
        for scale in [1.0, 1.25, 2.0] {
            let mut pages = pages();
            pages.set_page(page);
            let mut data = pages.listing().data.clone();
            data["items"] = json!(
                (0..100)
                    .map(|i| {
                        let mut row = serde_json::to_value(&pages.listing().items[0]).unwrap();
                        row["id"] = json!(format!("row-{i}"));
                        row
                    })
                    .collect::<Vec<_>>()
            );
            let quotes = pages.quotes.clone();
            pages.listing_mut().replace_fixture(data, &quotes);
            pages.dirty = true;
            pages.configure((1600, 1200), (400, 1200), scale);
            pages.listing_mut().scroll.to(200.0, false, Instant::now());
            pages.listing_mut().tick(Instant::now());
            pages.dirty = true;
            pages.layout();
            assert!(pages.listing().offset > 0);
            let offset = pages.listing().offset;
            let grid = pages.ui.bounds("page.grid").unwrap();
            let button = pages.ui.bounds("list:filters").unwrap();
            assert_eq!(button.width, pages.ui.px(32.0));
            assert_eq!(button.height, button.width);
            pages.listing_signal(Signal {
                key: "list:filters".into(),
                value: None,
            });
            pages.layout();
            assert!(pages.listing_filters_open());
            assert_eq!(pages.listing().offset, offset);
            assert_eq!(pages.ui.bounds("page.grid"), Some(grid));
            let popup = pages.ui.bounds("page.filters").unwrap();
            assert!(popup.x + popup.width <= pages.ui.size.0);
            assert!(popup.y + popup.height <= pages.ui.size.1);
            assert!(pages.ui.modal_open());
            pages.listing_signal(Signal {
                key: "list:filter:prime:yes".into(),
                value: None,
            });
            pages.layout();
            assert_eq!(pages.listing().prime, "yes");
            pages.listing_signal(Signal {
                key: "list:filter:prime:yes".into(),
                value: None,
            });
            pages.layout();
            assert_eq!(pages.listing().prime, "all");
            assert!(pages.listing_filters_open());
            assert!(pages.key(false, Key::Named(NamedKey::Escape), ModifiersState::empty()));
            assert!(!pages.listing_filters_open());
            assert_eq!(pages.ui.focus.as_deref(), Some("list:filters"));
            assert_eq!(pages.ui.bounds("page.grid"), Some(grid));
            pages.listing_signal(Signal {
                key: "list:filters".into(),
                value: None,
            });
            pages.layout();
            pages.ui.pointer_move((button.x as f64, button.y as f64));
            pages.button(false, true);
            pages.button(false, false);
            assert!(
                !pages.listing_filters_open(),
                "dismissal release reopened filters"
            );
        }
    }
}

#[test]
fn filter_keyboard_reveals_clipped_options_and_leaving_page_closes_popup() {
    use retinue_ui::{
        desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
        desktop::winit::keyboard::NamedKey,
    };
    let mut pages = pages();
    pages.set_page(Page::Inventory);
    pages.configure((800, 500), (300, 500), 2.0);
    pages.capture_state("filters", "", "", None).unwrap();
    let viewport = pages.ui.bounds("page.filters.content").unwrap();
    for _ in 0..8 {
        pages.key(false, Key::Named(NamedKey::Tab), ModifiersState::empty());
    }
    pages.tick(Instant::now() + Duration::from_secs(1));
    let target = pages.ui.bounds("list:filter:complete:yes").unwrap();
    assert_eq!(target.intersection(viewport), target);
    assert!(target.height > 0);
    pages.set_page(Page::Mastery);
    pages.set_page(Page::Inventory);
    pages.layout();
    assert!(!pages.listing_filters_open());
}

#[test]
fn listing_search_capsules_expand_left_beside_their_tools() {
    for page in [Page::Mastery, Page::Inventory] {
        for scale in [1.0, 1.25, 2.0] {
            let mut pages = pages();
            pages.set_page(page);
            pages.configure((1600, 1200), (400, 1200), scale);
            let collapsed = pages.ui.bounds("list:search").unwrap();
            let tool = pages
                .ui
                .bounds(if page == Page::Inventory {
                    "list:sort"
                } else {
                    "list:filters"
                })
                .unwrap();
            assert_eq!(collapsed.y, tool.y);
            assert!(tool.x.abs_diff(collapsed.x + collapsed.width) <= pages.ui.px(12.0));
            pages.search(page, "Ember Prime Neuroptics Blueprint");
            let expanded = pages.ui.bounds("list:search").unwrap();
            assert!(expanded.x < collapsed.x);
            assert_eq!(expanded.x + expanded.width, collapsed.x + collapsed.width);
            assert!(expanded.x + expanded.width <= tool.x);
            assert!(pages.ui.bounds("clear:list:search").is_some());
        }
    }
}

#[test]
fn market_checks_account_on_connect_and_does_not_flash_login() {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.configure((1000, 900), (400, 900), 1.0);
    pages.update(Update::Connected);
    assert!(pages.pending.contains_key("market"));
    let count = pages.requests.len();
    pages.set_page(Page::Market);
    pages.layout();
    assert_eq!(pages.requests.len(), count);
    assert!(pages.ui.bounds("market:email").is_none());
    assert!(pages.ui.bounds("market.account.loading").is_some());
    let request = pages
        .requests
        .iter()
        .find(|r| r.key == "market")
        .unwrap()
        .clone();
    pages.capture_reply(request.clone(), Err("Account unavailable".into()));
    assert!(pages.ui.bounds("market:email").is_none());
    assert!(pages.ui.bounds("market:refresh").is_some());
    assert_eq!(pages.ui.animation_damage(), Damage::None);
    pages.capture_reply(request, Ok(json!({"authenticated":false})));
    assert!(pages.ui.bounds("market:email").is_some());
    assert!(pages.ui.bounds("market.account.loading").is_none());
}

#[test]
fn rail_loading_is_independent_and_stops_on_reply_or_failure() {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.configure((1000, 900), (400, 900), 1.0);
    pages.update(Update::Connected);
    let tick = pages.rail.deadline().unwrap();
    let mut repainted = false;
    for frame in 0..22 {
        let now = tick + Duration::from_millis(frame * 16);
        let (center, rail) = pages.tick(now);
        assert_eq!(center, Damage::None);
        repainted |= rail != Damage::None;
        let report = pages.rail.layout_report();
        let composition = &report["composition"];
        assert_eq!(composition["view"]["bodies"], 0);
        assert_eq!(composition["view"]["mounted"], 0);
        assert_eq!(composition["view"]["unmounted"], 0);
        assert_eq!(composition["layout_computations"], 0);
        assert!(pages.rail.deadline().unwrap() <= now + Duration::from_millis(16));
    }
    assert!(repainted);
    assert!(pages.rail.bounds("rail.activity.loading").is_some());
    let request = pages
        .requests
        .iter()
        .find(|r| r.key == "activity")
        .unwrap()
        .clone();
    pages.capture_reply(request.clone(), Err("Worldstate unavailable".into()));
    assert_eq!(pages.rail.animation_damage(), Damage::None);
    assert!(pages.rail.bounds("rail:retry").is_some());
    pages.capture_reply(request, Ok(json!({"cycles":[],"fissures":[]})));
    assert!(pages.rail.bounds("rail.activity.loading").is_none());
    assert_eq!(pages.rail.animation_damage(), Damage::None);
}

#[test]
fn captured_listing_mode_requests_its_prices() {
    let mut pages = pages();
    pages.set_page(Page::Mastery);
    pages.fixture = false;
    pages.capture_state("platinum", "", "Ember", None).unwrap();
    assert!(pages.requests.iter().any(|request| {
        request.body["op"] == "market_quote"
            && request.body["cache_only"] == true
            && entries(&request.body["items"]).contains(&json!("Ember Prime Neuroptics Blueprint"))
    }));
    assert!(pages.ui.bounds("mastery.prices.loading").is_some());
}

#[test]
fn fresh_cache_finishes_price_loading_without_another_network_round() {
    let mut pages = pages();
    pages.fixture = false;
    pages.connected = true;
    pages.set_page(Page::Mastery);
    pages.start_prices(false);
    let request = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("cache:"))
        .unwrap()
        .clone();
    let names = entries(&request.body["items"]).to_vec();
    assert!(!names.is_empty());
    let quotes: Vec<_> = names
        .iter()
        .map(|n| json!({"item":n,"quote":{"lowest_sell":10,"quoted_at":unix_ms()}}))
        .collect();
    pages.capture_reply(request, Ok(json!({"quotes":quotes})));
    assert!(pages.quote_pending.is_empty());
    assert!(pages.ui.bounds("mastery.prices.loading").is_none());
    assert!(!pages.pending.keys().any(|k| k.starts_with("price:")));
    let count = pages.requests.len();
    pages.start_prices(false);
    assert_eq!(
        pages.requests.len(),
        count,
        "filter changes must not restart fresh prices"
    );
}

#[test]
fn unfinished_quote_sweep_is_not_restarted_and_other_pages_do_not_keep_bar_alive() {
    let mut pages = pages();
    pages.fixture = false;
    pages.connected = true;
    pages.set_page(Page::Mastery);
    pages.start_prices(false);
    let request = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("cache:"))
        .unwrap()
        .clone();
    pages.capture_reply(request, Ok(json!({"quotes":[]})));
    let network = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("price:"))
        .unwrap()
        .clone();
    let pending = pages.pending.clone();
    pages.last_prices = Instant::now() - Duration::from_secs(61);
    pages.tick(Instant::now());
    assert_eq!(pages.pending, pending);
    assert!(pages.ui.bounds("mastery.prices.loading").is_some());
    pages.quote_pending.insert("another page's item".into());
    pages.capture_reply(network, Err("Request timed out".into()));
    assert!(pages.ui.bounds("mastery.prices.loading").is_none());
    assert_eq!(pages.ui.animation_damage(), Damage::None);
}

#[test]
fn filtering_a_price_sweep_drops_hidden_work_and_finishes_the_visible_item() {
    let mut pages = pages();
    let mut rows = Vec::new();
    for i in 0..24 {
        let mut row = pages.mastery.items[0].clone();
        row.id = format!("item-{i:02}");
        row.name = if i == 23 {
            "Target".into()
        } else {
            format!("Hidden {i}")
        };
        row.components[0].market_name = Some(format!("part-{i:02}"));
        row.components[0].owned = 0;
        row.components[0].required = 1;
        rows.push(row);
    }
    pages.mastery.items = rows;
    pages.fixture = false;
    pages.connected = true;
    pages.set_page(Page::Mastery);
    let cache = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("cache:"))
        .unwrap()
        .clone();
    pages.capture_reply(cache, Ok(json!({"quotes":[]})));
    assert_eq!(pages.quote_queue.len(), 16);
    let active = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("price:"))
        .unwrap()
        .clone();

    pages.listing_signal(Signal {
        key: "list:search".into(),
        value: Some("Target".into()),
    });
    assert_eq!(
        pages
            .quote_queue
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["part-23"]
    );
    pages.capture_reply(active, Err("Request timed out".into()));
    let target = pages
        .requests
        .iter()
        .find(|r| r.body["items"] == json!(["part-23"]))
        .unwrap()
        .clone();
    pages.capture_reply(
        target,
        Ok(json!({"quotes":[{"item":"part-23","quote":{"lowest_sell":10,"quoted_at":unix_ms()}}]})),
    );
    assert!(pages.ui.bounds("mastery.prices.loading").is_none());
    assert!(pages.quote_queue.is_empty());
}

#[test]
fn late_cache_replies_do_not_fetch_prices_for_a_hidden_page() {
    let mut pages = pages();
    pages.fixture = false;
    pages.connected = true;
    pages.set_page(Page::Mastery);
    let cache = pages
        .requests
        .iter()
        .find(|r| r.key.starts_with("cache:"))
        .unwrap()
        .clone();
    pages.set_page(Page::Builds);
    pages.capture_reply(cache, Ok(json!({"quotes":[]})));
    assert!(pages.quote_queue.is_empty());
    assert!(pages.quote_pending.is_empty());
    assert!(!pages.pending.keys().any(|key| key.starts_with("price:")));
}

#[test]
fn listing_scrollbar_has_reserved_space_and_drag_events_coalesce() {
    let mut pages = pages();
    let source = serde_json::to_value(&pages.relics.items[0]).unwrap();
    let items: Vec<_> = (0..100)
        .map(|i| {
            let mut row = source.clone();
            row["id"] = json!(format!("relic-{i}"));
            row["name"] = json!(format!("Lith {i:03}"));
            row
        })
        .collect();
    pages
        .relics
        .replace_fixture(json!({"items": items}), &pages.quotes);
    pages.set_page(Page::Relics);
    for scale in [1.0, 1.25, 2.0] {
        pages.configure((1600, 1000), (400, 1000), scale);
        pages.relics.scroll.to(0.0, false, Instant::now());
        pages.relics.tick(Instant::now());
        pages.dirty = true;
        pages.layout();
        let viewport = pages.ui.bounds("page.grid").unwrap();
        let track = pages.ui.bounds("list:scroll").unwrap();
        assert!(track.x >= viewport.x + viewport.width + pages.ui.px(4.0));
        let card = pages.ui.bounds("page.card").unwrap();
        assert!(card.x + card.width <= viewport.x + viewport.width);
        pages.pointer(false, (track.x as f64 + 2.0, track.y as f64 + 2.0));
        pages.button(false, true);
        pages.tick(Instant::now());
        let center = pages.ui.layout_report()["composition"].clone();
        let rail = pages.rail.layout_report()["composition"].clone();
        for event in 0..20 {
            pages.pointer(
                false,
                (track.x as f64 + 2.0, track.y as f64 + 30.0 + event as f64),
            );
        }
        assert_eq!(pages.ui.layout_report()["composition"], center);
        assert_eq!(
            pages.ui.signals.len(),
            1,
            "keep only the latest drag position"
        );
        let (damage, rail_damage) = pages.tick(Instant::now());
        assert_ne!(damage, Damage::None);
        assert!(pages.relics.offset > 0);
        assert_eq!(rail_damage, Damage::None);
        assert_eq!(pages.rail.layout_report()["composition"], rail);
        pages.button(false, false);
    }
}

#[test]
fn context_menus_restore_focus_after_keyboard_dismissal_and_refresh() {
    use retinue_ui::{
        desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
        desktop::winit::keyboard::NamedKey,
    };
    let mut pages = pages();
    for page in [Page::Inventory, Page::Mastery, Page::Relics] {
        pages.set_page(page);
        let index = pages.listing().visible[0];
        let invoker = format!("list:item:{index}:menu");
        pages.ui.focus = Some(invoker.clone());
        for key in [NamedKey::Escape, NamedKey::Tab] {
            assert!(pages.keyboard_context_menu());
            assert!(pages.key(false, Key::Named(key), ModifiersState::empty()));
            assert!(pages.listing().menu.is_none());
            assert_eq!(pages.ui.focus.as_ref(), Some(&invoker), "{page:?}");
            assert!(pages.ui.bounds("page.menu").is_none());
        }
        assert!(pages.keyboard_context_menu());
        pages.listing_mut().menu = None;
        pages.dirty = true;
        pages.layout();
        assert_eq!(pages.ui.focus.as_ref(), Some(&invoker), "{page:?}");
    }
}

#[test]
fn category_capture_commits_the_sampled_page_geometry() {
    let mut pages = pages();
    for (page, source, target) in [
        (Page::Inventory, "parts", "relics"),
        (Page::Mastery, "all", "warframes"),
        (Page::Relics, "all", "lith"),
    ] {
        pages.set_page(page);
        pages.layout();
        let collapsed = pages.ui.px(if page == Page::Relics { 40.0 } else { 38.0 });
        pages
            .capture_category(0, 1, Duration::from_millis(85))
            .unwrap();
        let from = pages.ui.bounds(&format!("list:category:{source}")).unwrap();
        let to = pages.ui.bounds(&format!("list:category:{target}")).unwrap();
        assert!(from.width > collapsed, "outgoing {page:?}");
        assert!(to.width > collapsed, "incoming {page:?}");
        pages
            .capture_category(0, 1, Duration::from_millis(170))
            .unwrap();
        assert_eq!(
            pages
                .ui
                .bounds(&format!("list:category:{source}"))
                .unwrap()
                .width,
            collapsed
        );
        assert!(
            pages
                .ui
                .bounds(&format!("list:category:{target}"))
                .unwrap()
                .width
                > to.width
        );
    }
}

#[test]
fn relic_toolbar_centers_collapsed_icons_and_anchors_search_to_the_right() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages();
        pages.set_page(Page::Relics);
        pages.configure((1600, 1000), (400, 1000), scale);
        pages.search(Page::Relics, "");
        let tab = pages.ui.bounds("list:category:lith").unwrap();
        let icon = pages.ui.bounds("list:category:lith:icon").unwrap();
        assert_eq!(
            (tab.width, tab.height),
            (pages.ui.px(40.0), pages.ui.px(32.0))
        );
        assert_eq!(
            (icon.width, icon.height),
            (pages.ui.px(24.0), pages.ui.px(24.0))
        );
        assert!((2 * tab.x + tab.width).abs_diff(2 * icon.x + icon.width) <= 1);
        assert!((2 * tab.y + tab.height).abs_diff(2 * icon.y + icon.height) <= 1);
        let collapsed = pages.ui.bounds("list:search").unwrap();
        let last_tab = pages.ui.bounds("list:category:vanguard").unwrap();
        assert!(collapsed.y > last_tab.y || collapsed.x > last_tab.x + last_tab.width);
        pages.search(Page::Relics, "Ember Prime Neuroptics Blueprint");
        let expanded = pages.ui.bounds("list:search").unwrap();
        assert_eq!(expanded.x + expanded.width, collapsed.x + collapsed.width);
        assert_eq!(expanded.y, collapsed.y);
        assert!(expanded.x < collapsed.x);
        assert!(expanded.y > last_tab.y || expanded.x > last_tab.x + last_tab.width);
        assert!(pages.ui.bounds("clear:list:search").is_some());
    }
}

#[test]
fn fissure_navigation_refilters_existing_relics_and_animates_the_selected_tab() {
    for starting_page in [Page::Inventory, Page::Relics] {
        let mut pages = pages();
        let lith = serde_json::to_value(&pages.relics.items[0]).unwrap();
        let mut axi = lith.clone();
        axi["id"] = json!("axi-a1");
        axi["name"] = json!("Axi A1");
        axi["era"] = json!("Axi");
        pages
            .relics
            .replace_fixture(json!({"items": [lith, axi]}), &pages.quotes);
        pages.search(Page::Relics, "Lith");
        pages.set_page(starting_page);
        pages.layout();
        assert_eq!(pages.relics.visible.len(), 1);
        pages.rail.signals.push(Signal {
            key: "rail:era:axi".into(),
            value: None,
        });
        pages.tick(Instant::now());
        let (destination, query) = pages.navigation.take().unwrap();
        assert_eq!(destination, Page::Relics);
        assert!(query.is_empty());
        pages.set_page(destination);
        pages.layout();
        assert!(pages.relics.search.is_empty());
        assert_eq!(pages.relics.category, "axi");
        assert_eq!(pages.relics.visible.len(), 1);
        assert_eq!(
            pages.relics.items[pages.relics.visible[0]].era.as_str(),
            "Axi"
        );
        assert_eq!(pages.relics.offset, 0);
        let start = Instant::now();
        assert!(pages.relics.deadline(start).is_some());
        let initial = pages.ui.bounds("list:category:axi").unwrap();
        pages.tick(start + Duration::from_millis(85));
        let middle = pages.ui.bounds("list:category:axi").unwrap();
        assert!(middle.width > initial.width);
        pages.tick(start + Duration::from_millis(170));
        let end = pages.ui.bounds("list:category:axi").unwrap();
        assert!(end.width > middle.width);
        assert_eq!(
            pages.relics.deadline(start + Duration::from_millis(170)),
            None
        );
        pages.rail.signals.push(Signal {
            key: "rail:era:omnia".into(),
            value: None,
        });
        pages.tick(Instant::now());
        assert_eq!(pages.relics.category, "all");
        assert_eq!(pages.relics.visible.len(), 2);
    }
}

#[test]
fn resizing_one_region_preserves_the_other_scene() {
    let mut pages = pages();
    pages.activity["fissures"][0]["mission"] = json!("Defense");
    pages.set_page(Page::Relics);
    pages.layout();
    let rail = pages.rail.layout_report()["composition"].clone();
    assert!(rail["recordings"].as_u64().unwrap() > 0);
    pages.configure((1200, 900), (400, 900), 1.0);
    assert_eq!(pages.rail.layout_report()["composition"], rail);
    let center = pages.ui.layout_report()["composition"].clone();
    pages.configure((1200, 900), (500, 900), 1.0);
    assert_eq!(pages.ui.layout_report()["composition"], center);
}

#[test]
fn rail_only_ticks_remove_expired_fissures_from_the_retained_scene() {
    let mut pages = pages();
    pages.activity["fissures"][0]["expiry"] = json!(unix_ms() + 5000);
    pages.layout_rail();
    let era = pages.rail.bounds("rail:era:lith").unwrap();
    let point = (era.x as f64 + 10.0, era.y as f64 + 10.0);
    pages.rail.pointer_move(point);
    assert_eq!(pages.rail.hover.as_deref(), Some("rail:era:lith"));
    let (center, rail) = pages.tick(Instant::now() + Duration::from_secs(6));
    assert_eq!(center, Damage::None);
    assert_eq!(rail, Damage::Full);
    pages.rail.pointer_move(point);
    assert_eq!(pages.rail.hover, None);
    assert!(pages.rail.bounds("rail:era:lith").is_none());
}

#[test]
fn rail_scene_is_committed_for_market_and_hidden_states() {
    let mut pages = pages();
    pages.rail_tab = true;
    pages.layout_rail();
    let report = pages.rail.layout_report();
    assert!(report["composition"]["nodes"].as_u64().unwrap() > 1);
    let button = pages.rail.bounds("rail:tab:market").unwrap();
    pages
        .rail
        .pointer_move((button.x as f64 + 10.0, button.y as f64 + 10.0));
    assert_eq!(pages.rail.hover.as_deref(), Some("rail:tab:market"));
    pages.configure((1000, 900), (0, 900), 1.0);
    let report = pages.rail.layout_report();
    assert_eq!(report["composition"]["chunks"], 0);
    assert!(report["controls"].as_array().unwrap().is_empty());
    pages.rail.pointer_move((10.0, 10.0));
    assert_eq!(pages.rail.hover, None);
}

#[test]
fn retained_rail_preserves_notification_input_and_era_navigation() {
    use retinue_ui::{
        desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
        desktop::winit::keyboard::NamedKey,
    };
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages();
        pages.configure(
            (1000, 900),
            ((400.0 * scale) as u32, (900.0 * scale) as u32),
            scale,
        );
        let button = pages.rail.bounds("rail:notification").unwrap();
        pages
            .rail
            .pointer_move(((button.x + 3) as f64, (button.y + 3) as f64));
        pages.rail.pointer_button(true);
        pages.rail.pointer_button(false);
        assert!(pages.rail.popup_open());
        pages.layout_rail();
        assert!(pages.rail.popup_open());
        pages
            .rail
            .key(Key::Named(NamedKey::ArrowDown), ModifiersState::empty());
        pages
            .rail
            .key(Key::Named(NamedKey::Enter), ModifiersState::empty());
        assert!(!pages.rail.popup_open());
        let signal = pages.rail.signals.pop().unwrap();
        assert_eq!(signal.key, "rail:notification");
        assert_eq!(signal.value.as_deref(), Some("persistent"));
        let era = pages.rail.bounds("rail:era:lith").unwrap();
        pages.rail.pointer_move((
            (era.x + era.width / 2) as f64,
            (era.y + era.height / 2) as f64,
        ));
        pages.rail.pointer_button(true);
        pages.rail.pointer_button(false);
        let signal = pages.rail.signals.pop().unwrap();
        assert_eq!(signal.key, "rail:era:lith");
        pages.rail_signal(signal);
        assert_eq!(pages.navigation.as_ref().map(|n| n.0), Some(Page::Relics));
    }
}

#[test]
fn inactive_event_views_drop_visual_timers_without_dropping_service_refresh() {
    let mut pages = pages();
    assert!(pages.rail.deadline().is_some());
    pages.configure((1000, 900), (0, 900), 1.0);
    assert_eq!(pages.rail.deadline(), None);
    assert!(pages.deadline().is_some());
    pages.configure((1000, 900), (400, 900), 1.0);
    assert!(pages.rail.deadline().is_some());
    pages.rail_tab = true;
    pages.layout_rail();
    assert!(
        pages.rail.layout_report()["composition"]["declarative"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(pages.rail.deadline(), None);
    pages.fixture = false;
    pages.requests.clear();
    pages.tick(Instant::now() + Duration::from_secs(61));
    assert!(pages.requests.iter().any(|r| r.key == "activity"));
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
                .any(|e| e["kind"] == "fill" && e["color"] == "#6e9b6878")
        );
        assert!(
            elements
                .iter()
                .any(|e| e["kind"] == "inset_shadow" && e["color"] == "#46d234ff")
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
                if let retinue_render::thumbnails::Source::Embedded(bytes) = request.source {
                    assert!(
                        retinue_render::image::load_from_memory(bytes).is_ok(),
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
            result: Ok(crate::domain::session::Reply::Json(json!({}))),
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
        result: Ok(crate::domain::session::Reply::Json(
            json!({"resolutions":[{"matches":[{"name":"Old"}]}]}),
        )),
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
