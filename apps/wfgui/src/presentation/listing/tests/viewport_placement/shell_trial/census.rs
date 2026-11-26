use super::*;
use retinue_ui::scene::census::measure;
use serde_json::json;

#[test]
#[ignore = "same-frame publication and leaf-packet work; not a timing benchmark"]
fn shell_viewport_census() {
    let mut rows = Vec::new();
    for scale in [1.0, 1.2] {
        for scenario in ["repaint", "offset", "crossings"] {
            let mut windows = Vec::new();
            for path in ["eager", "live", "reuse"] {
                let mut t = Trial::new(scale, path != "eager");
                if let Some(grid) = &mut t.grid {
                    let mut grid = grid.borrow_mut();
                    grid.live_packet = true;
                    if path == "live" {
                        grid.host.override_packet_reuse(false);
                    }
                }
                let repair = t.shell.geometry.center;
                for i in 0..76 {
                    let offset = sample_offset(scenario, i);
                    let before = t.shell.pages.relic_window();
                    let child_updates = t.grid.as_ref().map(|g| g.borrow().freezes);
                    let packets = t
                        .grid
                        .as_ref()
                        .and_then(|g| g.borrow().host.leaf_packet_counts());
                    let prior_repair = t.local_repair.take();
                    let (_, work) = measure(|| t.frame(offset, i % 3, Damage::rect(repair)));
                    let after = t.shell.pages.relic_window();
                    if path == "eager" {
                        windows.push(after.clone());
                    } else {
                        assert_eq!(after, windows[i], "{scale} {scenario} {path} frame {i}");
                    }
                    let leaf = packets.map(|[hits, builds, invalidations]| {
                        let [h, b, d] = t
                            .grid
                            .as_ref()
                            .unwrap()
                            .borrow()
                            .host
                            .leaf_packet_counts()
                            .unwrap();
                        ([h - hits, b - builds], d - invalidations)
                    });
                    let repairs = prior_repair.as_ref().zip(t.local_repair.as_ref());
                    // Only attribute selection differences to repair when the grid did not update.
                    let selection = t
                        .grid
                        .as_ref()
                        .filter(|g| Some(g.borrow().freezes) == child_updates)
                        .and_then(|g| {
                            repairs.map(|(a, b)| g.borrow().host.viewport_selection_counts(a, b))
                        });
                    rows.push(json!({
                        "scale": scale, "path": path, "scenario": scenario, "frame": i,
                        "offset": offset, "window": [after.start, after.end],
                        "membership_changed": before != after, "warmup": i < 12,
                        "world_resolves": work.world_resolves, "restored_slots": work.restored_slots,
                        "paint_placements": work.paint_placements, "recordings": work.recordings,
                        "child_updates": child_updates.map(|n| t.grid.as_ref().unwrap().borrow().freezes - n),
                        "leaf_hits_builds": leaf.map(|x| x.0),
                        "leaf_invalidations": leaf.map(|x| x.1),
                        "local_repair_px": t.local_repair.as_ref().map(|r| r.area()),
                        "prior_overlap_px": repairs.map(|(a, b)| a.intersection_with(b).area()),
                        "instances_kept_entered_left": selection
                    }));
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec(&rows).unwrap(),
    )
    .unwrap();
}
