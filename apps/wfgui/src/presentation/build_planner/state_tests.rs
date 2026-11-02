use super::*;

fn pages(scale: f32) -> Pages {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    pages.set_page(Page::Builds);
    pages.configure(
        ((1000.0 * scale) as u32, (900.0 * scale) as u32),
        (400, 900),
        scale,
    );
    pages
}

fn state(pages: &mut Pages, name: &str) {
    let b = &mut pages.builds;
    match name {
        "groups-empty" => b.groups = json!({"groups":[]}),
        "groups-loading" => b.groups = Value::Null,
        "groups-choose" => b.group = Value::Null,
        "groups-empty-member" => {
            b.group["members"] = json!([]);
            b.group["plan_result"] = Value::Null;
        }
        "groups-blocked" => {
            b.group["plan_result"] = json!({"status":"blocked","reason":"unresolved_mod_cost"})
        }
        "groups-planned" => b.planned = true,
        "groups-changes" => {
            b.group["plan_result"]["operations"] = json!([
                {"action":"swap","label":"Mod 1","other_label":"Mod 2"},
                {"action":"apply","forma":"Umbral","label":"Mod 3","before":"none","polarity":"umbra"}]);
            b.group["plan_result"]["forma_requirements"] = json!({"umbral_forma":1});
            b.group["plan_result"]["final_polarities"][0]["changed"] = json!(true);
        }
        "equipment-choose" => {
            b.tab = "equipment".into();
            b.definition.clear();
        }
        "equipment-loading" => {
            b.tab = "equipment".into();
            b.definition.clear();
            b.equipment = None;
        }
        "equipment-collapsed" => {
            b.tab = "equipment".into();
            b.collapsed = true;
            b.collapse.set(1.0, Instant::now(), Duration::ZERO);
        }
        "discover-empty" => {
            b.tab = "discover".into();
            b.detail = Value::Null;
        }
        "discover-items" => {
            b.tab = "discover".into();
            b.detail = Value::Null;
            b.items = json!({"items":[{"canonical_id":"boar-prime","name":"Boar Prime","class":"primary"}]});
        }
        "discover-builds" => {
            b.tab = "discover".into();
            b.detail = Value::Null;
            b.sources = json!({"builds":[{"identity":{"external_id":123},"title":"Example build","author":{"username":"Tenno"},"formas":3,"score":23}]});
        }
        _ => {
            b.dialog = Some(name.into());
            b.name = "Boar Prime".into();
            b.target = "fixture-group".into();
        }
    }
    pages.builds.invalidate();
    pages.dirty = true;
    pages.layout();
}

#[test]
fn build_page_states_keep_input_scoped_and_can_be_captured() {
    for scale in [1.0, 1.25, 2.0] {
        for name in [
            "groups-empty",
            "groups-loading",
            "groups-choose",
            "groups-empty-member",
            "groups-blocked",
            "groups-planned",
            "groups-changes",
            "equipment-choose",
            "equipment-loading",
            "equipment-collapsed",
            "discover-empty",
            "discover-items",
            "discover-builds",
            "create",
            "create-source",
            "rename",
            "delete",
            "source",
            "config",
        ] {
            let mut pages = pages(scale);
            state(&mut pages, name);
            assert_eq!(pages.ui.has_modal(), pages.builds.dialog.is_some());
            capture(&mut pages, name);
        }
    }
}

fn capture(pages: &mut Pages, name: &str) {
    let Some(root) = std::env::var_os("GUI_BUILD_CAPTURES") else {
        return;
    };
    use retinue_render::{image::RgbaImage, thumbnails::Thumbnails};
    let requests = pages.ui.requests(&pages.media);
    let mut images = Thumbnails::new(0, 0);
    let ready = requests
        .iter()
        .filter_map(|r| images.prepare(r).ok().map(|i| (r.key, i)))
        .collect::<Vec<_>>();
    pages.media.accept(&ready, &requests);
    let (w, h) = pages.ui.size;
    let mut pixels = vec![0; (w * h * 4) as usize];
    let mut painter = Painter::new(&mut pixels, w, h).unwrap();
    pages.ui.paint(&mut painter, &pages.media);
    painter.finish().unwrap();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir_all(&root).unwrap();
    let stem = format!("{name}-{}", (pages.ui.scale * 100.0) as u32);
    RgbaImage::from_raw(w, h, pixels)
        .unwrap()
        .save(root.join(format!("{stem}.png")))
        .unwrap();
    std::fs::write(
        root.join(format!("{stem}.json")),
        serde_json::to_vec(&json!({"layout":pages.ui.layout_report()})).unwrap(),
    )
    .unwrap();
}
