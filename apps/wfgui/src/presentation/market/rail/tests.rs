use super::*;

fn pages(scale: f32) -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    pages.rail_tab = true;
    pages.market.item = "test-item".into();
    pages.market.query = "Test item".into();
    pages.market.description = json!({"name":"Test item","max_rank":10,
        "subtypes":["Intact","Radiant"],"max_charges":5,"max_amber_stars":2,"bulk_tradable":true});
    pages.market.account = json!({"authenticated":true});
    pages.market.quote = json!({"sell_orders":(0..50).map(|i|json!({
        "id":format!("order-{i}"),"quantity":i+1,"platinum":i+10,
        "user":{"ingame_name":format!("Player {i}"),"status":"ingame"}
    })).collect::<Vec<_>>()});
    pages.configure(
        (1000, 900),
        ((400.0 * scale) as u32, (900.0 * scale) as u32),
        scale,
    );
    pages
}

fn render(pages: &Pages) -> Vec<u8> {
    let (w, h) = pages.rail.size;
    let mut pixels = vec![0; (w * h * 4) as usize];
    let mut p = Painter::new(&mut pixels, w, h).unwrap();
    pages.rail.paint(&mut p, &pages.media);
    p.finish().unwrap();
    pixels
}

#[test]
fn declared_market_filters_and_posting_preserve_geometry_and_input() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = pages(scale);
        for (key, x, y) in [
            ("market:rank", 73.0, 168.0),
            ("market:subtype", 12.0, 205.0),
            ("market:variant:charges", 122.0, 242.0),
            ("market:variant:amberStars", 122.0, 279.0),
            ("market:per-trade", 122.0, 316.0),
            ("market:price", 12.0, 817.0),
            ("market:quantity", 116.0, 817.0),
        ] {
            let r = pages.rail.bounds(key).unwrap();
            assert_eq!(
                (r.x, r.y),
                (pages.rail.px(x), pages.rail.px(y)),
                "{key} at {scale}"
            );
        }
        let track = pages.rail.bounds("rail:scroll").unwrap();
        assert!(track.y >= pages.rail.px(382.0));
        let r = pages.rail.bounds("number:market:price:up").unwrap();
        pages
            .rail
            .pointer_move(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64));
        pages.rail.pointer_button(true);
        pages.rail.pointer_button(false);
        let signal = pages.rail.signals.pop().unwrap();
        assert_eq!(signal.key, "market:price");
        assert_eq!(signal.value.as_deref(), Some("2"));
        let cached = render(&pages);
        pages
            .rail
            .with_full_replay(|| assert_eq!(render(&pages), cached));
        pages.market.account = json!({"authenticated":false});
        pages.market.description = json!({"name":"Plain item"});
        pages.layout_rail();
        for key in [
            "market:rank",
            "market:price",
            "market:post",
            "market:subtype",
            "market:variant:charges",
        ] {
            assert!(pages.rail.bounds(key).is_none(), "stale {key}");
        }
        assert!(pages.rail.bounds("market:website").is_some());
    }
}

