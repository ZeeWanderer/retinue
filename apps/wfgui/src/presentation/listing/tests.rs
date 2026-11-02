use super::*;

#[test]
fn listing_layout_preserves_fractional_coordinates_across_scales() {
    let expected = Bounds::new(17.25, 12.125, 180.125, 27.5);
    for scale in [1.0, 1.25, 2.0] {
        let metrics = Metrics {
            scale,
            ..Default::default()
        };
        assert_eq!(
            metrics.bounds(expected.x, expected.y, expected.width, expected.height),
            expected
        );
        assert_eq!(
            metrics.local(Bounds::new(4.5, 7.25, 200.0, 100.0), expected),
            expected
        );
    }
}

fn pages(scale: f32, count: usize) -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    pages.set_page(Page::Inventory);
    let template = pages.inventory.items[0].clone();
    pages.inventory.items = (0..count)
        .map(|i| {
            let mut row = template.clone();
            row.id = format!("row-{i:05}");
            row.name = format!("Item {i:05}");
            row
        })
        .collect();
    pages.inventory.reorder(&pages.quotes, true);
    pages.configure(
        ((1000.0 * scale) as u32, (700.0 * scale) as u32),
        (400, 700),
        scale,
    );
    pages.layout_center();
    pages
}

fn render(pages: &Pages) -> Vec<u8> {
    let (w, h) = pages.ui.size;
    let mut pixels = vec![0; (w * h * 4) as usize];
    let mut p = Painter::new(&mut pixels, w, h).unwrap();
    pages.ui.paint(&mut p, &pages.media);
    p.finish().unwrap();
    pixels
}

#[test]
fn retained_listing_records_are_shared_until_the_source_changes() {
    let mut pages = pages(1.0, 80);
    let key = pages.inventory_view.model.cells.order().keys()[0];
    let before = pages.inventory_view.model.cells.get(key).unwrap();
    pages.ui.hover = Some(format!("list:item:{}:menu", before.index));
    pages.layout_center();
    let hovered = pages.inventory_view.model.cells.get(key).unwrap();
    assert!(std::rc::Rc::ptr_eq(&before.item, &hovered.item));
    assert!(hovered.hovered);
    pages.inventory.items[before.index].name = "Updated source".into();
    pages.layout_center();
    let changed = pages.inventory_view.model.cells.get(key).unwrap();
    assert!(!std::rc::Rc::ptr_eq(&before.item, &changed.item));
    assert_eq!(changed.item.name, "Updated source");
    assert_ne!(before.item.name, changed.item.name);
}

#[test]
fn scrolling_listing_artwork_matches_a_fresh_tree() {
    use crate::{presentation::tests::TestImages, presentation::tests::assert_pixels_match};
    for page in [Page::Inventory, Page::Relics] {
        for scale in [1.0, 1.25, 2.0] {
            let mut pages = pages(scale, 80);
            let listing = if page == Page::Inventory {
                &mut pages.inventory
            } else {
                &mut pages.relics
            };
            let template = listing.items[0].clone();
            listing.items = (0..80)
                .map(|i| {
                    let mut row = template.clone();
                    row.id = format!("artwork-{i:03}");
                    row.name = format!("Item {i:03}");
                    let asset = Asset {
                        id: "test.artwork".into(),
                        source: "wfcd".into(),
                        image_name: "artwork.png".into(),
                    };
                    row.asset = Some(asset.clone());
                    for reward in &mut row.rewards {
                        reward.asset = Some(asset.clone());
                    }
                    row
                })
                .collect();
            listing.reorder(&pages.quotes, true);
            pages.set_page(page);
            let mut images = TestImages::new();
            for offset in [0.0, 8.0, 36.0, 90.0, 135.0, 270.0, 800.0, 430.0, 75.0, 0.0] {
                let listing = if page == Page::Inventory {
                    &mut pages.inventory
                } else {
                    &mut pages.relics
                };
                listing.scroll.to(offset * scale, false, Instant::now());
                pages.layout_center();
                images.load(&mut pages);
                let retained = render(&pages);
                let view = if page == Page::Inventory {
                    &pages.inventory_view
                } else {
                    &pages.relics_view
                };
                pages.ui.install_view(view.view(&pages.ui));
                pages.layout_center();
                let fresh = render(&pages);
                assert_pixels_match(
                    &retained,
                    &fresh,
                    &format!("{page:?} scale={scale} offset={offset}"),
                );
            }
        }
    }
}

