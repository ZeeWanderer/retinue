use super::*;
use crate::{HostPolicy, RenderHost, ResidencyPolicy};
use std::time::Duration;

fn host(tasks: usize) -> RenderHost {
    RenderHost::new(HostPolicy {
        preparation_tasks: tasks,
        preparation_pixels: usize::MAX,
        preparation_time: Duration::MAX,
        ..HostPolicy::desktop()
    })
}

#[test]
fn deferred_items_use_latest_inputs_and_converge_without_retaining_obsolete_programs() {
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let host = host(1);
        let mut frame = PaintFrame::default();
        let initial: Vec<_> = (0..3)
            .map(|i| item(i, 0, (i as i32 * 40, 10), scale))
            .collect();
        let weak = Rc::downgrade(initial[2].program.commands());
        frame.replace(SIZE, scale, initial, Vec::new()).unwrap();
        let mut cached = Renderer::new(ResidencyPolicy::desktop(65536));
        cached.set_host(host.clone());
        let mut raw = Renderer::new(ResidencyPolicy::desktop(0));
        raw.set_optimizations(false);
        assert_eq!(
            paint(&frame, &mut cached, RasterMode::Explicit),
            paint(&frame, &mut raw, RasterMode::Replay)
        );
        assert_eq!(cached.stats.prepared_rasters, 1);
        assert_eq!(cached.stats.deferred_preparations, 2);
        assert_eq!(cached.stats.failed_preparations, 0);
        assert!(host.has_deferred());
        frame
            .replace(
                SIZE,
                scale,
                vec![item(0, 0, (0, 10), scale), item(1, 1, (50, 20), scale)],
                Vec::new(),
            )
            .unwrap();
        for _ in 0..3 {
            assert_eq!(
                paint(&frame, &mut cached, RasterMode::Explicit),
                paint(&frame, &mut raw, RasterMode::Replay)
            );
        }
        assert!(weak.upgrade().is_none(), "removed queued item retained");
        assert!(!cached.has_deferred());
        assert!(!host.has_deferred());
        assert_eq!(host.stats().pending_bytes, 0);
        assert_eq!(cached.residency_stats().entries, 2);
        cached.clear_cache();
        assert_eq!(host.stats().live_bytes, 0);
    }
}

#[test]
fn siblings_share_one_pass_and_retry_after_another_store_releases_memory() {
    let host = RenderHost::new(HostPolicy {
        peak_bytes: 40 * 30 * 4,
        ..HostPolicy::desktop()
    });
    let mut frame = PaintFrame::default();
    frame
        .replace(SIZE, 1.0, vec![item(1, 0, (0, 0), 1.0)], Vec::new())
        .unwrap();
    let mut a = Renderer::new(ResidencyPolicy::desktop(0));
    let mut b = Renderer::new(ResidencyPolicy::desktop(0));
    a.set_host(host.clone());
    b.set_host(host.clone());
    {
        let _pass = host.begin_pass(0);
        a.prepare(&frame, RasterMode::Explicit);
        b.prepare(&frame, RasterMode::Explicit);
    }
    assert_eq!(a.stats.prepared_rasters, 1);
    assert_eq!(b.stats.rejected_rasters, 1);
    assert!(
        !host.has_deferred(),
        "pressure rejection must not spin a retry loop"
    );
    drop(a);
    b.prepare(&frame, RasterMode::Explicit);
    assert_eq!(b.stats.prepared_rasters, 1);
    assert_eq!(host.stats().live_bytes, 40 * 30 * 4);
}

#[test]
fn canceled_reservations_unblock_unchanged_items() {
    let host = RenderHost::new(HostPolicy {
        peak_bytes: 2 * 40 * 30 * 4,
        preparation_tasks: 1,
        preparation_time: Duration::MAX,
        ..HostPolicy::desktop()
    });
    let mut frame = PaintFrame::default();
    frame
        .replace(
            SIZE,
            1.0,
            (0..3)
                .map(|i| item(i, 0, (i as i32 * 40, 0), 1.0))
                .collect(),
            Vec::new(),
        )
        .unwrap();
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.set_host(host.clone());
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(renderer.stats.prepared_rasters, 1);
    assert_eq!(renderer.stats.deferred_preparations, 1);
    assert_eq!(renderer.stats.rejected_rasters, 1);
    let mut chunks = frame.chunks.clone();
    chunks[1].set_pixels((1000, 0), viewport());
    frame.replace(SIZE, 1.0, chunks, Vec::new()).unwrap();
    paint(&frame, &mut renderer, RasterMode::Explicit);
    assert_eq!(
        renderer.stats.prepared_rasters, 1,
        "freed reservation did not wake blocked item"
    );
    assert_eq!(host.stats().selected_bytes, 2 * 40 * 30 * 4);
    assert!(!host.has_deferred());
}

#[test]
fn deferred_layer_repair_is_not_failure_or_a_cooldown() {
    let mut frame = PaintFrame::default();
    let mut a = item(1, 0, (10, 10), 1.0);
    a.cache = CachePolicy::Never;
    frame
        .replace(
            SIZE,
            1.0,
            vec![a],
            vec![PaintGroup {
                key: 9.into(),
                instances: 0..1,
                context: crate::OptimizationContext::default().with(crate::CacheHint::Stable),
            }],
        )
        .unwrap();
    let host = host(1);
    let mut cached = Renderer::with_layers(
        ResidencyPolicy::desktop(65536),
        LayerPolicy {
            stable_repaints: 1,
            conditional_repaints: 1,
            min_work: 1,
            threads: 2,
        },
    );
    cached.set_host(host.clone());
    let mut raw = Renderer::new(ResidencyPolicy::desktop(0));
    raw.set_optimizations(false);
    for _ in 0..3 {
        paint(&frame, &mut cached, RasterMode::Explicit);
    }
    assert_eq!(cached.layers.count(), 1);
    let mut changed = item(1, 1, (10, 10), 1.0);
    changed.cache = CachePolicy::Never;
    frame
        .replace(SIZE, 1.0, vec![changed], frame.groups.clone())
        .unwrap();
    {
        let _pass = host.begin_pass(0);
        assert!(host.admit(1, 1).is_some());
        assert_eq!(
            paint(&frame, &mut cached, RasterMode::Explicit),
            paint(&frame, &mut raw, RasterMode::Replay)
        );
        assert_eq!(cached.stats.deferred_preparations, 1);
        assert_eq!(cached.stats.failed_preparations, 0);
    }
    assert_eq!(
        paint(&frame, &mut cached, RasterMode::Explicit),
        paint(&frame, &mut raw, RasterMode::Replay)
    );
    assert_eq!(
        cached.layers.count(),
        1,
        "deferred repair entered failure cooldown"
    );
    assert!(!cached.has_deferred());
}
