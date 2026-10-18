use super::*;
use serde_json::json;

fn hover() -> DucatHover {
    DucatHover {
        title: "AKJAGARA PRIME BARREL".into(),
        ducats: 100,
        viewport: [2560, 1440],
        panel: [934.4, 264.0, 1494.4, 516.8],
        price_label: [958.4, 344.0, 1078.4, 386.88],
        scale: 1.6,
    }
}

fn item(name: &str, slug: Option<&str>) -> Value {
    json!({"name":name, "ducats":100, "slug":slug, "quote":null,
        "parent":{"name":"Akjagara Prime", "owned":true, "mastered":true, "subsumed":false}})
}

fn quoted(slug: &str) -> Value {
    json!({"data":{"quotes":[{"slug":slug, "error":null, "quote":{"lowest_sell":8,
        "quoted_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64}}]}})
}

fn catalog(pricing: &mut Pricing, items: Value, now: Instant) {
    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.kiosk(true, now);
    pricing.tick(&outbound, now);
    let daemon::Outbound::DucatInventory { reply } = receiver.try_recv().unwrap() else {
        panic!()
    };
    reply.send(Ok(json!({"data":{"items":items}})));
    assert_eq!(
        pricing.tick(&outbound, now),
        Some(("ducat_inventory", Ok(())))
    );
}

#[test]
fn badges_arrive_before_price_and_survive_missing_listing() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    pricing.observe(Some(hover()), now);
    catalog(
        &mut pricing,
        json!([item("Akjagara Prime Barrel", Some("barrel"))]),
        now,
    );
    let scene = pricing.current.as_ref().unwrap();
    assert!(scene.parent.as_ref().unwrap().owned && scene.listed);
    assert!(scene.quote.is_none());
    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.tick(&outbound, now + HOVER_DELAY);
    let daemon::Outbound::MarketQuote { reply, items } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(items, ["barrel"]);
    reply.send(Err("timeout".into()));
    assert!(pricing.tick(&outbound, now).unwrap().1.is_err());
    assert!(
        pricing
            .current
            .as_ref()
            .unwrap()
            .parent
            .as_ref()
            .unwrap()
            .owned
    );
    assert!(pricing.current.as_ref().unwrap().failed);

    pricing.kiosk(false, now);
    catalog(
        &mut pricing,
        json!([item("Akjagara Prime Barrel", None)]),
        now,
    );
    let scene = pricing.current.as_ref().unwrap();
    assert!(scene.parent.as_ref().unwrap().mastered);
    assert!(!scene.listed && !scene.failed);
    pricing.tick(&outbound, now + HOVER_DELAY);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn cached_quotes_and_geometry_are_immediate_and_matching_is_strict() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    let mut cached = item("Akjagara Prime Barrel", Some("barrel"));
    cached["quote"] = quoted("barrel")["data"]["quotes"][0]["quote"].clone();
    catalog(&mut pricing, json!([cached.clone()]), now);
    pricing.observe(Some(hover()), now);
    assert_eq!(
        pricing
            .current
            .as_ref()
            .unwrap()
            .quote
            .as_ref()
            .unwrap()
            .lowest_sell,
        Some(8)
    );
    let mut moved = hover();
    moved.panel[0] += 10.0;
    pricing.observe(Some(moved.clone()), now);
    assert_eq!(pricing.current.as_ref().unwrap().hover.panel, moved.panel);
    assert!(pricing.current.as_ref().unwrap().quote.is_some());
    moved.ducats = 45;
    pricing.observe(Some(moved), now);
    assert!(!pricing.current.as_ref().unwrap().listed);
    pricing.items.push(serde_json::from_value(cached).unwrap());
    pricing.observe(Some(hover()), now);
    assert!(!pricing.current.as_ref().unwrap().listed);
    pricing.observe(None, now);
    assert!(pricing.current.is_none());
}

#[test]
fn stale_cache_is_displayed_while_refresh_replies_must_be_fresh_and_match() {
    let mut reply = quoted("barrel");
    assert!(quote(&reply, "barrel").is_ok());
    assert!(quote(&reply, "other").is_err());
    reply["data"]["quotes"][0]["quote"]["stale"] = true.into();
    assert!(quote(&reply, "barrel").is_err());
    reply["data"]["quotes"][0]["quote"]["stale"] = false.into();
    reply["data"]["quotes"][0]["quote"]["quoted_at"] = 0.into();
    assert!(quote(&reply, "barrel").is_err());
    reply["data"]["quotes"] = json!([]);
    assert!(quote(&reply, "barrel").is_err());
    let mut pricing = Pricing::default();
    let now = Instant::now();
    let mut expired = item("Akjagara Prime Barrel", Some("barrel"));
    expired["quote"] = json!({"lowest_sell":8,"quoted_at":0,"stale":true});
    catalog(&mut pricing, json!([expired]), now);
    pricing.observe(Some(hover()), now);
    assert_eq!(
        pricing
            .current
            .as_ref()
            .unwrap()
            .quote
            .as_ref()
            .unwrap()
            .lowest_sell,
        Some(8)
    );
    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.tick(&outbound, now + HOVER_DELAY);
    let daemon::Outbound::MarketQuote { items, .. } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(items, ["barrel"]);
    assert_eq!(
        pricing
            .current
            .as_ref()
            .unwrap()
            .quote
            .as_ref()
            .unwrap()
            .lowest_sell,
        Some(8)
    );
}

#[test]
fn visible_price_survives_expiry_and_failed_refresh_until_replaced() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    let mut cached = item("Akjagara Prime Barrel", Some("barrel"));
    cached["quote"] = quoted("barrel")["data"]["quotes"][0]["quote"].clone();
    catalog(&mut pricing, json!([cached]), now);
    pricing.observe(Some(hover()), now);
    pricing.items[0].quote.as_mut().unwrap().quoted_at = 0;
    pricing
        .current
        .as_mut()
        .unwrap()
        .quote
        .as_mut()
        .unwrap()
        .quoted_at = 0;
    let shown = pricing.current.clone();
    let now = now + HOVER_DELAY;
    pricing.observe(Some(hover()), now);
    assert_eq!(pricing.current, shown);
    let expires = pricing.expires;

    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.tick(&outbound, now);
    let daemon::Outbound::MarketQuote { reply, .. } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(pricing.current, shown);
    reply.send(Err("timeout".into()));
    assert!(pricing.tick(&outbound, now).unwrap().1.is_err());
    let scene = pricing.current.as_ref().unwrap();
    assert_eq!(scene.quote.as_ref().unwrap().lowest_sell, Some(8));
    assert!(scene.failed && scene.parent.as_ref().unwrap().owned);

    pricing.tick(&outbound, now + RETRY_DELAY);
    let daemon::Outbound::MarketQuote { reply, .. } = receiver.try_recv().unwrap() else {
        panic!()
    };
    let mut updated = quoted("barrel");
    updated["data"]["quotes"][0]["quote"]["lowest_sell"] = 9.into();
    reply.send(Ok(updated));
    assert!(
        pricing
            .tick(&outbound, now + RETRY_DELAY)
            .unwrap()
            .1
            .is_ok()
    );
    let scene = pricing.current.as_ref().unwrap();
    assert_eq!(scene.quote.as_ref().unwrap().lowest_sell, Some(9));
    assert!(!scene.failed);
    assert_eq!(pricing.expires, expires);
}

#[test]
fn retained_price_never_crosses_hover_or_market_identity() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    let mut cached = item("Akjagara Prime Barrel", Some("barrel"));
    cached["quote"] = quoted("barrel")["data"]["quotes"][0]["quote"].clone();
    catalog(
        &mut pricing,
        json!([cached, item("Other", Some("other"))]),
        now,
    );
    pricing.observe(Some(hover()), now);
    pricing.items[0].quote.as_mut().unwrap().quoted_at = 0;
    pricing.observe(Some(hover()), now);
    assert!(pricing.current.as_ref().unwrap().quote.is_some());

    pricing.inventory_due = Some(now);
    catalog(
        &mut pricing,
        json!([
            item("Akjagara Prime Barrel", Some("replacement")),
            item("Other", Some("other"))
        ]),
        now,
    );
    assert!(pricing.current.as_ref().unwrap().quote.is_none());
    pricing.items[0].quote =
        serde_json::from_value(quoted("replacement")["data"]["quotes"][0]["quote"].clone())
            .unwrap();
    pricing.observe(Some(hover()), now);
    assert!(pricing.current.as_ref().unwrap().quote.is_some());
    let mut other = hover();
    other.title = "OTHER".into();
    pricing.observe(Some(other), now);
    assert!(pricing.current.as_ref().unwrap().quote.is_none());
}

#[test]
fn metadata_refresh_does_not_replace_newer_quote_with_an_old_snapshot() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    let mut cached = item("Akjagara Prime Barrel", Some("barrel"));
    cached["quote"] = quoted("barrel")["data"]["quotes"][0]["quote"].clone();
    catalog(&mut pricing, json!([cached.clone()]), now);
    pricing.observe(Some(hover()), now);
    let shown = pricing.current.clone();

    cached["quote"] = json!({"lowest_sell":7, "quoted_at":0});
    pricing.inventory_due = Some(now);
    catalog(&mut pricing, json!([cached.clone()]), now);
    assert_eq!(pricing.current, shown);
    cached["quote"] = Value::Null;
    pricing.inventory_due = Some(now);
    catalog(&mut pricing, json!([cached]), now);
    assert_eq!(pricing.current, shown);
}

#[test]
fn failed_memory_read_preserves_scene_only_until_original_lease_expires() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    pricing.observe(Some(hover()), now);
    let shown = pricing.current.clone();
    let expires = pricing.expires;
    for elapsed in [ACTIVE_INTERVAL, ACTIVE_INTERVAL * 2] {
        pricing.read_failed(now + elapsed);
        assert_eq!(pricing.current, shown);
        assert_eq!(pricing.expires, expires);
    }
    pricing.read_failed(now + LEASE);
    assert!(pricing.current.is_none() && pricing.expires.is_none());
    pricing.observe(Some(hover()), now + LEASE);
    pricing.observe(None, now + LEASE);
    assert!(pricing.current.is_none() && pricing.expires.is_none());
}

#[test]
fn native_main_blueprint_status_is_not_duplicated() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    catalog(
        &mut pricing,
        json!([item("Akjagara Prime Blueprint", Some("blueprint"))]),
        now,
    );
    let mut blueprint = hover();
    blueprint.title = "AKJAGARA PRIME BLUEPRINT".into();
    pricing.observe(Some(blueprint), now);
    assert!(pricing.current.as_ref().unwrap().listed);
    assert!(pricing.current.as_ref().unwrap().parent.is_none());
}

