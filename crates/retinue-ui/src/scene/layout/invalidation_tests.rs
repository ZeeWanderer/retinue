use super::*;
use crate::{
    Bounds,
    layout::{
        Layout as LayoutProtocol, LayoutError, LayoutSize, Proposal, Subviews,
        alignment::{AlignedStack, AlignmentGuide},
        anchors::AnchorPoint,
        layered::Layered,
    },
    scene::{Change, RecordingMode, Scene},
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq)]
struct Leaf {
    size: LayoutSize,
    revision: u32,
    baseline: f32,
}

impl LayoutProtocol for Leaf {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(self.size)
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
    fn explicit_alignment(
        &self,
        guide: AlignmentGuide,
        _: Bounds,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<Option<f32>, LayoutError> {
        Ok(guide
            .is::<crate::view::FirstTextBaseline>()
            .then_some(self.baseline))
    }
}

#[derive(Clone, PartialEq)]
struct Counted {
    updates: Rc<Cell<usize>>,
    measurements: Rc<Cell<usize>>,
    placements: Rc<Cell<usize>>,
}

impl Counted {
    fn new() -> Self {
        Self {
            updates: Rc::default(),
            measurements: Rc::default(),
            placements: Rc::default(),
        }
    }
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.updates.get(),
            self.measurements.get(),
            self.placements.get(),
        )
    }
}

impl LayoutProtocol for Counted {
    type Cache = ();
    fn update_cache(&self, _: &mut (), _: &mut Subviews<'_, '_>) -> Result<(), LayoutError> {
        self.updates.set(self.updates.get() + 1);
        Ok(())
    }
    fn measure(
        &self,
        p: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        self.measurements.set(self.measurements.get() + 1);
        Layered {
            alignment: AnchorPoint::CENTER,
        }
        .measure(p, children, cache)
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut (),
    ) -> Result<(), LayoutError> {
        self.placements.set(self.placements.get() + 1);
        Layered {
            alignment: AnchorPoint::CENTER,
        }
        .place(bounds, children, cache)
    }
}

fn key(id: u64) -> Key {
    Key::new("incremental.layout", id)
}

fn leaf(id: u64, width: f32) -> Element {
    let mut element = Element::new(key(id), Style::default());
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Leaf {
        size: LayoutSize::new(width, 20.0),
        revision: 0,
        baseline: 10.0,
    }));
    element
}

fn counted(id: u64, counter: &Counted, children: Vec<Element>) -> Element {
    let mut element = Element::new(key(id), Style::default()).with_children(children);
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::owner(counter.clone()));
    element
}

fn change(width: f32, revision: u32) -> Change {
    Change::Layout(
        key(3),
        Style::default(),
        Algorithm::Custom(LayoutAlgorithm::new(Leaf {
            size: LayoutSize::new(width, 20.0),
            revision,
            baseline: 10.0,
        })),
    )
}

#[test]
fn unchanged_intrinsic_size_does_not_execute_parent_or_siblings_without_a_frame() {
    let parent = Counted::new();
    let sibling = Counted::new();
    let root = counted(
        1,
        &parent,
        vec![leaf(3, 40.0), counted(4, &sibling, vec![leaf(5, 80.0)])],
    );
    let mut scene = Scene::default();
    scene
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let old = (parent.counts(), sibling.counts());
    let bounds = scene.bounds(key(3)).unwrap().layout;
    for revision in 1..5 {
        scene.patch([change(40.0, revision)]).unwrap();
        assert_eq!((parent.counts(), sibling.counts()), old);
        assert_eq!(scene.bounds(key(3)).unwrap().layout, bounds);
        assert!(scene.stats.layout_computations <= 3, "{:?}", scene.stats);
    }
}

