use retinue_render::{Bounds, Color, Key};
use retinue_ui::{
    layout::{
        anchors::{AnchorPoint, AnchorTarget, Anchored, Attachment},
        values::LayoutValueKey,
    },
    prelude::*,
    runtime::ViewTree,
    semantics::{Role, WindowAction},
};

fn root(view: impl IntoElement) -> ViewTree {
    let mut tree = ViewTree::new(view, || {});
    tree.update((400, 300), 1.0).unwrap();
    tree
}

fn fills(tree: &ViewTree) -> Vec<Bounds> {
    tree.frame()
        .instances()
        .iter()
        .map(|instance| {
            let bounds = instance.visible_bounds();
            Bounds::new(
                bounds.x as f32,
                bounds.y as f32,
                bounds.width as f32,
                bounds.height as f32,
            )
        })
        .collect()
}

fn square() -> Element {
    Rectangle::new(Color::rgb(0xaa2222)).frame(40.0, 30.0)
}

#[test]
fn root_proposes_the_full_viewport_to_flexible_content() {
    let tree = root(ui! {
        ZStack() {
            Rectangle(Color::rgb(0x112233)).id("fill")
            square().offset(10.0, 20.0)
        }.alignment(Alignment::Start)
    });
    let fill = tree
        .geometry(tree.unique("fill").unwrap().unwrap())
        .unwrap();
    assert_eq!(fill.layout, Bounds::new(0.0, 0.0, 400.0, 300.0));
}

#[derive(PartialEq)]
struct NamedBody;
impl View for NamedBody {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        square().id("body")
    }
}

#[test]
fn named_geometry_includes_names_on_flattened_bodies() {
    for element in [NamedBody.into_element(), NamedBody.id("outer")] {
        let tree = root(element);
        let body = tree
            .geometry(tree.unique("body").unwrap().unwrap())
            .unwrap();
        let names = tree.named_geometry();
        assert!(
            names
                .iter()
                .any(|g| g.key == Key::from("body") && g.layout == body.layout)
        );
        let key = tree.authored_key(body.key).unwrap();
        assert!(key == Key::from("body") || key == Key::from("outer"));
    }
}

