use super::*;
use retinue_render::{RasterMode, Renderer, ResidencyPolicy, painter::Painter};

#[test]
fn collection_edits_keep_unchanged_scene_subtrees() {
    #[derive(Clone, PartialEq)]
    struct Rows {
        order: State<Vec<u64>>,
        changed: State<u32>,
    }
    impl View for Rows {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            VStack::new(
                2.0,
                self.order
                    .get()
                    .into_iter()
                    .map(|id| {
                        if id == 3 {
                            Counter {
                                value: self.changed.clone(),
                                calls: Rc::default(),
                            }
                            .into_element()
                        } else {
                            tile(0x668899 + id as u32)
                        }
                        .id(id)
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }
    for scale in [1.0, 1.25, 2.0] {
        let view = Rows {
            order: State::new((0..24).collect()),
            changed: State::new(0x227733),
        };
        let mut tree = ViewTree::new(view.clone(), || {});
        let size = ((200.0 * scale) as u32, (650.0 * scale) as u32);
        tree.update(size, scale).unwrap();
        let id = tree.unique(3).unwrap().unwrap();
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        for order in [
            (0..25).collect(),
            (0..25).rev().collect(),
            (3..18).collect(),
        ] {
            view.order.set(order);
            view.changed.set(view.changed.get() + 1);
            let damage = tree.update(size, scale).unwrap();
            assert!(
                tree.stats.projected <= 6,
                "{} projected nodes",
                tree.stats.projected
            );
            assert!(tree.scene().stats.recordings <= 2);
            assert_eq!(tree.unique(3).unwrap(), Some(id));
            paint(&tree, &mut renderer, &mut pixels, damage);
            let mut fresh = ViewTree::new(view.clone(), || {});
            fresh.update(size, scale).unwrap();
            let mut expected = vec![0; pixels.len()];
            paint(
                &fresh,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            assert!(pixels == expected, "retained collection at {scale}");
        }
    }
}

fn paint(tree: &ViewTree, renderer: &mut Renderer, pixels: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(pixels, size.0, size.1, 2.try_into().unwrap()).unwrap();
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x171717));
    }
    painter.finish().unwrap();
}

#[test]
fn several_changed_bodies_commit_once_and_partial_repair_matches_a_fresh_frame() {
    for scale in [1.0, 1.25, 2.0] {
        let a = counter(0xaa1122);
        let b = counter(0x11aa22);
        let c = counter(0x1122aa);
        let mut tree = ViewTree::new(HStack::new(8.0, (a.clone(), b.clone(), c.clone())), || {});
        let size = ((200.0 * scale) as u32, (120.0 * scale) as u32);
        tree.update(size, scale).unwrap();
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        let generation = tree.frame().generation();
        a.value.set(0xaaaa00);
        c.value.set(0xffffff);
        let damage = tree.update(size, scale).unwrap();
        assert_eq!(tree.stats.bodies, 2);
        assert_eq!(tree.stats.projected, 2);
        assert_eq!(tree.frame().generation(), generation + 1);
        assert_eq!(tree.scene().stats.layout_computations, 0);
        paint(&tree, &mut renderer, &mut pixels, damage);
        let mut fresh = vec![0; pixels.len()];
        paint(
            &tree,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            &mut fresh,
            Damage::Full,
        );
        assert!(pixels == fresh, "batched repair differs at scale {scale}");
        assert_eq!(b.calls.get(), 1);
    }
}

#[test]
fn resize_reuses_bodies_and_environment_wrappers_do_not_change_layout() {
    let view = counter(0x667788);
    let mut tree = ViewTree::new(view.clone(), || {});
    update(&mut tree);
    let bounds = tree.frame().instances()[0].visible_bounds();
    assert_eq!((bounds.width, bounds.height), (24, 20));
    tree.update((400, 240), 2.0).unwrap();
    assert_eq!(view.calls.get(), 1);
    assert_eq!(tree.stats.bodies, 0);
    let bounds = tree.frame().instances()[0].visible_bounds();
    assert_eq!((bounds.width, bounds.height), (48, 40));
    let mut wrapped = ViewTree::new(view.environment_model(State::new(5)), || {});
    wrapped.update((400, 240), 2.0).unwrap();
    assert_eq!(wrapped.frame().instances()[0].visible_bounds(), bounds);
}

#[test]
fn simultaneous_viewport_and_property_changes_lower_only_the_changed_owner() {
    let child = counter(0x667788);
    let outer = || {
        VStack::new(
            3.0,
            (
                Text::new("Static header"),
                child.clone(),
                Text::new("Static footer"),
            ),
        )
    };
    let mut tree = ViewTree::new(outer(), || {});
    tree.update((400, 240), 1.0).unwrap();
    for (size, scale, color) in [
        ((500, 300), 1.0, 0x33ee44),
        ((520, 300), 1.25, 0xee3344),
        ((800, 480), 2.0, 0x3344ee),
    ] {
        child.value.set(color);
        tree.update(size, scale).unwrap();
        assert_eq!(tree.stats.bodies, 1);
        assert_eq!(tree.stats.projected, 1);
        let mut fresh = ViewTree::new(outer(), || {});
        fresh.update(size, scale).unwrap();
        let mut actual = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut expected = actual.clone();
        paint(
            &tree,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            &mut actual,
            Damage::Full,
        );
        paint(
            &fresh,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            &mut expected,
            Damage::Full,
        );
        assert!(actual == expected);
    }
}

#[test]
fn simultaneous_resize_and_branch_replacement_matches_a_fresh_tree() {
    #[derive(PartialEq)]
    struct Branch(State<bool>);
    impl View for Branch {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            if self.0.get() {
                Text::new("Expanded content").into_element()
            } else {
                empty()
            }
        }
    }
    let active = State::new(false);
    let root = || {
        VStack::new(
            4.0,
            (Text::new("Header"), Branch(active.clone()), tile(0x667788)),
        )
    };
    let mut tree = ViewTree::new(root(), || {});
    tree.update((200, 160), 1.0).unwrap();
    for (size, scale, expanded) in [
        ((220, 180), 1.0, true),
        ((320, 240), 1.25, false),
        ((400, 300), 2.0, true),
    ] {
        active.set(expanded);
        tree.update(size, scale).unwrap();
        let mut fresh = ViewTree::new(root(), || {});
        fresh.update(size, scale).unwrap();
        let mut actual = vec![0; size.0 as usize * size.1 as usize * 4];
        let mut expected = actual.clone();
        paint(
            &tree,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            &mut actual,
            Damage::Full,
        );
        paint(
            &fresh,
            &mut Renderer::new(ResidencyPolicy::desktop(0)),
            &mut expected,
            Damage::Full,
        );
        assert!(actual == expected);
    }
}