fn check_replay(pages: &Pages, label: &str) {
    let cached = render(pages);
    pages.ui.with_full_replay(|| {
        let replay = render(pages);
        let changed = cached
            .chunks_exact(4)
            .zip(replay.chunks_exact(4))
            .enumerate()
            .filter_map(|(i, (a, b))| {
                (a != b).then(|| {
                    (
                        (i as u32 % pages.ui.size.0, i as u32 / pages.ui.size.0),
                        a,
                        b,
                    )
                })
            })
            .collect::<Vec<_>>();
        let maximum = cached
            .iter()
            .zip(&replay)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        // A precomposed translucent glow rounds twice in an 8-bit raster.
        assert!(
            maximum <= 2,
            "{label} scale={} changed={} max={} first={:?}",
            pages.ui.scale,
            changed.len(),
            maximum,
            &changed[..changed.len().min(4)]
        );
    });
}

#[test]
fn inventory_mounts_only_visible_data_and_keeps_order_on_scroll_and_quotes() {
    let mut pages = pages(1.25, 10_000);
    let order = pages.inventory_view.model.order.get();
    let revision = pages.inventory_view.revision;
    let first = order.keys()[0];
    assert_eq!(order.len(), 10_000);
    assert!(pages.inventory_view.model.cells.order().len() < 20);
    assert!(pages.ui.view_bounds(first).is_some());
    assert!(pages.ui.view_bounds(order.keys()[500]).is_none());
    let before = render(&pages);
    pages
        .quotes
        .get_mut("Ember Prime Neuroptics Blueprint")
        .unwrap()
        .sell = Some(123.0);
    pages.layout_center();
    assert_ne!(render(&pages), before);
    assert_eq!(pages.inventory_view.revision, revision);
    assert_eq!(pages.inventory_view.model.order.get(), order);
    pages.layout_center();
    assert_eq!(pages.ui.layout_report()["composition"]["view"]["bodies"], 0);
    assert_eq!(
        pages.ui.layout_report()["composition"]["layout_computations"],
        0
    );
    pages.inventory.scroll.to(1500.0, false, Instant::now());
    pages.layout_center();
    assert_eq!(pages.inventory_view.revision, revision);
    assert_eq!(pages.inventory_view.model.order.get(), order);
    assert!(pages.inventory_view.model.cells.get(first).is_none());
    assert!(pages.ui.view_bounds(first).is_none());
    assert!(pages.inventory_view.model.cells.order().len() < 20);
    check_replay(&pages, "scrolled");
    let scroll = pages.ui.bounds("list:scroll").unwrap();
    assert!(scroll.x >= pages.inventory.viewport.x + pages.inventory.viewport.width);
    pages.inventory.search = "Item 00000".into();
    pages.inventory.reorder(&pages.quotes, false);
    pages.layout_center();
    assert_eq!(pages.inventory_view.model.order.get().keys(), &[first]);
    assert_eq!(pages.inventory.offset, 0);
    assert!(pages.ui.bounds("list:scroll").is_none());
}

#[test]
fn inventory_reorder_and_page_switch_preserve_identity_not_hidden_controls() {
    let mut pages = pages(1.0, 4);
    let first = pages.inventory_view.model.order.get().keys()[0];
    let before = pages.ui.view_bounds(first).unwrap();
    pages.inventory.descending = true;
    pages.inventory.reorder(&pages.quotes, true);
    pages.layout_center();
    assert_eq!(pages.inventory_view.model.order.get().keys()[3], first);
    assert_ne!(pages.ui.view_bounds(first).unwrap(), before);
    let after = pages.ui.view_bounds(first).unwrap();
    for page in [Page::Mastery, Page::Relics, Page::Market, Page::Builds] {
        pages.set_page(page);
        pages.layout_center();
        assert!(pages.ui.view_bounds(first).is_none());
        assert!(pages.ui.bounds("list:sort").is_none());
        pages.set_page(Page::Inventory);
        pages.layout_center();
        assert_eq!(pages.ui.view_bounds(first), Some(after));
        assert_eq!(pages.inventory_view.model.order.get().keys()[3], first);
    }
}

