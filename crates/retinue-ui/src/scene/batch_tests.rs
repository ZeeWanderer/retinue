use super::*;

fn box_at(id: u64, x: f32, color: u32) -> Element {
    let mut element = Element::at(id, Bounds::new(x, 0.0, 20.0, 20.0));
    element.paint = Some(Paint::new(
        PaintKey::new(id.into(), color as u64),
        move |size| {
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                radius: 0.0,
                color: Color::rgb(color),
            }]
        },
    ));
    element
}

fn scene() -> Scene {
    let mut scene = Scene::default();
    scene
        .update(
            Element::at(0, Bounds::new(0.0, 0.0, 200.0, 120.0))
                .with_children([box_at(1, 0.0, 0xff0000), box_at(2, 40.0, 0x00ff00)]),
            (200, 120),
            1.0,
            RecordingMode::Retained,
        )
        .unwrap();
    scene
}

#[test]
fn subtree_batch_validates_every_patch_before_changing_anything() {
    let mut scene = scene();
    let generation = scene.frame().generation();
    assert_eq!(
        scene.update_subtrees([box_at(1, 0.0, 0xffffff), box_at(9, 0.0, 0)]),
        Err("unknown composition key")
    );
    assert_eq!(scene.frame().generation(), generation);
    assert_eq!(scene.paint(1.into()).unwrap().key.revision, 0xff0000);
}

#[test]
fn batch_rejects_overlapping_roots_and_cross_subtree_keys() {
    let mut scene = scene();
    let root = Element::at(0, Bounds::new(0.0, 0.0, 200.0, 120.0))
        .with_children([box_at(1, 0.0, 1), box_at(2, 40.0, 2)]);
    assert_eq!(
        scene.update_subtrees([root, box_at(1, 0.0, 3)]),
        Err("overlapping composition subtrees")
    );
    assert_eq!(
        scene.update_subtrees([box_at(1, 0.0, 3).with_children([box_at(2, 0.0, 4)])]),
        Err("duplicate or out-of-scope composition key")
    );
    assert_eq!(
        scene.update_subtrees([
            box_at(1, 0.0, 3).with_children([box_at(5, 0.0, 5)]),
            box_at(2, 0.0, 4).with_children([box_at(5, 0.0, 5)]),
        ]),
        Err("duplicate or out-of-scope composition key")
    );
}

#[test]
fn batch_keeps_both_updates_in_one_generation() {
    let mut scene = scene();
    let generation = scene.frame().generation();
    scene
        .update_subtrees([box_at(1, 0.0, 0x112233), box_at(2, 40.0, 0x332211)])
        .unwrap();
    assert_eq!(scene.frame().generation(), generation + 1);
    assert_eq!(scene.paint(1.into()).unwrap().key.revision, 0x112233);
    assert_eq!(scene.paint(2.into()).unwrap().key.revision, 0x332211);
    assert_eq!(scene.stats.layout_computations, 0);
    assert_eq!(scene.update_subtrees([]).unwrap(), Damage::default());
    assert_eq!(scene.frame().generation(), generation + 1);
}

#[test]
fn retained_replacements_validate_scope_and_keep_recordings() {
    use layout::Subtree;
    let mut scene = scene();
    let root = || Element::at(0, Bounds::new(0.0, 0.0, 200.0, 120.0));
    let patch = |children| Subtree::Replace(Box::new(root()), children);
    let generation = scene.frame().generation();
    for children in [
        vec![Subtree::Retained(1.into()), Subtree::Retained(1.into())],
        vec![Subtree::Retained(9.into())],
        vec![Subtree::Retained(0.into())],
    ] {
        assert!(
            scene
                .update_retained_subtrees_at(vec![patch(children)], (200, 120), 1.0)
                .is_err()
        );
        assert_eq!(scene.frame().generation(), generation);
    }
    assert!(
        scene
            .update_retained_subtrees_at(
                vec![Subtree::Replace(
                    Box::new(box_at(1, 0.0, 0)),
                    vec![Subtree::Retained(2.into())]
                )],
                (200, 120),
                1.0
            )
            .is_err()
    );
    scene
        .update_retained_subtrees_at(
            vec![patch(vec![
                Subtree::Retained(2.into()),
                box_at(3, 80.0, 0x778899).into(),
                Subtree::Retained(1.into()),
            ])],
            (200, 120),
            1.0,
        )
        .unwrap();
    assert_eq!(scene.stats.recordings, 1);
    assert_eq!(scene.paint(1.into()).unwrap().key.revision, 0xff0000);
    scene
        .update_retained_subtrees_at(
            vec![patch(vec![Subtree::Retained(2.into())])],
            (200, 120),
            1.0,
        )
        .unwrap();
    assert!(!scene.contains(1.into()));
    assert!(!scene.contains(3.into()));
    assert_eq!(scene.stats.recordings, 0);
}

#[test]
fn retained_branches_cannot_alias_replaced_descendants_in_either_order() {
    use layout::Subtree;
    let mut scene = scene();
    scene
        .update_subtree(box_at(1, 0.0, 0xff0000).with_children([box_at(3, 5.0, 0x112233)]))
        .unwrap();
    let generation = scene.frame().generation();
    for reverse in [false, true] {
        let mut children = vec![
            Subtree::Retained(1.into()),
            box_at(3, 20.0, 0x778899).into(),
        ];
        if reverse {
            children.reverse();
        }
        let root = Element::at(0, Bounds::new(0.0, 0.0, 200.0, 120.0));
        assert!(
            scene
                .update_retained_subtrees_at(
                    vec![Subtree::Replace(Box::new(root), children)],
                    (200, 120),
                    1.0
                )
                .is_err()
        );
        assert_eq!(scene.frame().generation(), generation);
        assert_eq!(scene.bounds(3.into()).unwrap().parent, Some(1.into()));
    }
    scene
        .update_retained_subtrees_at(
            vec![Subtree::Replace(
                Box::new(Element::at(0, Bounds::new(0.0, 0.0, 200.0, 120.0))),
                vec![Subtree::Replace(
                    Box::new(box_at(2, 10.0, 0)),
                    vec![Subtree::Retained(1.into())],
                )],
            )],
            (200, 120),
            1.0,
        )
        .unwrap();
    assert_eq!(scene.bounds(1.into()).unwrap().parent, Some(2.into()));
    assert_eq!(scene.bounds(3.into()).unwrap().parent, Some(1.into()));
    assert_eq!(scene.paint(3.into()).unwrap().key.revision, 0x112233);
}
