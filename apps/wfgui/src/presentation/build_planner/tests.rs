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

#[test]
fn pointer_scroll_and_modal_edits_do_not_reproject_equipment() {
    let mut p = pages(1.25);
    let model = p.build_view.model.get();
    p.capture_hover("build:mod:mod-1").unwrap();
    assert!(Rc::ptr_eq(&model, &p.build_view.model.get()));
    p.capture_scroll(100);
    p.dirty = true;
    p.layout();
    assert!(Rc::ptr_eq(&model, &p.build_view.model.get()));
    p.build_signal(Signal {
        key: "build:rename".into(),
        value: None,
    });
    p.dirty = true;
    p.layout();
    p.build_signal(Signal {
        key: "build:name".into(),
        value: Some("A different name".into()),
    });
    p.dirty = true;
    p.layout();
    assert!(Rc::ptr_eq(&model, &p.build_view.model.get()));
    assert!(p.ui.has_modal());
    p.builds.group["members"][0]["name"] = json!("Updated configuration");
    p.builds.invalidate();
    p.dirty = true;
    p.layout();
    assert!(!Rc::ptr_eq(&model, &p.build_view.model.get()));
    assert!(
        p.build_view.model.get().groups.members[0]
            .1
            .contains("Updated configuration")
    );
}

#[test]
fn declared_tabs_release_inactive_controls_and_preserve_model() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = pages(scale);
        let model = p.build_view.model.get();
        for tab in ["equipment", "discover", "groups", "equipment"] {
            p.build_signal(Signal {
                key: format!("build:tab:{tab}"),
                value: None,
            });
            p.dirty = true;
            p.layout();
            assert_eq!(p.ui.bounds("build:group").is_some(), tab == "groups");
            assert_eq!(p.ui.bounds("build:instance").is_some(), tab == "equipment");
            assert_eq!(p.ui.bounds("build:scope").is_some(), tab == "discover");
            assert!(Rc::ptr_eq(&model, &p.build_view.model.get()));
            assert!(p.ui.bounds("build:mod:mod-1").is_some());
        }
        p.set_page(Page::Market);
        p.layout();
        assert!(p.ui.bounds("build:tab:groups").is_none());
        p.set_page(Page::Builds);
        p.layout();
        assert!(p.ui.bounds("build:instance").is_some());
    }
}

#[test]
fn source_list_mounts_only_visible_rows_and_scrolls_to_last_result() {
    let mut p = pages(1.0);
    p.builds.tab = "discover".into();
    p.builds.detail = Value::Null;
    p.builds.items = json!({"items":(0..1000).map(|i|json!({"canonical_id":format!("item-{i}"),"name":format!("Equipment {i}"),"class":"primary"})).collect::<Vec<_>>()});
    p.builds.invalidate();
    p.dirty = true;
    p.layout();
    assert!(
        p.ui.layout_report()["controls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("build:source-item:")))
            .count()
            < 20
    );
    let model = p.build_view.model.get();
    p.builds
        .scroll
        .to(p.builds.scroll.maximum(), false, Instant::now());
    p.builds.tick(Instant::now());
    p.dirty = true;
    p.layout();
    assert!(p.ui.bounds("build:source-item:item-999").is_some());
    assert!(p.ui.bounds("build:source-item:item-0").is_none());
    assert!(Rc::ptr_eq(&model, &p.build_view.model.get()));
}

#[test]
fn projected_instance_suffix_preserves_unicode_boundaries() {
    let mut p = pages(1.0);
    let e = p.builds.equipment.as_mut().unwrap();
    e.instances[0].instance_id = "abcdefghЖЖЖЖЖЖЖ".into();
    p.builds.instance = e.instances[0].instance_id.clone();
    let model = build_planner_model::Model::new(&p.builds);
    assert!(
        model
            .equipment
            .copies
            .iter()
            .any(|(_, name)| name.ends_with("(ЖЖЖЖЖЖ)"))
    );
}
