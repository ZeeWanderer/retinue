use super::*;
use crate::{Bounds, Color, Command, PaintProgram, painter::Painter};

fn recipe(key: u64) -> Recipe {
    Recipe::item(
        &PaintProgram::new(
            key,
            (16.0, 16.0),
            1.0,
            Rc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 16.0, 16.0),
                radius: 0.0,
                color: Color::rgb(0x225577),
            }]),
        )
        .unwrap(),
    )
}

fn value(recipe: Recipe) -> Materialization {
    let image = RasterImage::render(recipe.size.0, recipe.size.1, |p| {
        for i in 0..recipe.fragments.len() {
            recipe.paint_fragment(i, p, recipe.canvas());
        }
    })
    .unwrap();
    Materialization {
        image: Rc::new(image),
        recipe,
    }
}

#[test]
fn host_expiry_releases_idle_pixels_and_reacquisition_replaces_the_deadline() {
    let host = RenderHost::new(HostPolicy::desktop());
    let policy = ResidencyPolicy {
        warm_lifetime: Duration::from_millis(100),
        ..ResidencyPolicy::desktop(65536)
    };
    let mut a = Residency::with_host(policy, host.clone());
    let mut b = Residency::with_host(policy, host.clone());
    let ra = recipe(1);
    let rb = recipe(2);
    let oa = Owner::Item(ra.fragments[0].program.id());
    let ob = Owner::Item(rb.fragments[0].program.id());
    let ia = a.insert(oa, value(ra.clone()), 500);
    let ib = b.insert(ob, value(rb.clone()), 500);
    a.drawn(ia);
    b.drawn(ib);
    let start = Instant::now();
    let first = start + policy.warm_lifetime;
    let second = first + Duration::from_millis(50);
    a.release(ia, Retention::Nearby, start);
    b.release(ib, Retention::Nearby, start + Duration::from_millis(50));
    assert_eq!(host.maintenance_deadline(), Some(first));
    host.maintain(first - Duration::from_nanos(1));
    assert_eq!(host.stats().warm_bytes, 2048);
    host.maintain(first);
    assert_eq!(host.stats().warm_bytes, 1024);
    assert_eq!(host.stats().expired_entries, 1);
    assert_eq!(host.maintenance_deadline(), Some(second));
    assert_eq!(
        a.stats().live_bytes,
        0,
        "idle store retained expired pixels"
    );
    assert_eq!(a.acquire(oa, &ra), None);
    assert_eq!(b.acquire(ob, &rb), Some(ib));
    assert_eq!(host.maintenance_deadline(), None);
    b.release(ib, Retention::Nearby, first);
    let renewed = first + policy.warm_lifetime;
    assert_eq!(host.maintenance_deadline(), Some(renewed));
    host.maintain(second);
    assert_eq!(
        host.stats().warm_bytes,
        1024,
        "old deadline expired renewed retention"
    );
    drop(b);
    assert_eq!(host.maintenance_deadline(), None);
    assert_eq!(host.stats().live_bytes, 0);
    a.maintain(renewed);
    assert!(a.entries.is_empty());
}

#[test]
fn expired_retention_does_not_reactivate_an_allocation_held_by_a_sibling() {
    let host = RenderHost::new(HostPolicy::desktop());
    let mut a = Residency::with_host(ResidencyPolicy::desktop(65536), host.clone());
    let mut b = Residency::with_host(ResidencyPolicy::desktop(65536), host.clone());
    let recipe = recipe(1);
    let owner = Owner::Item(recipe.fragments[0].program.id());
    let pixels = value(recipe.clone());
    let ia = a.insert(owner, pixels.clone(), 500);
    let ib = b.insert(owner, pixels, 500);
    a.drawn(ia);
    a.release(ia, Retention::Nearby, Instant::now());
    host.maintain(host.maintenance_deadline().unwrap());
    assert_eq!(host.stats().selected_bytes, 1024);
    assert_eq!(host.stats().warm_bytes, 0);
    assert_eq!(a.stats().warm_bytes, 0);
    assert_eq!(a.acquire(owner, &recipe), None);
    assert_eq!(b.acquire(owner, &recipe), Some(ib));
    a.clear();
    assert_eq!(host.stats().selected_bytes, 1024);
    drop(b);
    assert_eq!(host.stats().live_bytes, 0);
}