#[test]
fn inventory_cards_keep_component_overflow_and_clipped_input_at_all_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages(scale, 1);
        let mut row = pages.inventory.items[0].clone();
        row.group = "sets".into();
        row.mastered = true;
        row.is_prime = true;
        row.vaulted = Some(true);
        let part = pages.mastery.items[0].components[0].clone();
        row.components = (0..6)
            .map(|i| {
                let mut p = part.clone();
                p.owned = 1000 + i;
                p
            })
            .collect();
        pages.inventory.category = "sets".into();
        pages.inventory.items = vec![row];
        pages.inventory.reorder(&pages.quotes, true);
        pages.layout_center();
        let id = pages.inventory_view.model.order.get().keys()[0];
        let image = pages
            .ui
            .view_geometry(inventory::part_key(id, 0, false))
            .unwrap()
            .layout;
        let badge = pages
            .ui
            .view_geometry(inventory::part_key(id, 0, true))
            .unwrap()
            .layout;
        assert!((badge.y + badge.height - image.y - image.height - 2.0).abs() < 0.001);
        assert!(badge.x + badge.width > image.x + image.width);
        let r = pages.ui.bounds("list:item:0:component:5").unwrap();
        pages
            .ui
            .pointer_move(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64));
        assert_eq!(pages.ui.hover.as_deref(), Some("list:item:0:component:5"));
        pages.layout_center();
        check_replay(&pages, "cards");
        for view in ["filters", "context-menu"] {
            pages.capture_state(view, "", "", None).unwrap();
            check_replay(&pages, view);
        }
    }
}

fn listing_view(pages: &Pages) -> &ListingController {
    match pages.page {
        Page::Inventory => &pages.inventory_view,
        Page::Mastery => &pages.mastery_view,
        Page::Relics => &pages.relics_view,
        _ => unreachable!(),
    }
}

#[test]
fn mastery_and_relic_windows_keep_offscreen_data_unmounted_and_order_stable() {
    for page in [Page::Mastery, Page::Relics] {
        let mut pages = pages(1.25, 1);
        pages.set_page(page);
        let template = pages.listing().items[0].clone();
        pages.listing_mut().items = (0..10_000)
            .map(|i| {
                let mut item = template.clone();
                item.id = format!("id-{i:05}");
                item.name = format!("Item {i:05}");
                item
            })
            .collect();
        let quotes = pages.quotes.clone();
        pages.listing_mut().reorder(&quotes, true);
        pages.layout_center();
        let order = listing_view(&pages).model.order.get();
        let revision = listing_view(&pages).revision;
        assert_eq!(order.len(), 10_000);
        assert!(listing_view(&pages).model.cells.order().len() < 24);
        assert!(pages.ui.view_bounds(order.keys()[500]).is_none());
        pages.listing_mut().scroll.to(4000.0, false, Instant::now());
        pages.layout_center();
        assert_eq!(listing_view(&pages).revision, revision);
        assert_eq!(listing_view(&pages).model.order.get(), order);
        assert!(
            listing_view(&pages)
                .model
                .cells
                .get(order.keys()[0])
                .is_none()
        );
        assert!(listing_view(&pages).model.cells.order().len() < 24);
        let listing = pages.listing();
        let scroll = pages.ui.bounds("list:scroll").unwrap();
        assert!(scroll.x >= listing.viewport.x + listing.viewport.width);
        pages.layout_center();
        let report = pages.ui.layout_report();
        assert_eq!(report["composition"]["view"]["bodies"], 0);
        assert_eq!(report["composition"]["layout_computations"], 0);
        check_replay(&pages, "virtual grid");
    }
}