#[test]
fn changed_size_repositions_inside_fixed_frame_without_executing_ancestors() {
    let ancestor = Counted::new();
    let frame = Counted::new();
    let sibling = Counted::new();
    let mut framed = counted(2, &frame, vec![leaf(3, 40.0)]);
    framed.style.size = Size::from_lengths(100.0, 40.0);
    let root = counted(
        1,
        &ancestor,
        vec![framed, counted(4, &sibling, vec![leaf(5, 80.0)])],
    );
    let mut scene = Scene::default();
    scene
        .update(root.clone(), (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let old = (ancestor.counts(), sibling.counts());
    let frame_bounds = scene.bounds(key(2)).unwrap().layout;
    for (revision, width) in [70.0, 10.0, 120.0, 40.0].into_iter().enumerate() {
        scene.patch([change(width, revision as u32 + 1)]).unwrap();
        assert_eq!((ancestor.counts(), sibling.counts()), old);
        assert_eq!(scene.bounds(key(2)).unwrap().layout, frame_bounds);
        let bounds = scene.bounds(key(3)).unwrap().layout;
        assert_eq!(bounds.width, width);
        assert_eq!(bounds.x, frame_bounds.x + (100.0 - width) / 2.0);
        assert!(scene.stats.layout_computations <= 6, "{:?}", scene.stats);
    }
}

#[test]
fn intrinsic_change_relayouts_parent_and_reproposes_space_without_invalidating_sibling_inputs() {
    let parent = Counted::new();
    let sibling = Counted::new();
    let root = counted(
        1,
        &parent,
        vec![leaf(3, 40.0), counted(4, &sibling, vec![leaf(5, 20.0)])],
    );
    let mut scene = Scene::default();
    scene
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let old = (parent.counts(), sibling.counts());
    scene.patch([change(70.0, 1)]).unwrap();
    assert!(parent.counts().0 > old.0.0);
    assert_eq!(sibling.updates.get(), old.1.0);
    assert!(sibling.measurements.get() > old.1.1);
    assert_eq!(scene.bounds(key(1)).unwrap().layout.width, 70.0);
}

#[test]
fn baseline_dependencies_propagate_even_when_measured_size_is_unchanged() {
    let mut root =
        Element::new(key(1), Style::default()).with_children([leaf(3, 40.0), leaf(4, 20.0)]);
    root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::row::<
        crate::view::FirstTextBaseline,
    >(5.0)));
    let mut scene = Scene::default();
    scene
        .update(root.clone(), (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let old = scene.bounds(key(4)).unwrap().layout;
    let root_slot = scene.tree.slot(key(1)).unwrap();
    let revision = scene.tree.state.nodes[root_slot].as_ref().unwrap().revision;
    let leaf_slot = scene.tree.slot(key(3)).unwrap();
    let queries = &scene.tree.state.nodes[leaf_slot]
        .as_ref()
        .unwrap()
        .dependencies;
    let local_queries = queries.measurements.len() + queries.alignments.len() + 1;
    scene.patch([change(40.0, 1)]).unwrap();
    assert!(
        scene.stats.layout_computations <= local_queries,
        "{:?}",
        scene.stats
    );
    assert_eq!(
        scene.tree.state.nodes[root_slot].as_ref().unwrap().revision,
        revision
    );
    assert_eq!(scene.bounds(key(4)).unwrap().layout, old);
    let updated = Algorithm::Custom(LayoutAlgorithm::new(Leaf {
        size: LayoutSize::new(40.0, 20.0),
        revision: 2,
        baseline: 14.0,
    }));
    scene
        .patch([Change::Layout(key(3), Style::default(), updated.clone())])
        .unwrap();
    assert_eq!(scene.bounds(key(4)).unwrap().layout.y, old.y + 4.0);
    root.children[0].algorithm = updated;
    let mut fresh = Scene::default();
    fresh
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    for (actual, expected) in scene.geometry().iter().zip(fresh.geometry()) {
        assert_eq!(actual.layout, expected.layout);
    }
}

#[test]
fn width_and_margin_changes_notify_the_parent_but_stop_at_fixed_ancestor() {
    let ancestor = Counted::new();
    let frame = Counted::new();
    let mut child = leaf(3, 40.0);
    let mut framed = counted(2, &frame, vec![child.clone()]);
    framed.style.size = Size::from_lengths(100.0, 40.0);
    let root = counted(1, &ancestor, vec![framed]);
    let mut scene = Scene::default();
    scene
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let before = ancestor.counts();
    child.style.size.width = taffy::Dimension::length(70.0);
    scene
        .patch([Change::Layout(
            child.key,
            child.style.clone(),
            child.algorithm.clone(),
        )])
        .unwrap();
    assert_eq!(ancestor.counts(), before);
    assert_eq!(scene.bounds(key(3)).unwrap().layout.width, 70.0);
    assert_eq!(scene.bounds(key(3)).unwrap().layout.x, 15.0);

    let mut flex = Element::row(key(2), 0.0, vec![leaf(3, 20.0), leaf(4, 20.0)]);
    flex.style.size = Size::from_lengths(100.0, 40.0);
    let root = counted(1, &ancestor, vec![flex]);
    scene
        .update(root.clone(), (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let before = ancestor.counts();
    let old_x = scene.bounds(key(4)).unwrap().layout.x;
    let mut child = root.children[0].children[0].clone();
    child.style.margin.right = taffy::LengthPercentageAuto::length(10.0);
    scene
        .patch([Change::Layout(child.key, child.style, child.algorithm)])
        .unwrap();
    assert_eq!(ancestor.counts(), before);
    assert_eq!(scene.bounds(key(4)).unwrap().layout.x, old_x + 10.0);
}

#[test]
fn failed_incremental_measurement_does_not_poison_the_next_update() {
    let root = counted(1, &Counted::new(), vec![leaf(3, 40.0)]);
    let mut scene = Scene::default();
    scene
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    assert!(scene.patch([change(f32::NAN, 1)]).is_err());
    assert!(scene.layout_failure().is_some());
    scene.patch([change(70.0, 2)]).unwrap();
    assert!(scene.layout_failure().is_none());
    assert_eq!(scene.bounds(key(1)).unwrap().layout.width, 70.0);
    let root_slot = scene.tree.slot(key(1)).unwrap();
    let revision = scene.tree.state.nodes[root_slot].as_ref().unwrap().revision;
    scene.patch([change(70.0, 3)]).unwrap();
    assert_eq!(
        scene.tree.state.nodes[root_slot].as_ref().unwrap().revision,
        revision
    );
}

#[test]
fn untracked_proposals_conservatively_propagate_instead_of_reusing_stale_layout() {
    use crate::layout::Available;

    #[derive(PartialEq)]
    struct Responsive(f32);
    impl LayoutProtocol for Responsive {
        type Cache = ();
        fn measure(
            &self,
            proposal: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(
                10.0,
                if proposal.width == Available::Limit(20.0) {
                    self.0
                } else {
                    10.0
                },
            ))
        }
        fn place(
            &self,
            _: Bounds,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            Ok(())
        }
    }

    #[derive(PartialEq)]
    struct Probe;
    impl LayoutProtocol for Probe {
        type Cache = ();
        fn measure(
            &self,
            _: Proposal,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            let mut size = LayoutSize::ZERO;
            for width in 1..=20 {
                size = children.measure(
                    0,
                    Proposal {
                        width: Available::Limit(width as f32),
                        height: Available::Ideal,
                    },
                )?;
            }
            Ok(size)
        }
        fn place(
            &self,
            _: Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            children.place(0, Bounds::new(0.0, 0.0, 10.0, 10.0))
        }
    }

    let mut child = leaf(3, 10.0);
    child.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Responsive(10.0)));
    let mut root = Element::new(key(1), Style::default()).with_children([child]);
    root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Probe));
    let mut scene = Scene::default();
    scene
        .update(root, (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let slot = scene.tree.slot(key(3)).unwrap();
    let dependencies = &scene.tree.state.nodes[slot].as_ref().unwrap().dependencies;
    assert!(dependencies.overflowed);
    assert_eq!(dependencies.measurements.len(), 16);
    scene
        .patch([Change::Layout(
            key(3),
            Style::default(),
            Algorithm::Custom(LayoutAlgorithm::new(Responsive(40.0))),
        )])
        .unwrap();
    assert_eq!(scene.bounds(key(1)).unwrap().layout.height, 40.0);
}