#[test]
fn failed_custom_layout_keeps_the_published_frame_and_retries_corrected_inputs() {
    #[derive(PartialEq)]
    struct Failable(bool);
    impl Layout for Failable {
        type Cache = ();
        fn measure(
            &self,
            proposal: Proposal,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            if self.0 {
                Err(LayoutError::InvalidGeometry)
            } else {
                children.measure(0, proposal)
            }
        }
        fn place(
            &self,
            bounds: crate::Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            children.place(0, bounds)
        }
    }
    #[derive(PartialEq)]
    struct Root(State<bool>);
    impl View for Root {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            LayoutView::new(
                Failable(self.0.get()),
                (Text::new("Retained through a layout error"),),
            )
        }
    }
    let broken = State::new(false);
    let mut tree = ViewTree::new(Root(broken.clone()), || {});
    tree.update((300, 180), 1.0).unwrap();
    let generation = tree.frame().generation();
    let program = tree.frame().instances()[0].program.id();
    broken.set(true);
    assert!(tree.update((400, 240), 1.25).is_err());
    assert_eq!(tree.frame().generation(), generation);
    assert_eq!(tree.frame().size(), (300, 180));
    assert_eq!(tree.frame().instances()[0].program.id(), program);
    assert!(tree.needs_update());
    assert!(tree.update((400, 240), 1.25).is_err());
    assert_eq!(tree.stats.bodies, 0);
    broken.set(false);
    tree.update((400, 240), 1.25).unwrap();
    assert_eq!(tree.frame().generation(), generation + 1);
    assert!(!tree.needs_update());
    assert!(tree.scene().layout_failure().is_none());
    let mut fresh = ViewTree::new(Root(broken), || {});
    fresh.update((400, 240), 1.25).unwrap();
    let mut actual = vec![0; 400 * 240 * 4];
    let mut expected = actual.clone();
    paint(
        &tree,
        &mut Renderer::new(ResidencyPolicy::desktop(0)),
        &mut actual,
        Damage::Full,
    );
    paint(
        &fresh,
        &mut Renderer::new(ResidencyPolicy::desktop(0)),
        &mut expected,
        Damage::Full,
    );
    assert!(actual == expected);
}

