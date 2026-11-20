use super::*;
use crate::{
    input::{EventHandler, InputOptions},
    runtime::ViewTree,
    scene::Scene,
    view as reference,
};
use retinue_render::{RasterMode, Rect, Renderer, ResidencyPolicy, painter::Painter};

fn mount<B: Body>(body: B) -> (InstanceGraph, crate::ir::Handle) {
    let mut body = Some(body);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(move |_| body.take().unwrap(), Environment::default())
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    (graph, root)
}

pub(super) fn pixels(scene: &Scene) -> Vec<u8> {
    let size = scene.frame().size();
    let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare(scene.frame(), RasterMode::Replay);
    let mut painter =
        Painter::with_threads(&mut pixels, size.0, size.1, 4.try_into().unwrap()).unwrap();
    renderer.paint(
        scene.frame(),
        &mut painter,
        Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        },
        Color::rgb(0x202020),
    );
    painter.finish().unwrap();
    pixels
}

pub(super) fn matches_reference(body: impl Body, reference: impl reference::IntoElement) {
    let (mut graph, _) = mount(body);
    let mut tree = ViewTree::new(reference, || {});
    for scale in [1.0, 1.2, 1.5] {
        let size = ((180.0 * scale) as u32, (100.0 * scale) as u32);
        graph.advance(size, scale, None).unwrap();
        tree.update(size, scale).unwrap();
        assert!(
            pixels(graph.scene()) == pixels(tree.scene()),
            "scale {scale}"
        );
    }
}

fn framed(content: impl reference::IntoElement) -> reference::Element {
    reference::ViewExt::frame(content, 140.0, 60.0)
}

#[test]
fn background_fusion_preserves_frame_and_offset_order() {
    matches_reference(
        Text("Frame then background")
            .frame(Frame::new().width(140.0).height(60.0))
            .background(Canvas(Fill)),
        reference::ViewExt::background(framed(Text("Frame then background")), Canvas(Fill)),
    );
    matches_reference(
        Text("Background then frame")
            .background(Canvas(Fill))
            .frame(Frame::new().width(140.0).height(60.0)),
        framed(reference::ViewExt::background(
            Text("Background then frame"),
            Canvas(Fill),
        )),
    );
    matches_reference(
        Text("Moved content")
            .frame(Frame::new().width(140.0).height(60.0))
            .offset(17.0, 9.0)
            .background(Canvas(Fill)),
        reference::ViewExt::background(
            reference::ViewExt::offset(framed(Text("Moved content")), 17.0, 9.0),
            Canvas(Fill),
        ),
    );
    matches_reference(
        Text("Moved background")
            .frame(Frame::new().width(140.0).height(60.0))
            .background(Canvas(Fill))
            .offset(17.0, 9.0),
        reference::ViewExt::offset(
            reference::ViewExt::background(framed(Text("Moved background")), Canvas(Fill)),
            17.0,
            9.0,
        ),
    );
}

#[test]
fn intrinsic_and_modified_backgrounds_keep_their_own_layout() {
    matches_reference(
        Text("Front")
            .frame(Frame::new().width(140.0).height(60.0))
            .background(Text("Behind")),
        reference::ViewExt::background(framed(Text("Front")), Text("Behind")),
    );
    matches_reference(
        Text("Front")
            .frame(Frame::new().width(140.0).height(60.0))
            .background(Canvas(Fill).frame(Frame::new().width(40.0).height(20.0))),
        reference::ViewExt::background(
            framed(Text("Front")),
            reference::ViewExt::frame(Canvas(Fill), 40.0, 20.0),
        ),
    );
}

#[derive(PartialEq)]
struct FramedLabel;
#[view]
impl View for FramedLabel {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Text("Shared bounds").frame(Frame::new().width(140.0).height(60.0))
    }
}

#[derive(PartialEq)]
struct NestedLabel;
#[view]
impl View for NestedLabel {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        compose(FramedLabel)
    }
}