#[test]
fn market_rows_keep_identity_across_quotes_and_reorder_but_only_bind_visible_rows() {
    let mut pages = pages(1.25);
    let order = pages.market_rail_view.orders.order();
    let first = order.keys()[0];
    let currency = Key::new("market.order.currency", first.value);
    assert!(pages.rail.view_bounds(currency).is_some());
    assert!(pages.rail.bounds("market:whisper:0").is_some());
    assert!(pages.rail.bounds("market:whisper:49").is_none());
    let before = render(&pages);
    for _ in 0..10 {
        render(&pages);
    }
    assert!(
        pages.rail.paint_stats().blits > 0,
        "listing viewport should remain eligible for raster reuse"
    );
    pages.market.quote["sell_orders"][0]["platinum"] = json!(999);
    pages.layout_rail();
    assert_eq!(pages.market_rail_view.orders.order().keys()[0], first);
    assert_ne!(render(&pages), before);
    pages.market.quote["sell_orders"]
        .as_array_mut()
        .unwrap()
        .reverse();
    pages.layout_rail();
    assert_eq!(pages.market_rail_view.orders.order().keys()[49], first);
    assert!(pages.rail.view_bounds(currency).is_none());
    let now = Instant::now();
    pages
        .rail_scroll
        .to(pages.rail_scroll.maximum(), false, now);
    pages.layout_rail_at(FrameTime::now());
    assert!(pages.rail.bounds("market:whisper:0").is_none());
    let last = pages.rail.bounds("market:whisper:49").unwrap();
    let viewport = pages.rail.view_bounds(VIEWPORT).unwrap();
    assert!(last.y >= viewport.y && last.y + last.height <= viewport.y + viewport.height);
    pages
        .rail
        .pointer_move(((last.x + 10) as f64, (last.y + last.height / 2) as f64));
    pages.rail.pointer_button(true);
    pages.rail.pointer_button(false);
    assert_eq!(pages.rail.signals.pop().unwrap().key, "market:whisper:49");
    let cached = render(&pages);
    pages
        .rail
        .with_full_replay(|| assert_eq!(render(&pages), cached));
    pages.rail_tab = false;
    pages.layout_rail();
    assert!(pages.rail.view_bounds(currency).is_none());
    assert!(pages.rail.bounds("market:query").is_none());
    assert!(pages.rail.bounds("market:price").is_none());
    assert!(
        pages.rail.layout_report()["composition"]["declarative"]
            .as_bool()
            .unwrap()
    );
    pages.rail_tab = true;
    pages.layout_rail();
    assert_eq!(pages.market_rail_view.orders.order().keys()[49], first);
}

#[test]
fn late_market_icons_repaint_only_visible_requests_and_drop_on_tab_switch() {
    use retinue_render::{image, resources::ResourceCache};
    let mut pages = pages(1.25);
    let before = render(&pages);
    let currency = ImageRequest::embedded(
        crate::domain::media::icon("platinum"),
        pages.rail.px(16.0),
        None,
    );
    let wanted = pages.rail.requests(&pages.media);
    assert!(wanted.iter().any(|request| request.key == currency.key));
    let mut cache = ResourceCache::new(8192);
    let bitmap = cache.insert(
        currency.key,
        RasterImage::from_rgba(image::RgbaImage::from_pixel(
            20,
            20,
            image::Rgba([230, 70, 40, 255]),
        ))
        .unwrap(),
    );
    pages.media.accept(&[(currency.key, bitmap)], &wanted);
    let damage = pages
        .rail
        .image_damage(&HashSet::from([currency.key]), &pages.media)
        .bounds(pages.rail.size)
        .unwrap();
    let viewport = pages.rail.view_bounds(VIEWPORT).unwrap();
    assert!(damage.y >= viewport.y && damage.y + damage.height <= viewport.y + viewport.height);
    assert_ne!(render(&pages), before);
    assert_eq!(
        pages.rail.layout_report()["composition"]["layout_computations"],
        0
    );
    pages.rail_tab = false;
    pages.layout_rail();
    assert!(
        !pages
            .rail
            .requests(&pages.media)
            .iter()
            .any(|request| request.key == currency.key)
    );
}

#[test]
fn market_suggestions_paint_and_hit_above_item_and_filters() {
    let mut pages = pages(1.0);
    pages.market.matches = json!({"resolutions":[{"matches":(0..5).map(|i| json!({"itemId":format!("item-{i}"),"name":format!("Match {i}")})).collect::<Vec<_>>()}]});
    pages.market.suggestions = true;
    pages.market.suggestion = Some(3);
    pages.layout_rail();
    let r = pages.rail.bounds("market:match:3").unwrap();
    pages
        .rail
        .pointer_move(((r.x + 80) as f64, (r.y + r.height / 2) as f64));
    pages.rail.pointer_button(true);
    pages.rail.pointer_button(false);
    assert_eq!(pages.rail.signals.pop().unwrap().key, "market:match:3");
    let cached = render(&pages);
    pages
        .rail
        .with_full_replay(|| assert_eq!(render(&pages), cached));
    pages.market.suggestions = false;
    pages.layout_rail();
    assert!(pages.rail.bounds("market:match:3").is_none());
    assert!(pages.rail.bounds("market:rank").is_some());
}
