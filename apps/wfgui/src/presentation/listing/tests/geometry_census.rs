use super::*;
use retinue_render::{FrameSnapshot, PaintFrame};
use retinue_ui::scene::census;
use serde_json::{Value, json};

mod property_probe;

fn fixture(scale: f32) -> Pages {
    let mut pages = pages(scale, 1);
    pages.relics_view.eager_grid = true;
    let base = pages.relics.items[0].clone();
    pages.relics.items = (0..90)
        .map(|i| {
            let mut row = base.clone();
            row.id = format!("relic-{i:03}");
            row.name = format!("Item {i:03}");
            row
        })
        .collect();
    pages.relics.reorder(&pages.quotes, true);
    pages.set_page(Page::Relics);
    pages.relics.scroll.to(8.0 * scale, false, Instant::now());
    pages.layout_center();
    pages
}

fn paint_snapshot(pages: &Pages) -> FrameSnapshot {
    let (width, height) = pages.ui.size;
    pages
        .ui
        .paint_snapshot(
            &mut PaintFrame::default(),
            Rect {
                width,
                height,
                ..Default::default()
            },
        )
        .0
}

fn hit_map(pages: &mut Pages) -> Vec<Option<String>> {
    let (w, h) = pages.ui.size;
    let old = pages.ui.pointer();
    let mut hits = Vec::new();
    for y in (0..h).step_by(19) {
        for x in (0..w).step_by(31) {
            pages.ui.pointer_move((x.into(), y.into()));
            hits.push(pages.ui.hover.clone());
        }
    }
    pages.ui.pointer_move(old);
    hits
}

fn run(scale: f32, name: &str) -> Value {
    let mut pages = fixture(scale);
    let keys = pages.relics_view.model.cells.order().keys().to_vec();
    let old = paint_snapshot(&pages);
    let replace = matches!(name, "membership" | "both");
    if replace {
        let i = pages.relics.visible[1];
        pages
            .relics
            .edit_fixture_rows(|rows| rows[i].id.push_str("-replacement"));
    }
    if matches!(name, "offset" | "both" | "row-crossing") {
        let delta = if name == "row-crossing" { 140.0 } else { 4.0 };
        pages
            .relics
            .scroll
            .to((8.0 + delta) * scale, false, Instant::now());
    }
    let input = pages.ui.input_census;
    let (_, work) = census::measure(|| pages.layout_center());
    let input_lookups = pages.ui.input_census.0 - input.0;
    let hit_regions = pages.ui.input_census.1 - input.1;
    let scene = pages.ui.scene_update_stats();
    let new_keys = pages.relics_view.model.cells.order();
    let mounted = new_keys.keys().len();
    let added = new_keys.keys().iter().filter(|k| !keys.contains(k)).count();
    let removed = keys.iter().filter(|k| !new_keys.keys().contains(k)).count();
    if name == "row-crossing" {
        assert!(added > 0 && removed > 0);
    } else {
        assert_eq!(
            (added, removed),
            (usize::from(replace), usize::from(replace))
        );
    }
    let next = paint_snapshot(&pages);
    let prior: HashMap<_, _> = old.frame().instances().iter().map(|i| (i.key, i)).collect();
    let mut moved = 0;
    let mut reclip = 0;
    for i in next.frame().instances() {
        if let Some(j) = prior.get(&i.key) {
            moved += usize::from(i.origin() != j.origin());
            reclip += usize::from(i.clip() != j.clip());
        }
    }
    let properties = if name == "offset" {
        assert_eq!(scene.recordings, 0);
        Some(property_probe::compare(&old, &next, scale))
    } else {
        None
    };
    let image = render(&pages);
    let hits = hit_map(&mut pages);
    assert!(hits.iter().flatten().any(|s| s.starts_with("list:item:")));
    pages.ui.install_compiled(pages.relics_view.view(&pages.ui));
    pages.layout_center();
    crate::presentation::tests::assert_pixels_match(&image, &render(&pages), name);
    assert_eq!(hits, hit_map(&mut pages));
    json!({"edit":name,"scale":scale,"added_cells":added,"removed_cells":removed,
        "world_resolves":work.world_resolves,"restored_slots":work.restored_slots,
        "bounds_calls":work.bounds_calls,"distinct_bounds_keys":work.bounds_keys.len(),
        "input_binding_lookups":input_lookups,"hit_regions_published":hit_regions,
        "slice_requests":work.slice_requests,"paint_placements_emitted":work.paint_placements,
        "survivor_origins_changed":moved,"survivor_clips_changed":reclip,
        "recordings":scene.recordings,"mounted_cells":mounted,
        "property_probe":properties})
}

#[test]
#[ignore = "headless geometry reader census and transform publication experiment"]
fn listing_geometry_census() {
    let rows: Vec<_> = [1.0, 1.2]
        .into_iter()
        .flat_map(|scale| {
            ["unchanged", "offset", "membership", "both", "row-crossing"]
                .map(|name| run(scale, name))
        })
        .collect();
    std::fs::write(
        std::env::var_os("GUI_BENCH_OUTPUT").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