#[test]
fn expiry_drops_only_retention_not_queued_native_readers() {
    let host = RenderHost::new(HostPolicy {
        peak_bytes: 1024,
        ..HostPolicy::desktop()
    });
    let mut store = Residency::with_host(ResidencyPolicy::desktop(65536), host.clone());
    let source = recipe(1);
    let id = store.insert(
        Owner::Item(source.fragments[0].program.id()),
        value(source.clone()),
        500,
    );
    let mut pixels = vec![0; 32 * 32 * 4];
    let mut painter =
        Painter::with_threads(&mut pixels, 32, 32, std::num::NonZeroU32::new(2).unwrap()).unwrap();
    painter.draw_image(store.image(id), 0, 0, 16, 16);
    store.drawn(id);
    store.release(id, Retention::Nearby, Instant::now());
    host.maintain(host.maintenance_deadline().unwrap());
    assert_eq!(host.maintenance_deadline(), None);
    assert_eq!(host.stats().retired_bytes, 1024);
    assert_eq!(store.stats().retired_bytes, 1024);
    assert!(store.reserve(&source).is_none());
    painter.finish().unwrap();
    host.maintain(Instant::now());
    assert_eq!(host.stats().live_bytes, 0);
    assert!(store.reserve(&source).is_some());
}

#[test]
fn host_peak_and_native_lifetimes_span_independent_stores() {
    let host = RenderHost::new(HostPolicy {
        peak_bytes: 2048,
        ..HostPolicy::desktop()
    });
    let mut a = Residency::with_host(ResidencyPolicy::desktop(65536), host.clone());
    let mut b = Residency::with_host(ResidencyPolicy::desktop(65536), host.clone());
    let source = recipe(1);
    let owner = Owner::Item(source.fragments[0].program.id());
    let reserve_a = a.reserve(&source).unwrap();
    let reserve_b = b.reserve(&source).unwrap();
    assert_eq!(host.stats().pending_bytes, 2048);
    assert!(b.reserve(&source).is_none());
    drop(reserve_b);
    let materialized = value(source.clone());
    drop(reserve_a);
    let id_a = a.insert(owner, materialized.clone(), 500);
    b.insert(owner, materialized, 500);
    assert_eq!(
        host.stats().selected_bytes,
        1024,
        "same allocation counted twice"
    );
    a.drawn(id_a);
    let image = a.image(id_a).clone();
    a.release(id_a, Retention::Nearby, Instant::now());
    assert_eq!(host.stats().selected_bytes, 1024, "sibling still selected");
    b.clear();
    assert_eq!(host.stats().warm_bytes, 1024);
    let mut pixels = vec![0; 32 * 32 * 4];
    let mut painter =
        Painter::with_threads(&mut pixels, 32, 32, std::num::NonZeroU32::new(2).unwrap()).unwrap();
    painter.draw_image(&image, 0, 0, 16, 16);
    drop(image);
    drop(a);
    assert_eq!(host.stats().retired_bytes, 1024);
    let reserve = b.reserve(&source).unwrap();
    assert!(
        b.reserve(&source).is_none(),
        "in-flight pixels must still count"
    );
    painter.finish().unwrap();
    assert_eq!(host.stats().live_bytes, 0);
    assert!(b.reserve(&source).is_some());
    drop(reserve);
    drop(b);
    assert_eq!(host.stats().pending_bytes, 0);
}

#[test]
fn selected_consumers_ignore_warm_limits_and_share_allocations() {
    let mut residency = Residency::new(ResidencyPolicy::desktop(0));
    let recipe = recipe(1);
    let owner = Owner::Item(recipe.fragments[0].program.id());
    let id = residency.insert(owner, value(recipe.clone()), 500);
    assert_eq!(residency.acquire(owner, &recipe), Some(id));
    let alias = residency.insert(
        Owner::Group(ScopeId::group(2.into())),
        residency.materialization(id),
        500,
    );
    assert_eq!(residency.stats().selected_bytes, 16 * 16 * 4);
    residency.release(id, Retention::Discard, Instant::now());
    residency.release(alias, Retention::Discard, Instant::now());
    assert_eq!(residency.stats().live_bytes, 16 * 16 * 4);
    residency.release(id, Retention::Nearby, Instant::now());
    residency.maintain(Instant::now());
    assert_eq!(residency.stats().live_bytes, 0);
    assert!(residency.entries.is_empty());
    assert!(residency.allocations.is_empty());
}