#[test]
fn background_fusion_crosses_fixed_view_boundaries_without_losing_scopes() {
    matches_reference(
        compose(NestedLabel).background(Canvas(Fill)),
        reference::ViewExt::background(framed(Text("Shared bounds")), Canvas(Fill)),
    );
    let (mut graph, root) = mount(compose(NestedLabel).background(Canvas(Fill)));
    advance(&mut graph);
    assert_eq!(graph.len(), 1);
    for local in 0..4 {
        assert!(
            graph
                .geometry(Address {
                    instance: root,
                    local: Local(local)
                })
                .is_none()
        );
        assert!(graph.key(root, Local(local)).is_ok());
    }
    let key = graph.key(root, Local(4)).unwrap();
    assert!(!graph.scene().paint(key).unwrap().uses_layout);
    assert_eq!(graph.scene().bounds(key).unwrap().layout.width, 140.0);
}

#[test]
fn content_hit_policy_does_not_disable_its_background() {
    let (mut graph, root) = mount(
        Text("Disabled content")
            .frame(Frame::new().width(140.0).height(60.0))
            .hit_testing(false)
            .background(Canvas(Fill)),
    );
    graph
        .bind_input(
            Address {
                instance: root,
                local: Local(1),
            },
            true,
            Some(InputOptions::new(EventHandler::new(|_, _| {}))),
        )
        .unwrap();
    advance(&mut graph);
    assert!(graph.input().hit_test((10.0, 10.0)).is_some());
}

#[derive(PartialEq)]
struct Tint(Color);
impl CanvasContent for Tint {
    fn draw(&self, context: &mut DrawingContext) {
        context.fill(context.bounds, 0.0, self.0);
    }
}

#[test]
fn fused_background_updates_only_paint_and_keeps_logical_identity() {
    let tint = State::new(Color::rgb(0x123456));
    let observed = tint.clone();
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                Text("Foreground")
                    .frame(Frame::new().width(140.0).height(60.0))
                    .background(Canvas(Tint(observed.get())))
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(
        graph
            .scene()
            .geometry()
            .iter()
            .filter(|geometry| geometry.key.namespace == "ui.ir.node")
            .count(),
        2
    );
    for local in [0, 1] {
        assert!(
            graph
                .geometry(Address {
                    instance: root,
                    local: Local(local)
                })
                .is_none()
        );
        assert!(graph.key(root, Local(local)).is_ok());
    }
    let key = graph.key(root, Local(2)).unwrap();
    assert!(!graph.scene().paint(key).unwrap().uses_layout);
    let before = pixels(graph.scene());
    tint.set(Color::rgb(0xabcdee));
    let update = advance(&mut graph);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(update.work.topology_visited, 0);
    assert_eq!(update.work.nodes_materialized, 0);
    assert_eq!(graph.scene().stats.layout_computations, 0);
    assert_eq!(graph.scene().stats.recordings, 1);
    assert_eq!(graph.key(root, Local(2)).unwrap(), key);
    assert_ne!(pixels(graph.scene()), before);
}

#[derive(PartialEq)]
struct Badge;
#[view]
impl View for Badge {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Text("Badge")
            .frame(Frame::new().width(140.0).height(60.0))
            .background(Canvas(Fill))
    }
}

#[test]
fn outer_hit_testing_overrides_inner_setting_through_fused_logical_groups() {
    let enabled = State::new(true);
    let observed = enabled.clone();
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                compose(Badge)
                    .hit_testing(false)
                    .hit_testing(observed.get())
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph
        .bind_input(
            Address {
                instance: root,
                local: Local(0),
            },
            true,
            Some(InputOptions::new(EventHandler::new(|_, _| {}))),
        )
        .unwrap();
    advance(&mut graph);
    assert!(graph.input().hit_test((10.0, 10.0)).is_some());
    enabled.set(false);
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    assert!(graph.input().hit_test((10.0, 10.0)).is_none());
    enabled.set(true);
    advance(&mut graph);
    assert!(graph.input().hit_test((10.0, 10.0)).is_some());
}
