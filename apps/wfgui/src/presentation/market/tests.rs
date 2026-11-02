use super::*;

fn pages(scale: f32, count: usize) -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    let mut account = pages.market.account.clone();
    let template = account["orders"][0].clone();
    account["orders"] = (0..count)
        .map(|i| {
            let mut order = template.clone();
            order["id"] = json!(format!("order-{i:05}"));
            order["platinum"] = json!(i + 1);
            order
        })
        .collect();
    pages.market_reply("market", account);
    pages.set_page(Page::Market);
    pages.configure(
        ((1000.0 * scale) as u32, (700.0 * scale) as u32),
        (400, 700),
        scale,
    );
    pages.pending.clear();
    pages.requests.clear();
    pages.fixture = false;
    pages.layout_center();
    pages
}

fn signal(pages: &mut Pages, key: &str, value: &str) {
    pages.market_signal(Signal {
        key: key.into(),
        value: Some(value.into()),
    });
    pages.layout_center();
}

fn render(pages: &Pages) -> Vec<u8> {
    let (w, h) = pages.ui.size;
    let mut pixels = vec![0; (w * h * 4) as usize];
    let mut p = Painter::new(&mut pixels, w, h).unwrap();
    pages.ui.paint(&mut p, &pages.media);
    p.finish().unwrap();
    pixels
}

fn replay(pages: &Pages) {
    let cached = render(pages);
    pages.ui.with_full_replay(|| {
        let full = render(pages);
        let changed = cached.iter().zip(&full).filter(|(a, b)| a != b).count();
        assert_eq!(changed, 0, "cached market painting differs");
    });
}

#[test]
fn orders_mount_only_visible_rows_and_keep_identity_across_quotes_sort_and_scroll() {
    let mut pages = pages(1.25, 10_000);
    let order = pages.market_view.model.order.get();
    let first = order.keys()[0];
    assert_eq!(order.len(), 10_000);
    assert!(pages.market_view.model.cells.order().len() < 20);
    assert!(pages.ui.view_bounds(first).is_some());
    assert!(pages.ui.view_bounds(order.keys()[500]).is_none());
    let before = render(&pages);
    pages.market_reply(
        "market:comparison:order-00000",
        json!({"quotes":[{"quote":{"lowest_sell":123}}]}),
    );
    pages.layout_center();
    assert_ne!(before, render(&pages));
    assert_eq!(pages.market_view.model.order.get(), order);
    pages.layout_center();
    let report = pages.ui.layout_report();
    assert_eq!(report["composition"]["view"]["bodies"], 0);
    assert_eq!(report["composition"]["layout_computations"], 0);
    signal(&mut pages, "market:scroll", "1800");
    assert!(pages.market_view.model.cells.get(first).is_none());
    assert!(pages.ui.view_bounds(first).is_none());
    assert!(pages.market_view.model.cells.order().len() < 20);
    assert_eq!(pages.market_view.model.order.get(), order);
    replay(&pages);
    signal(&mut pages, "market:sort", "price");
    signal(&mut pages, "market:direction", "");
    assert_eq!(pages.market_view.model.order.get().keys()[9999], first);
    signal(&mut pages, "market:search", "nonexistent");
    assert!(pages.market.filtered.is_empty());
    assert!(pages.market_view.model.cells.order().is_empty());
    assert!(pages.ui.bounds("market:order:order-00000:edit").is_none());
    assert_eq!(pages.market.offset, 0);
}

#[test]
fn late_catalog_and_inventory_refresh_rebuild_filters_and_owned_status() {
    let mut pages = pages(1.0, 2);
    let mut account = pages.market.account.clone();
    account["orders"][1]["itemId"] = json!("late-item");
    account["orders"][1]["rank"] = json!(1);
    pages.market_reply("market", account);
    signal(&mut pages, "market:search", "Late name");
    assert!(pages.market.filtered.is_empty());
    pages.market_reply(
        "market:catalog:late-item",
        json!({"items":[{"id":"late-item","name":"Late name","group":"mods"}]}),
    );
    pages.layout_center();
    assert_eq!(pages.market.filtered, ["order-00001"]);
    assert_eq!(pages.market_view.orders[0].owned, Some(0.0));
    let mut row = pages.inventory.items[0].clone();
    row.market_name = Some("Late name".into());
    row.quantity = 7;
    row.rank = 1;
    pages.inventory.items.push(row.clone());
    row.quantity = 100;
    row.rank = 0;
    pages.inventory.items.push(row);
    pages.resort(true);
    pages.layout_center();
    assert_eq!(pages.market_view.orders[0].owned, Some(7.0));
    signal(&mut pages, "market:stock", "missing");
    assert!(pages.market.filtered.is_empty());
    signal(&mut pages, "market:stock", "available");
    assert_eq!(pages.market.filtered, ["order-00001"]);
    pages.inventory.ready = false;
    pages.layout_center();
    assert_eq!(pages.market_view.orders[0].owned, None);
}