#[test]
fn warm_entries_release_source_assets_and_expire() {
    let mut residency = Residency::new(ResidencyPolicy::desktop(65536));
    let source = Rc::new(RasterImage::render(128, 128, |_| {}).unwrap());
    let weak = Rc::downgrade(&source);
    let program = PaintProgram::new(
        1,
        (16.0, 16.0),
        1.0,
        Rc::new(vec![Command::Image {
            bounds: Bounds::new(0.0, 0.0, 16.0, 16.0),
            image: source.into(),
            fit: crate::ImageFit::Contain,
        }]),
    )
    .unwrap();
    let owner = Owner::Item(program.id());
    let id = residency.insert(owner, value(Recipe::item(&program)), 500);
    drop(program);
    let now = Instant::now();
    residency.drawn(id);
    residency.release(id, Retention::Nearby, now);
    assert!(
        weak.upgrade().is_none(),
        "warm raster retained the source image"
    );
    assert_eq!(residency.stats().selected_bytes, 0);
    assert_eq!(residency.stats().warm_bytes, 1024);
    residency.maintain(now + residency.policy.warm_lifetime);
    assert_eq!(residency.stats().live_bytes, 0);
    assert!(residency.owners.is_empty());
    assert!(residency.items.is_empty());
}

#[test]
fn only_matching_nearby_content_returns_from_warm_storage() {
    let mut residency = Residency::new(ResidencyPolicy::desktop(65536));
    let recipe = recipe(1);
    let owner = Owner::Group(ScopeId::group(4.into()));
    let id = residency.insert(owner, value(recipe.clone()), 500);
    residency.drawn(id);
    residency.release(id, Retention::Nearby, Instant::now());
    let mut changed = recipe.clone();
    Rc::make_mut(&mut changed.fragments)[0].origin.0 += 1;
    assert_eq!(residency.acquire(owner, &changed), None);
    assert_eq!(residency.acquire(owner, &recipe), Some(id));
    assert_eq!(residency.stats().warm_bytes, 0);
    assert_eq!(residency.stats().selected_bytes, 1024);
    residency.release(id, Retention::Discard, Instant::now());
    assert_eq!(residency.stats().live_bytes, 0);
}

#[test]
fn pending_outputs_reserve_peak_storage_before_allocation() {
    let mut policy = ResidencyPolicy::desktop(0);
    policy.peak_bytes = 1024;
    let mut residency = Residency::new(policy);
    let recipe = recipe(1);
    let reservation = residency.reserve(&recipe).unwrap();
    assert_eq!(residency.stats().pending_bytes, 1024);
    assert!(residency.reserve(&recipe).is_none());
    drop(reservation);
    assert_eq!(residency.stats().pending_bytes, 0);
    let id = residency.insert(
        Owner::Group(ScopeId::group(1.into())),
        value(recipe.clone()),
        500,
    );
    assert!(residency.reserve(&recipe).is_none());
    residency.release(id, Retention::Discard, Instant::now());
    assert!(residency.reserve(&recipe).is_some());
}

#[test]
fn native_submission_remains_accounted_after_retirement_and_clear() {
    for clear in [false, true] {
        let mut residency = Residency::new(ResidencyPolicy::desktop(0));
        let id = residency.insert(
            Owner::Group(ScopeId::group(1.into())),
            value(recipe(1)),
            500,
        );
        let mut pixels = vec![0; 32 * 32 * 4];
        let mut painter =
            Painter::with_threads(&mut pixels, 32, 32, std::num::NonZeroU32::new(2).unwrap())
                .unwrap();
        painter.draw_image(residency.image(id), 0, 0, 16, 16);
        if clear {
            residency.clear();
        } else {
            residency.release(id, Retention::Discard, Instant::now());
        }
        residency.maintain(Instant::now());
        assert_eq!(residency.stats().selected_bytes, 0);
        assert_eq!(residency.stats().retired_bytes, 1024);
        painter.finish().unwrap();
        residency.maintain(Instant::now());
        assert_eq!(residency.stats().live_bytes, 0);
        assert!(residency.allocations.is_empty());
    }
}

#[test]
fn warm_replacement_never_displaces_selected_resources() {
    let mut residency = Residency::new(ResidencyPolicy::desktop(65536));
    let cheap = residency.insert(Owner::Group(ScopeId::group(1.into())), value(recipe(1)), 10);
    let useful = residency.insert(
        Owner::Group(ScopeId::group(2.into())),
        value(recipe(2)),
        1000,
    );
    let active = residency.insert(Owner::Group(ScopeId::group(3.into())), value(recipe(3)), 5);
    residency.policy.warm_bytes = residency.entries[&cheap].warm_cost;
    residency.drawn(cheap);
    residency.drawn(useful);
    residency.release(cheap, Retention::Nearby, Instant::now());
    residency.release(useful, Retention::Nearby, Instant::now());
    assert!(!residency.entries.contains_key(&cheap));
    assert!(residency.entries.contains_key(&useful));
    assert!(residency.entries.contains_key(&active));
    residency.trim_warm();
    assert_eq!(residency.stats().selected_bytes, 1024);
    assert_eq!(residency.stats().warm_bytes, 0);
    residency.release(active, Retention::Discard, Instant::now());
    assert_eq!(residency.stats().live_bytes, 0);
}