fn texts(pages: &Pages) -> Vec<String> {
    pages.ui.layout_report()["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["text"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn mastery_binds_owned_state_cost_favorites_and_only_relevant_loading() {
    let mut pages = pages(1.25, 1);
    pages.set_page(Page::Mastery);
    pages.layout_center();
    assert!(texts(&pages).contains(&"Level 18/30".into()));
    assert!(pages.ui.bounds("list:item:0:component:0").is_none());
    let before = render(&pages);
    pages.preferences.toggle_favorite("ember-prime");
    pages.layout_center();
    assert_ne!(render(&pages), before);
    assert!(
        pages
            .mastery_view
            .model
            .cells
            .get(pages.mastery_view.model.order.get().keys()[0])
            .unwrap()
            .favorite
    );
    pages.mastery.mode = "platinum".into();
    pages.mastery.items[0].owned = false;
    pages.mastery.items[0].components[0].blueprint_owned = 0;
    pages.mastery.items[0].relic_probability = 0.0247;
    pages.mastery.reorder(&pages.quotes, true);
    pages.layout_center();
    assert!(texts(&pages).contains(&"2.5%".into()));
    assert!(texts(&pages).contains(&"15".into()));
    assert!(!texts(&pages).contains(&"Level 18/30".into()));
    assert!(pages.ui.bounds("list:item:0:component:0").is_some());
    pages.quote_pending.insert("unrelated".into());
    pages.layout_center();
    assert!(pages.ui.bounds("mastery.prices.loading").is_none());
    pages
        .quote_pending
        .insert("Ember Prime Neuroptics Blueprint".into());
    pages.layout_center();
    assert!(pages.ui.bounds("mastery.prices.loading").is_some());
    assert_ne!(pages.ui.animation_damage(), Damage::None);
    pages
        .quotes
        .get_mut("Ember Prime Neuroptics Blueprint")
        .unwrap()
        .sell = Some(42.0);
    pages
        .quote_pending
        .remove("Ember Prime Neuroptics Blueprint");
    pages.layout_center();
    assert!(texts(&pages).contains(&"42".into()));
    assert!(pages.ui.bounds("mastery.prices.loading").is_none());
    assert_eq!(pages.ui.animation_damage(), Damage::None);
    check_replay(&pages, "unowned mastery");
}

#[test]
fn relic_refinements_reward_highlights_and_context_menus_are_live_bindings() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages(scale, 1);
        pages.set_page(Page::Relics);
        let template = pages.relics.items[0].refinements[0].clone();
        for name in ["Exceptional", "Flawless"] {
            let mut refinement = template.clone();
            refinement.refinement = name.into();
            refinement.amount_owned = 0;
            pages.relics.items[0].refinements.push(refinement);
        }
        pages.relics.owned = false;
        pages.relics.reorder(&pages.quotes, true);
        pages.layout_center();
        assert!(texts(&pages).contains(&"0x Flawless".into()));
        pages.relics.owned = true;
        pages.layout_center();
        assert!(!texts(&pages).contains(&"0x Flawless".into()));
        assert!(texts(&pages).contains(&"1x Radiant".into()));
        let before = render(&pages);
        pages.relics.search = "Ember".into();
        pages.layout_center();
        assert_ne!(render(&pages), before);
        let button = pages
            .ui
            .buttons
            .iter()
            .find(|b| b.key == "list:item:0:reward:0")
            .unwrap();
        assert!(button.enabled);
        assert!(
            !pages
                .ui
                .buttons
                .iter()
                .find(|b| b.key == "list:item:0:reward:1")
                .unwrap()
                .enabled
        );
        check_replay(&pages, "reward search");
        pages.relics.menu = Some((0, Some(0)));
        pages.relics.menu_anchor = (40.0, 80.0);
        pages.layout_center();
        assert!(pages.ui.bounds("list:menu:market").is_some());
        check_replay(&pages, "reward menu");
    }
}

#[test]
fn summary_and_category_geometry_wraps_without_clipping_at_small_sizes() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages(scale, 1);
        pages.configure(
            ((400.0 * scale) as u32, (1200.0 * scale) as u32),
            (400, 1200),
            scale,
        );
        pages.set_page(Page::Mastery);
        pages.layout_center();
        let a = pages.ui.bounds("mastery.summary.game").unwrap();
        let c = pages.ui.bounds("mastery.summary.intrinsics").unwrap();
        assert!(c.y > a.y);
        let search = pages.ui.bounds("list:search").unwrap();
        let first_tab = pages.ui.bounds("list:category:all").unwrap();
        assert!(search.y > first_tab.y);
        let grid = pages.ui.bounds("page.grid").unwrap();
        assert!(
            grid.y > search.y + search.height,
            "scale={scale} grid={grid:?} search={search:?} metrics={:?}",
            (
                pages.mastery_view.model.metrics.get().grid_y(),
                pages.mastery_view.model.metrics.get().viewport()
            )
        );
        assert!(texts(&pages).contains(&"360/700".into()));
        check_replay(&pages, "wrapped summary");
        pages.capture_state("filters", "", "", None).unwrap();
        assert!(pages.ui.bounds("list:filter:prime:yes").is_some());
        assert!(pages.ui.bounds("list:filter:duplicates:yes").is_none());
    }
}
