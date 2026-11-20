use super::*;
use crate::{
    Bounds,
    layout::{Insets, anchors::AnchorPoint},
    view as reference,
};
use modifiers::{matches_reference, pixels};

#[test]
fn layered_alignment_places_every_physical_root_in_a_group() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount_body(|_| {
        ui! {
            ZStack() {
                Group() {
                    Rectangle(Color::rgb(0xff0000)).frame(Frame::new().width(80.0).height(40.0))
                    Rectangle(Color::rgb(0x00ff00)).frame(Frame::new().width(20.0).height(10.0))
                }
            }.alignment(AnchorPoint::TOP_TRAILING).frame(Frame::new().width(80.0).height(40.0))
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let address = |local| Address {
        instance: root,
        local: Local(local),
    };
    assert_eq!(
        graph.geometry(address(3)).unwrap().layout,
        Bounds::new(0.0, 0.0, 80.0, 40.0)
    );
    assert_eq!(
        graph.geometry(address(5)).unwrap().layout,
        Bounds::new(60.0, 0.0, 20.0, 10.0)
    );
}

#[test]
fn shapes_padding_and_modifier_order_match_direct_layout() {
    matches_reference(
        Text("Padded")
            .padding(9.0)
            .background(Rectangle(Color::rgb(0x406080)).corner_radius(5.0)),
        reference::ViewExt::background(
            reference::ViewExt::padding(Text("Padded"), 9.0),
            Rectangle(Color::rgb(0x406080)).corner_radius(5.0),
        ),
    );
    matches_reference(
        Text("Padded")
            .background(Rectangle(Color::rgb(0x406080)).corner_radius(5.0))
            .padding(9.0),
        reference::ViewExt::padding(
            reference::ViewExt::background(
                Text("Padded"),
                Rectangle(Color::rgb(0x406080)).corner_radius(5.0),
            ),
            9.0,
        ),
    );
    let overlay = ui! {
        ZStack() {
            Rectangle(Color::rgb(0x406080)).corner_radius(5.0).frame(Frame::new().width(140.0).height(60.0))
            Text("Centered")
        }
    };
    matches_reference(
        overlay,
        reference::ZStack::new([
            reference::ViewExt::frame(
                Rectangle(Color::rgb(0x406080)).corner_radius(5.0),
                140.0,
                60.0,
            ),
            reference::IntoElement::into_element(Text("Centered")),
        ]),
    );
}

fn rows() -> impl Body {
    ui! {
        VStack(0.0) {
            Rectangle(Color::rgb(0xc02030)).frame(Frame::new().width(70.0).height(40.0))
            Rectangle(Color::rgb(0x30c020)).frame(Frame::new().width(70.0).height(40.0))
            Rectangle(Color::rgb(0x2030c0)).frame(Frame::new().width(70.0).height(40.0))
        }
    }
}

#[test]
fn viewport_scrolling_changes_only_translation_and_keeps_clip_and_insets() {
    let mut graph = InstanceGraph::default();
    let position = State::new(0.0);
    let read = position.clone();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    ScrollViewport() { rows() }
                        .position(0.0, read.get())
                        .content_insets(Insets { left: 3.0, top: 4.0, right: 5.0, bottom: 9.0 })
                        .frame(Frame::new().width(78.0).height(65.0))
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    for scale in [1.0, 1.2, 1.5] {
        let size = ((180.0 * scale) as u32, (100.0 * scale) as u32);
        for y in [0.0, 17.0, 68.0, 8.0] {
            position.set(y);
            let update = graph.advance(size, scale, None).unwrap();
            assert!(update.completion.unwrap().is_ready());
            if y != 0.0 {
                assert_eq!(update.work.bindings_emitted, 1);
                assert_eq!(graph.scene().stats.layout_computations, 0);
            }
            let direct = reference::VStack::new(
                0.0,
                [0xc02030, 0x30c020, 0x2030c0].map(|color| {
                    reference::ViewExt::frame(Rectangle(Color::rgb(color)), 70.0, 40.0)
                }),
            );
            let viewport = reference::ScrollViewport::new(direct)
                .position(0.0, y)
                .content_insets(Insets {
                    left: 3.0,
                    top: 4.0,
                    right: 5.0,
                    bottom: 9.0,
                });
            let mut tree = crate::runtime::ViewTree::new(
                reference::ViewExt::frame(viewport, 78.0, 65.0),
                || {},
            );
            tree.update(size, scale).unwrap();
            assert!(
                pixels(graph.scene()) == pixels(tree.scene()),
                "scale={scale}, scroll={y}"
            );
        }
    }
}

#[test]
fn a_frame_can_contain_empty_or_multiple_transparent_children() {
    let mut graph = InstanceGraph::default();
    let selected = State::new(false);
    let read = selected.clone();
    let root = graph.mount_body(move |_| {
        ui! {
            Group() {
                if read.get() {
                    Rectangle(Color::rgb(0xff0000)).frame(Frame::new().width(50.0).height(20.0))
                }
                if read.get() {
                    Rectangle(Color::rgb(0x00ff00)).frame(Frame::new().width(10.0).height(20.0))
                }
            }.frame(Frame::new().width(80.0).height(40.0))
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    selected.set(true);
    advance(&mut graph);
    assert_eq!(graph.scene().frame().instances().len(), 2);
    selected.set(false);
    advance(&mut graph);
    assert!(graph.scene().frame().instances().is_empty());
}

#[test]
fn backgrounds_keep_dynamic_group_roots_together() {
    let mut graph = InstanceGraph::default();
    let selected = State::new(true);
    let read = selected.clone();
    let root = graph.mount_body(move |_| {
        ui! {
            Group() {
                if read.get() {
                    Rectangle(Color::rgb(0xc02030)).frame(Frame::new().width(70.0).height(40.0))
                    Rectangle(Color::rgb(0x30c020)).frame(Frame::new().width(20.0).height(10.0))
                }
            }.background { Rectangle(Color::rgb(0x2030c0)) }
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    for visible in [true, false, true] {
        selected.set(visible);
        advance(&mut graph);
        let children: Vec<_> = if visible {
            vec![
                reference::ViewExt::frame(Rectangle(Color::rgb(0xc02030)), 70.0, 40.0),
                reference::ViewExt::frame(Rectangle(Color::rgb(0x30c020)), 20.0, 10.0),
            ]
        } else {
            vec![]
        };
        let expected = reference::ViewExt::background(
            reference::ZStack::new(children),
            Rectangle(Color::rgb(0x2030c0)),
        );
        let mut tree = crate::runtime::ViewTree::new(expected, || {});
        tree.update((400, 300), 1.0).unwrap();
        assert!(pixels(graph.scene()) == pixels(tree.scene()));
    }
}
#[test]
fn positioned_stack_updates_child_placement_without_rebinding_the_parent() {
    let key = crate::Key::new("placed", 0);
    let slot = State::new(crate::Bounds::new(2.0, 3.0, 10.0, 15.0));
    let input = slot.clone();
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    PositionedStack(crate::layout::LayoutSize::new(200.0, 80.0)) {
                        Rectangle(Color::rgb(0)).placed(input.get()).named(key)
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let address = graph.unique(key).unwrap().unwrap();
    slot.set(crate::Bounds::new(5.0, 9.0, 40.0, 17.0));
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    assert_eq!(graph.unique(key).unwrap(), Some(address));
    assert_eq!(graph.geometry(address).unwrap().layout, slot.get());
}

#[test]
fn parent_frames_move_retained_keyed_children_without_masking_their_own_changes() {
    use crate::{Key, layout::LayoutSize};
    use std::cell::Cell;
    const FIRST: Key = Key::new("child", 1);
    const SECOND: Key = Key::new("child", 2);
    let fallback = Bounds::new(4.0, 6.0, 11.0, 10.0);

    #[derive(Clone, PartialEq)]
    struct Parts(State<bool>, Rc<Cell<usize>>, Bounds);
    #[view]
    impl View for Parts {
        fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
            self.1.set(self.1.get() + 1);
            let order = if self.0.get() {
                [SECOND, FIRST]
            } else {
                [FIRST, SECOND]
            };
            ForEach(
                order,
                |key| *key,
                |key| {
                    Rectangle(Color::rgb(0))
                        .placed(self.2)
                        .interactive()
                        .named(key)
                },
            )
        }
    }

    let parts = Parts(State::new(false), Rc::default(), fallback);
    let slot = State::new(Some(Bounds::new(17.0, 8.0, 12.0, 10.0)));
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            {
                let parts = parts.clone();
                let slot = slot.clone();
                move |_| {
                    PositionedStack(LayoutSize::new(90.0, 60.0), (compose(parts.clone()),))
                        .child_frames(slot.get().map(|bounds| (FIRST, bounds)))
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let first = graph.unique(FIRST).unwrap();
    assert_eq!(
        graph.named_bounds(FIRST).unwrap().unwrap().layout,
        slot.get().unwrap()
    );
    slot.set(Some(Bounds::new(40.0, 9.0, 16.0, 14.0)));
    advance(&mut graph);
    assert_eq!(graph.hit_test((41.0, 10.0)), Some(FIRST));
    assert_eq!(graph.hit_test((18.0, 9.0)), None);
    assert_eq!(parts.1.get(), 1);
    parts.0.set(true);
    advance(&mut graph);
    assert_eq!(parts.1.get(), 2);
    assert_eq!(graph.unique(FIRST).unwrap(), first);
    assert_eq!(
        graph.named_bounds(FIRST).unwrap().unwrap().layout,
        slot.get().unwrap()
    );
    assert_eq!(
        graph.named_bounds(SECOND).unwrap().unwrap().layout,
        fallback
    );
    slot.set(None);
    advance(&mut graph);
    assert_eq!(graph.named_bounds(FIRST).unwrap().unwrap().layout, fallback);
    assert_eq!(parts.1.get(), 2);
}

#[test]
fn placed_composes_with_parent_read_keys_in_either_order() {
    use crate::layout::{
        Layout, LayoutError, LayoutSize, Proposal, Subviews, positioned::Placement,
        values::LayoutValueKey,
    };
    struct Inset;
    impl LayoutValueKey for Inset {
        type Value = f32;
        fn default_value() -> f32 {
            0.0
        }
    }
    #[derive(PartialEq)]
    struct Parent;
    impl Layout for Parent {
        type Cache = ();
        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> std::result::Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(150.0, 90.0))
        }
        fn place(
            &self,
            _: Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> std::result::Result<(), LayoutError> {
            for i in 0..children.len() {
                let mut r = children.value::<Placement>(i)?.unwrap();
                r.x += children.value::<Inset>(i)?;
                children.place(i, r)?;
            }
            Ok(())
        }
    }
    let slot = State::new(Bounds::new(2.0, 3.0, 20.0, 5.0));
    let inset = State::new(7.0);
    let keys = [crate::Key::new("slot", 0), crate::Key::new("slot", 1)];
    let mut graph = InstanceGraph::default();
    let (r, x) = (slot.clone(), inset.clone());
    let root = graph
        .mount_body(
            move |_| {
                LayoutView::new(
                    Parent,
                    (
                        Rectangle(Color::rgb(0x334455))
                            .placed(r.get())
                            .layout_value::<Inset>(x.get())
                            .named(keys[0]),
                        Rectangle(Color::rgb(0x556677))
                            .layout_value::<Inset>(x.get())
                            .placed(r.get())
                            .named(keys[1]),
                    ),
                )
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    for (rect, dx) in [
        (slot.get(), 7.0),
        (Bounds::new(4.0, 6.0, 20.0, 5.0), 7.0),
        (Bounds::new(4.0, 6.0, 25.0, 9.0), 7.0),
        (Bounds::new(4.0, 6.0, 25.0, 9.0), 3.0),
    ] {
        slot.set(rect);
        inset.set(dx);
        advance(&mut graph);
        for key in keys {
            let actual = graph
                .geometry(graph.unique(key).unwrap().unwrap())
                .unwrap()
                .layout;
            assert_eq!(
                actual,
                Bounds::new(rect.x + dx, rect.y, rect.width, rect.height)
            );
        }
    }
}
