use super::*;
use crate::Key;

#[test]
fn named_overlay_keeps_its_address_and_hit_region_when_alignment_changes() {
    use crate::layout::anchors::AnchorPoint;

    let alignment = State::new(AnchorPoint::TOP_LEADING);
    let placement = alignment.clone();
    let badge = Key::new("badge", 1);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                empty()
                    .frame(Frame::new().width(120.0).height(80.0))
                    .overlay_aligned(
                        placement.get(),
                        empty()
                            .frame(Frame::new().width(20.0).height(10.0))
                            .named(badge)
                            .interactive(),
                    )
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let address = graph.unique(badge).unwrap().unwrap();
    let first = graph.geometry(address).unwrap().layout;
    let old_point = (first.x + 5.0, first.y + 5.0);
    assert_eq!(graph.hit_test(old_point), Some(badge));
    alignment.set(AnchorPoint::BOTTOM_TRAILING);
    advance(&mut graph);
    assert_eq!(graph.unique(badge).unwrap(), Some(address));
    let moved = graph.named_bounds(badge).unwrap().unwrap().layout;
    assert_eq!(graph.geometry(address).unwrap().layout, moved);
    assert!(moved.x > first.x && moved.y > first.y);
    assert_ne!(graph.hit_test(old_point), Some(badge));
    assert_eq!(graph.hit_test((moved.x + 5.0, moved.y + 5.0)), Some(badge));
}

#[test]
fn input_and_window_actions_resolve_names_outside_the_drawing_leaf() {
    use crate::semantics::{WindowAction, WindowCapabilities};
    let name = Key::new("button", 1);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                Rectangle(Color::rgb(0x123456))
                    .interactive()
                    .window_action(WindowAction::Close)
                    .frame(Frame::new().width(80.0).height(24.0))
                    .named(name)
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let rect = graph
        .geometry(graph.unique(name).unwrap().unwrap())
        .unwrap()
        .layout;
    let position = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    assert_eq!(graph.hit_test(position), Some(name));
    let (key, action) = graph
        .scene()
        .window_action(
            (position.0 as f64, position.1 as f64),
            WindowCapabilities::DESKTOP,
        )
        .unwrap();
    assert_eq!(graph.enclosing_name(key), Some(name));
    assert_eq!(action, WindowAction::Close);
}

#[test]
fn names_follow_published_geometry_and_retire_with_a_branch() {
    let visible = State::new(true);
    let name = State::new(Key::new("slot", 1));
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            {
                let visible = visible.clone();
                let name = name.clone();
                move |_| {
                    ui! {
                        if visible.get() {
                            Rectangle(Color::rgb(0x123456))
                                .frame(Frame::new().width(80.0).height(24.0))
                                .named(name.get()).interactive()
                        }
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    assert!(!graph.has_name(name.get()));
    assert!(graph.named_bounds(name.get()).unwrap().is_none());
    advance(&mut graph);
    let first = graph.unique(name.get()).unwrap().unwrap();
    let geometry = graph.geometry(first).unwrap();
    assert_eq!(
        graph.named_bounds(name.get()).unwrap().unwrap().key,
        geometry.key
    );
    assert_eq!(
        graph.named_bounds(name.get()).unwrap().unwrap().layout,
        geometry.layout
    );
    assert_eq!(geometry.layout.width, 80.0);
    assert!(geometry.interactive);
    name.set(Key::new("slot", 2));
    assert!(graph.has_name(Key::new("slot", 1)));
    assert_eq!(
        graph
            .named_bounds(Key::new("slot", 1))
            .unwrap()
            .unwrap()
            .layout,
        geometry.layout
    );
    assert!(graph.named_bounds(Key::new("slot", 2)).unwrap().is_none());
    assert!(!graph.has_name(Key::new("slot", 2)));
    advance(&mut graph);
    assert_eq!(graph.unique(name.get()).unwrap(), Some(first));
    assert!(!graph.has_name(Key::new("slot", 1)));
    visible.set(false);
    assert!(graph.has_name(name.get()));
    advance(&mut graph);
    assert!(!graph.has_name(name.get()));
    assert!(graph.named_bounds(name.get()).unwrap().is_none());
    assert!(graph.geometry(first).is_none());
}

#[test]
fn names_are_not_collection_identity_and_ambiguity_is_explicit() {
    let mut graph = InstanceGraph::default();
    let name = Key::new("shared name", 0);
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    HStack(4.0) {
                        Rectangle(Color::rgb(0x123456)).named(name)
                        Rectangle(Color::rgb(0x123456)).named(name)
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.unique(name), Err("ambiguous authored name"));
    assert_eq!(
        graph.named_bounds(name).unwrap_err(),
        "ambiguous authored name"
    );
}

#[test]
fn a_named_composite_exposes_one_geometry_without_a_logical_solver_box() {
    let name = Key::new("composite", 0);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    Group() {
                        Rectangle(Color::rgb(0x123456)).frame(Frame::new().width(20.0).height(30.0))
                        Rectangle(Color::rgb(0x654321)).frame(Frame::new().width(60.0).height(10.0))
                    }.named(name)
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let bounds = graph
        .geometry(graph.unique(name).unwrap().unwrap())
        .unwrap();
    assert_eq!((bounds.layout.width, bounds.layout.height), (60.0, 30.0));
    assert_eq!(graph.authored_key(bounds.key), Some(name));
}

#[test]
fn outer_metadata_replaces_inner_bindings_and_fill_tracks_the_proposal() {
    let name = Key::new("outer", 0);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                Rectangle(Color::rgb(0xffffff))
                    .frame(Frame::new().fill_width().height(24.0))
                    .named(Key::new("inner", 0))
                    .interactive()
                    .clipped()
                    .named(name)
                    .interactive()
                    .clipped()
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let address = graph.unique(name).unwrap().unwrap();
    assert!(!graph.has_name(Key::new("inner", 0)));
    assert_eq!(graph.geometry(address).unwrap().layout.width, 400.0);
    graph.advance((300, 200), 1.0, None).unwrap();
    assert_eq!(graph.geometry(address).unwrap().layout.width, 300.0);
    assert!(graph.geometry(address).unwrap().interactive);
}