#[test]
fn fixed_frame_proposes_its_size_to_flexible_children() {
    let tree = root(ui! {
        ZStack() {
            Rectangle(Color::rgb(0x112233)).id("fill")
            empty().frame(40.0, 30.0).offset(10.0, 20.0).interactive().id("control")
        }.alignment(Alignment::Start).frame(200.0, 100.0)
    });
    let rect = |name| {
        tree.geometry(tree.unique(name).unwrap().unwrap())
            .unwrap()
            .layout
    };
    assert_eq!(rect("fill"), Bounds::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(rect("control"), Bounds::new(10.0, 20.0, 40.0, 30.0));
    let aligned = root(square().frame_aligned(100.0, 80.0, AnchorPoint::BOTTOM_TRAILING));
    assert_eq!(fills(&aligned), [Bounds::new(60.0, 50.0, 40.0, 30.0)]);
}

#[derive(PartialEq)]
struct Flexible;
impl Layout for Flexible {
    type Cache = ();
    fn measure(
        &self,
        proposal: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let value = |axis| match axis {
            retinue_ui::layout::Available::Limit(n) => n,
            _ => 0.0,
        };
        Ok(LayoutSize::new(
            value(proposal.width),
            value(proposal.height),
        ))
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
}

#[test]
fn padding_preserves_custom_layout_proposals_in_both_axes() {
    let tree = root(
        LayoutView::new(Flexible, ())
            .id("flexible")
            .padding(6.0)
            .frame(200.0, 100.0),
    );
    let geometry = tree
        .geometry(tree.unique("flexible").unwrap().unwrap())
        .unwrap();
    assert_eq!(geometry.layout, Bounds::new(6.0, 6.0, 188.0, 88.0));
}

#[test]
fn semantic_annotations_add_no_boxes_and_hits_resolve_through_modifiers() {
    use retinue_ui::semantics::WindowCapabilities;
    let plain = root(square());
    let tree = root(
        square()
            .id("title")
            .role(Role::Title)
            .window_action(WindowAction::Move),
    );
    assert_eq!(tree.node_count(), plain.node_count());
    let (key, action) = tree
        .scene()
        .window_action((20.0, 15.0), WindowCapabilities::DESKTOP)
        .unwrap();
    assert_eq!(tree.authored_key(key), Some("title".into()));
    assert_eq!(action, WindowAction::Move);
    let tree = root(
        Rectangle::new(Color::rgb(0xffffff))
            .interactive()
            .frame(40.0, 30.0)
            .offset(10.0, 20.0)
            .id("control"),
    );
    let hit = tree.scene().hit_test((20.0, 30.0)).unwrap();
    assert_eq!(tree.authored_key(hit), Some("control".into()));
    let id = tree.unique("control").unwrap().unwrap();
    assert_eq!(tree.geometry(id).unwrap().hit.x, 10);
}

#[test]
fn scoped_names_are_not_silently_treated_as_global_identity() {
    let tree = root(HStack::new(
        8.0,
        [
            square().id("item"),
            VStack::new(0.0, [square().id("item")]).into_element(),
        ],
    ));
    assert_eq!(tree.views("item").len(), 2);
    assert!(tree.unique("item").is_err());
    assert_eq!(tree.unique("absent").unwrap(), None);
}

#[test]
fn background_and_padding_order_change_decoration_bounds_not_content() {
    let a = root(
        square()
            .background(Rectangle::new(Color::rgb(0x2222aa)))
            .padding(10.0),
    );
    let b = root(
        square()
            .padding(10.0)
            .background(Rectangle::new(Color::rgb(0x2222aa))),
    );
    assert_eq!(
        fills(&a),
        [
            Bounds::new(10.0, 10.0, 40.0, 30.0),
            Bounds::new(10.0, 10.0, 40.0, 30.0)
        ]
    );
    assert_eq!(
        fills(&b),
        [
            Bounds::new(0.0, 0.0, 60.0, 50.0),
            Bounds::new(10.0, 10.0, 40.0, 30.0)
        ]
    );
}

#[test]
fn overlay_does_not_expand_primary_layout_and_has_explicit_alignment() {
    let tree = root(square().overlay_aligned(
        AnchorPoint::BOTTOM_TRAILING,
        Rectangle::new(Color::rgb(0x1122aa)).frame(10.0, 10.0),
    ));
    assert_eq!(
        fills(&tree),
        [
            Bounds::new(0.0, 0.0, 40.0, 30.0),
            Bounds::new(30.0, 20.0, 10.0, 10.0)
        ]
    );
    let large = root(square().overlay(Rectangle::new(Color::rgb(0x1122aa)).frame(80.0, 40.0)));
    let bounds = large
        .scene()
        .bounds(large.root().unwrap().key())
        .unwrap()
        .layout;
    assert_eq!((bounds.width, bounds.height), (40.0, 30.0));
}

#[test]
fn offset_moves_paint_not_layout_and_clipping_order_is_explicit() {
    let moved = root(square().offset(20.0, 10.0));
    assert_eq!(fills(&moved), [Bounds::new(20.0, 10.0, 40.0, 30.0)]);
    let root_bounds = moved
        .scene()
        .bounds(moved.root().unwrap().key())
        .unwrap()
        .layout;
    assert_eq!((root_bounds.width, root_bounds.height), (40.0, 30.0));
    assert_eq!(
        fills(&root(square().offset(20.0, 10.0).clipped())),
        [Bounds::new(20.0, 10.0, 20.0, 20.0)]
    );
    assert_eq!(
        fills(&root(square().clipped().offset(20.0, 10.0))),
        [Bounds::new(20.0, 10.0, 40.0, 30.0)]
    );
}

#[test]
fn window_annotations_do_not_require_a_special_window_layout() {
    let tree = root(square().window_action(WindowAction::Move).role(Role::Title));
    assert!(
        tree.scene()
            .geometry()
            .iter()
            .any(|node| node.window_action == Some(WindowAction::Move) && node.interactive)
    );
    assert!(
        tree.scene()
            .geometry()
            .iter()
            .any(|node| node.role == Role::Title)
    );
    let gated = root(
        square()
            .window_action(WindowAction::Move)
            .hit_testing(false),
    );
    assert!(
        gated
            .scene()
            .geometry()
            .iter()
            .all(|node| !node.interactive)
    );
}

#[test]
fn custom_layouts_receive_authored_sibling_keys_not_renderer_ids() {
    let a = Key::new("item", 1);
    let b = Key::new("item", 2);
    let layout = Anchored::new(
        AnchorPoint::TOP_LEADING,
        vec![Attachment {
            child: b,
            target: AnchorTarget::Child(a),
            target_point: AnchorPoint::BOTTOM_TRAILING,
            child_point: AnchorPoint::TOP_LEADING,
            offset: (5.0, 7.0),
        }],
    )
    .unwrap();
    let tree = root(LayoutView::new(layout, (square().id(a), square().id(b))).frame(200.0, 150.0));
    assert_eq!(
        fills(&tree),
        [
            Bounds::new(80.0, 60.0, 40.0, 30.0),
            Bounds::new(125.0, 97.0, 40.0, 30.0)
        ]
    );
}

struct A;
struct B;
impl LayoutValueKey for A {
    type Value = f32;
    fn default_value() -> f32 {
        0.0
    }
}
impl LayoutValueKey for B {
    type Value = f32;
    fn default_value() -> f32 {
        0.0
    }
}

#[derive(PartialEq)]
struct Placer;
impl Layout for Placer {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(LayoutSize::new(100.0, 100.0))
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let size = children.measure(0, Proposal::IDEAL)?;
        children.place(
            0,
            Bounds::new(
                bounds.x + children.value::<A>(0)?,
                bounds.y + children.value::<B>(0)?,
                size.width,
                size.height,
            ),
        )
    }
}

#[derive(PartialEq)]
struct Composite;
impl View for Composite {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        square().layout_value::<A>(2.0)
    }
}

#[test]
fn layout_values_compose_across_body_boundaries_without_extra_layout_boxes() {
    let tree = root(LayoutView::new(
        Placer,
        (Composite.layout_value::<A>(12.0).layout_value::<B>(18.0),),
    ));
    assert_eq!(fills(&tree), [Bounds::new(12.0, 18.0, 40.0, 30.0)]);
}