#[test]
fn same_slot_body_type_changes_keep_a_replaceable_scene_root() {
    #[derive(PartialEq)]
    struct Branch(State<bool>);
    impl View for Branch {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            if self.0.get() {
                tile(0xff0000)
            } else {
                empty()
            }
        }
    }
    let visible = State::new(true);
    let mut tree = ViewTree::new(Branch(visible.clone()), || {});
    update(&mut tree);
    let root = tree.root().unwrap();
    visible.set(false);
    update(&mut tree);
    assert!(tree.scene().contains(root.key()));
    assert!(tree.frame().instances().is_empty());
    visible.set(true);
    update(&mut tree);
    assert_eq!(tree.frame().instances().len(), 1);
    assert_eq!(tree.root(), Some(root));
}

#[test]
fn replacing_a_zstack_child_preserves_its_parent_placement() {
    let a = counter(0xaa1122);
    let b = counter(0x11aa22);
    let mut tree = ViewTree::new(
        ZStack::new((a.clone(), b.clone())).frame(100.0, 80.0),
        || {},
    );
    update(&mut tree);
    let before: Vec<_> = tree
        .frame()
        .instances()
        .iter()
        .map(|item| item.visible_bounds())
        .collect();
    assert_eq!(before[0], before[1]);
    b.value.set(0xabcdef);
    update(&mut tree);
    let after: Vec<_> = tree
        .frame()
        .instances()
        .iter()
        .map(|item| item.visible_bounds())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn custom_layout_uses_the_same_children_and_retained_cache_contract() {
    #[derive(PartialEq)]
    struct Inset(f32);
    impl Layout for Inset {
        type Cache = ();
        fn measure(
            &self,
            proposal: Proposal,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            let size = children.measure(0, proposal)?;
            Ok(LayoutSize::new(
                size.width + 2.0 * self.0,
                size.height + 2.0 * self.0,
            ))
        }
        fn place(
            &self,
            bounds: crate::Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            let size = children.measure(0, Proposal::IDEAL)?;
            children.place(
                0,
                crate::Bounds::new(
                    bounds.x + self.0,
                    bounds.y + self.0,
                    size.width,
                    size.height,
                ),
            )
        }
    }
    let view = counter(0x667788);
    let mut tree = ViewTree::new(LayoutView::new(Inset(4.0), [view.clone()]), || {});
    update(&mut tree);
    let before = tree.frame().instances()[0].visible_bounds();
    tree.set_root(LayoutView::new(Inset(8.0), [view.clone()]));
    update(&mut tree);
    let after = tree.frame().instances()[0].visible_bounds();
    assert_eq!((after.x - before.x, after.y - before.y), (4, 4));
    assert_eq!((after.width, after.height), (24, 20));
    assert_eq!(view.calls.get(), 1);
    assert_eq!(tree.stats.mounted, 0);
}

#[test]
fn switching_layout_algorithm_preserves_children_and_releases_only_layout_cache() {
    #[derive(Default)]
    struct Cache(Option<Rc<Cell<usize>>>);
    impl Drop for Cache {
        fn drop(&mut self) {
            if let Some(drops) = &self.0 {
                drops.set(drops.get() + 1);
            }
        }
    }
    #[derive(PartialEq)]
    struct First(Rc<Cell<usize>>);
    #[derive(PartialEq)]
    struct Second(Rc<Cell<usize>>);
    macro_rules! layout {
        ($kind:ident, $inset:literal) => {
            impl Layout for $kind {
                type Cache = Cache;
                fn update_cache(
                    &self,
                    cache: &mut Cache,
                    _: &mut Subviews<'_, '_>,
                ) -> Result<(), LayoutError> {
                    cache.0 = Some(self.0.clone());
                    Ok(())
                }
                fn measure(
                    &self,
                    proposal: Proposal,
                    children: &mut Subviews<'_, '_>,
                    _: &mut Cache,
                ) -> Result<LayoutSize, LayoutError> {
                    children.measure(0, proposal)
                }
                fn place(
                    &self,
                    bounds: crate::Bounds,
                    children: &mut Subviews<'_, '_>,
                    _: &mut Cache,
                ) -> Result<(), LayoutError> {
                    children.place(
                        0,
                        crate::Bounds::new(
                            bounds.x + $inset,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                        ),
                    )
                }
            }
        };
    }
    layout!(First, 4.0);
    layout!(Second, 12.0);
    let drops = Rc::new(Cell::new(0));
    let child = counter(0x667788);
    let mut tree = ViewTree::new(
        LayoutView::new(First(drops.clone()), [child.clone().id("child")]),
        || {},
    );
    update(&mut tree);
    let id = tree.unique("child").unwrap().unwrap();
    tree.set_root(LayoutView::new(
        Second(drops.clone()),
        [child.clone().id("child")],
    ));
    update(&mut tree);
    assert_eq!(tree.unique("child").unwrap(), Some(id));
    assert_eq!(tree.stats.mounted, 0);
    assert_eq!(child.calls.get(), 1);
    assert_eq!(tree.scene().stats.recordings, 0);
    assert_eq!(drops.get(), 1);
    assert_eq!(tree.geometry(id).unwrap().layout.x, 12.0);
}

#[test]
fn mixed_reconciliation_and_repair_match_fresh_trees() {
    #[derive(Clone, PartialEq)]
    struct Tile {
        id: u64,
        color: u32,
    }
    impl View for Tile {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            Rectangle::new(Color::rgb(self.color))
                .corner_radius(3.0)
                .frame(18.0 + self.id as f32 * 2.0, 20.0)
        }
    }
    #[derive(Clone, PartialEq)]
    struct Tiles {
        items: State<Vec<(u64, u32)>>,
        vertical: State<bool>,
    }
    impl View for Tiles {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            let children = self
                .items
                .read()
                .iter()
                .map(|&(id, color)| Tile { id, color }.id(id))
                .collect::<Vec<_>>();
            if self.vertical.get() {
                VStack::new(5.0, children)
                    .alignment(Alignment::Start)
                    .into_element()
            } else {
                HStack::new(5.0, children)
                    .alignment(Alignment::Start)
                    .into_element()
            }
        }
    }
    for scale in [1.0, 1.25, 2.0] {
        let model = Tiles {
            items: State::new(vec![]),
            vertical: State::new(false),
        };
        let size = ((200.0 * scale) as u32, (140.0 * scale) as u32);
        let mut tree = ViewTree::new(model.clone(), || {});
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        for step in 0..24u32 {
            let mut items: Vec<_> = (0..step % 6)
                .map(|id| (id as u64, 0x235678 + (step * 771 + id * 3889) % 0x10101))
                .collect();
            if step % 2 == 0 {
                items.reverse();
            }
            model.items.set(items);
            model.vertical.set(step % 8 < 4);
            let damage = tree.update(size, scale).unwrap();
            paint(&tree, &mut renderer, &mut pixels, damage);
            let mut fresh = ViewTree::new(model.clone(), || {});
            fresh.update(size, scale).unwrap();
            let mut expected = vec![0; pixels.len()];
            paint(
                &fresh,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            assert!(
                pixels == expected,
                "incremental tree differs at step {step}, scale {scale}, byte {:?}",
                pixels.iter().zip(&expected).position(|(a, b)| a != b)
            );
        }
    }
}