#[test]
fn account_loading_login_errors_and_authenticated_controls_are_exclusive() {
    let mut pages = pages(1.0, 1);
    pages.market_reply("market", Value::Null);
    pages.layout_center();
    assert!(pages.ui.bounds("market:search").is_none());
    assert!(pages.ui.bounds("market:email").is_none());
    assert!(
        pages
            .ui
            .view_bounds(Key::new("market.account.progress", 0))
            .is_some()
    );
    pages.market_error("market", "Offline".into());
    pages.layout_center();
    assert!(
        pages
            .ui
            .view_bounds(Key::new("market.account.progress", 0))
            .is_none()
    );
    assert!(pages.ui.bounds("market:refresh").is_some());
    pages.market_reply("market", json!({"authenticated":false}));
    pages.layout_center();
    assert!(pages.ui.bounds("market:email").is_some());
    assert!(pages.ui.bounds("market:search").is_none());
    signal(&mut pages, "market:email", "account@example.invalid");
    signal(&mut pages, "market:password", "test-password");
    signal(&mut pages, "market:login", "");
    assert_eq!(
        pages.pending["market:write:login"]["email"],
        "account@example.invalid"
    );
    let count = pages.requests.len();
    signal(&mut pages, "market:login", "");
    assert_eq!(pages.requests.len(), count);
    pages.market_reply("market:write:login", json!({}));
    assert!(pages.market.password.is_empty());
    pages.market_reply("market", json!({"authenticated":true,"orders":[]}));
    pages.layout_center();
    assert!(pages.ui.bounds("market:password").is_none());
    assert!(pages.ui.bounds("market:search").is_some());
}

#[test]
fn edit_and_confirmation_dialogs_preserve_variants_input_isolation_and_scale() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages(scale, 1);
        let mut account = pages.market.account.clone();
        account["orders"][0]["rank"] = json!(3);
        account["orders"][0]["subtype"] = json!("intact");
        account["orders"][0]["perTrade"] = json!(3);
        account["orders"][0]["quantity"] = json!(7);
        pages.market_reply("market", account);
        pages.market_reply("market:catalog:ember-neuroptics", json!({"items":[{"id":"ember-neuroptics","name":"Variant","max_rank":10,"subtypes":["intact","radiant"],"bulk_tradable":true}]}));
        signal(&mut pages, "market:order:order-00000:edit", "");
        assert!(pages.ui.has_modal());
        assert!(pages.ui.bounds("market:edit:rank").is_some());
        assert!(pages.ui.bounds("market:edit:subtype").is_some());
        replay(&pages);
        let behind = pages.ui.bounds("market:logout").unwrap();
        pages
            .ui
            .pointer_move(((behind.x + 3) as f64, (behind.y + 3) as f64));
        pages.ui.pointer_button(true);
        pages.ui.pointer_button(false);
        assert!(pages.ui.signals.is_empty());
        signal(&mut pages, "market:edit:rank", "11");
        signal(&mut pages, "market:confirm", "");
        assert!(pages.market.error.is_some());
        assert!(!pages.pending.contains_key("market:write:edit"));
        signal(&mut pages, "market:edit:rank", "5");
        signal(&mut pages, "market:edit:subtype", "radiant");
        signal(&mut pages, "market:confirm", "");
        let patch = &pages.pending["market:write:edit"]["patch"];
        assert_eq!(patch["rank"], 5);
        assert_eq!(patch["quantity"], 6);
        assert_eq!(patch["subtype"], "radiant");
        pages.pending.clear();
        signal(&mut pages, "market:cancel", "");
        assert!(!pages.ui.has_modal());
        assert!(pages.ui.bounds("market:edit:rank").is_none());
        signal(&mut pages, "market:order:order-00000:delete", "");
        assert!(pages.ui.bounds("market:edit:rank").is_none());
        assert!(pages.ui.bounds("market:confirm").is_some());
        replay(&pages);
        signal(&mut pages, "market:cancel", "");
        assert!(!pages.pending.contains_key("market:write:delete"));
    }
}

#[test]
fn page_switches_preserve_market_state_without_retaining_hidden_controls() {
    let mut pages = pages(1.0, 3);
    signal(&mut pages, "market:order:order-00000:select", "");
    signal(&mut pages, "market:sort", "price");
    signal(&mut pages, "market:direction", "");
    let order = pages.market_view.model.order.get();
    for page in [Page::Inventory, Page::Mastery, Page::Relics, Page::Builds] {
        pages.set_page(page);
        pages.layout_center();
        assert!(pages.ui.bounds("market:search").is_none());
        pages.set_page(Page::Market);
        pages.layout_center();
        assert_eq!(pages.market_view.model.order.get(), order);
        assert!(pages.market.selected.contains("order-00000"));
    }
    signal(&mut pages, "market:bulk", "delete");
    assert_eq!(
        pages.market.dialog.as_ref().unwrap()["operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !pages
            .requests
            .iter()
            .any(|r| r.key.starts_with("market:write"))
    );
}

#[test]
fn visible_quote_requests_keep_variants_and_narrow_toolbar_stays_above_grid() {
    let mut pages = pages(1.0, 80);
    let visible = pages.market_view.model.cells.order().len();
    assert_eq!(pages.requests.len(), visible);
    pages.layout_center();
    assert_eq!(pages.requests.len(), visible);
    let mut account = pages.market.account.clone();
    account["orders"][0]["rank"] = json!(5);
    account["orders"][0]["charges"] = json!(2);
    account["orders"][0]["subtype"] = json!("radiant");
    pages.market_reply("market", account);
    pages.pending.clear();
    pages.requests.clear();
    pages.configure((600, 700), (400, 700), 1.0);
    pages.layout_center();
    let quote = &pages.pending["market:comparison:order-00000"];
    assert_eq!(quote["op"], "market_quote_variant");
    assert_eq!(
        quote["filters"],
        json!({"rank":5,"charges":2,"subtype":"radiant"})
    );
    let bulk = pages.ui.bounds("market:bulk").unwrap();
    let category = pages.ui.bounds("market:category").unwrap();
    let grid = pages.ui.view_bounds(GRID).unwrap();
    assert_eq!(bulk.y, 170);
    assert_eq!(category.y, 130);
    assert!(bulk.y + bulk.height < grid.y);
    replay(&pages);
}