#[test]
fn newest_hover_precedes_warm_queue_and_old_reply_still_populates_cache() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    catalog(
        &mut pricing,
        json!([
            item("Other", Some("other")),
            item("Akjagara Prime Barrel", Some("barrel")),
            item("Newest", Some("newest"))
        ]),
        now,
    );
    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.observe(Some(hover()), now);
    pricing.tick(&outbound, now + HOVER_DELAY);
    let daemon::Outbound::MarketQuote { reply, items } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(items, ["barrel"]);
    let mut newest = hover();
    newest.title = "NEWEST".into();
    for _ in 0..20 {
        pricing.observe(Some(newest.clone()), now + HOVER_DELAY);
        pricing.tick(&outbound, now + HOVER_DELAY * 2);
        assert!(receiver.try_recv().is_err());
    }
    reply.send(Ok(quoted("barrel")));
    assert_eq!(
        pricing.tick(&outbound, now + HOVER_DELAY * 2),
        Some(("ducat_market", Ok(())))
    );
    assert!(pricing.current.as_ref().unwrap().quote.is_none());
    assert!(
        pricing
            .items
            .iter()
            .find(|i| i.slug.as_deref() == Some("barrel"))
            .unwrap()
            .quote
            .is_some()
    );
    pricing.tick(&outbound, now + HOVER_DELAY * 2);
    let daemon::Outbound::MarketQuote { items, .. } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(items, ["newest"]);
}

#[test]
fn prefetch_starts_without_hover_and_closing_stops_new_work_not_accepted_work() {
    let mut pricing = Pricing::default();
    let now = Instant::now();
    catalog(
        &mut pricing,
        json!([
            item("Akjagara Prime Barrel", Some("barrel")),
            item("Other", Some("other"))
        ]),
        now,
    );
    let (outbound, mut receiver) = daemon::outbound_channel();
    pricing.tick(&outbound, now);
    let daemon::Outbound::MarketQuote { reply, items } = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(items, ["barrel"]);
    pricing.kiosk(false, now);
    reply.send(Ok(quoted("barrel")));
    assert_eq!(pricing.tick(&outbound, now), Some(("ducat_market", Ok(()))));
    assert!(pricing.items[0].quote.is_some());
    pricing.tick(&outbound, now);
    assert!(receiver.try_recv().is_err());
}
