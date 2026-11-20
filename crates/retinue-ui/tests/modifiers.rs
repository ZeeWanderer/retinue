use retinue_render::{Bounds, Color};
use retinue_ui::{authoring::*, ir::InstanceGraph, layout::anchors::AnchorPoint};

#[test]
fn named_content_keeps_its_bounds_inside_padding_and_background() {
    let mut graph = InstanceGraph::with_root_body(
        |_| {
            ui! {
                ZStack() {
                    Rectangle(Color::rgb(0xaa2222))
                        .frame(Frame::new().width(40.0).height(30.0))
                        .named("tile".into())
                        .padding(5.0)
                        .background(Rectangle(Color::rgb(0x112233)).named("back".into()))
                        .offset(10.0, 20.0)
                }.alignment(AnchorPoint::TOP_LEADING)
            }
        },
        Default::default(),
    )
    .unwrap();
    graph.update_inline((400, 300), 1.0).unwrap();
    let bounds = |name: &'static str| graph.named_bounds(name.into()).unwrap().unwrap().layout;
    assert_eq!(bounds("tile"), Bounds::new(15.0, 25.0, 40.0, 30.0));
    assert_eq!(bounds("back"), Bounds::new(10.0, 20.0, 50.0, 40.0));
}
