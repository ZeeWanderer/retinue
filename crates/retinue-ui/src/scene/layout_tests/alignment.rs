use super::*;
use crate::layout::{
    alignment::{
        AlignedStack, AlignmentGuide, AlignmentId, AlignmentValue, Axis, GuideValue,
        HorizontalCenter, VerticalCenter,
    },
    values::{LayoutValueKey, LayoutValues},
};

struct Offset;
impl LayoutValueKey for Offset {
    type Value = f32;
    fn default_value() -> f32 {
        0.0
    }
}

#[derive(PartialEq)]
struct ValueParent(Tracked);
impl lp::Layout for ValueParent {
    type Cache = Vec<Key>;
    fn update_cache(
        &self,
        cache: &mut Self::Cache,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        lp::Layout::update_cache(&self.0, cache, children)
    }
    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        cache: &mut Self::Cache,
    ) -> Result<LayoutSize, LayoutError> {
        lp::Layout::measure(&self.0, proposal, children, cache)
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut Self::Cache,
    ) -> Result<(), LayoutError> {
        for i in 0..children.len() {
            let size = children.measure(i, Proposal::IDEAL)?;
            children.place(
                i,
                Bounds::new(
                    bounds.x + children.value::<Offset>(i)?,
                    bounds.y,
                    size.width,
                    size.height,
                ),
            )?;
        }
        Ok(())
    }
}

#[test]
fn child_values_invalidate_only_the_arranging_parent_and_ancestors() {
    let parents = Rc::new(Cell::new(0));
    let children = Rc::new(Cell::new(0));
    let mut child = container(1, vec![leaf(2, 10.0, 10.0)], vec![]);
    child.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Tracked {
        offset: 0.0,
        updates: children.clone(),
    }));
    let mut root = container(0, vec![child], vec![]);
    root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(ValueParent(Tracked {
        offset: 0.0,
        updates: parents.clone(),
    })));
    let mut scene = Scene::default();
    scene
        .update(root.clone(), (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!((parents.get(), children.get()), (1, 1));
    let program = scene.frame.instances()[0].program.id();
    let values = LayoutValues::default().with::<Offset>(12.0);
    scene
        .patch([Change::LayoutValues(key(1), values.clone())])
        .unwrap();
    assert_eq!((parents.get(), children.get()), (2, 1));
    assert_eq!(scene.bounds(key(1)).unwrap().layout.x, 12.0);
    assert_eq!(scene.stats.recordings, 0);
    assert_eq!(scene.frame.instances()[0].program.id(), program);
    scene
        .patch([Change::LayoutValues(key(1), values.clone())])
        .unwrap();
    assert_eq!(scene.stats.layout_computations, 0);
    root.children[0].layout_values = values.with::<Offset>(18.0);
    scene
        .update(root, (100, 80), 1.0, RecordingMode::Retained)
        .unwrap();
    assert_eq!((parents.get(), children.get()), (3, 1));
    assert_eq!(scene.bounds(key(1)).unwrap().layout.x, 18.0);
}

fn aligned(id: u64, children: Vec<Element>) -> Element {
    let mut node = Element::new(key(id), Style::default()).with_children(children);
    node.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::row::<VerticalCenter>(
        4.0,
    )));
    node
}

fn guide(mut node: Element, at: f32) -> Element {
    node.layout_values = node
        .layout_values
        .with::<GuideValue<VerticalCenter>>(Some(AlignmentValue::at(at)));
    node
}

#[test]
fn named_alignment_exports_through_nested_layout_and_respects_padding() {
    let mut nested = aligned(
        1,
        vec![
            guide(leaf(2, 20.0, 10.0), 2.0),
            guide(leaf(3, 10.0, 20.0), 15.0),
        ],
    );
    nested.style.padding = taffy::Rect {
        left: taffy::LengthPercentage::length(1.0),
        right: taffy::LengthPercentage::length(2.0),
        top: taffy::LengthPercentage::length(3.0),
        bottom: taffy::LengthPercentage::length(2.0),
    };
    let root = aligned(0, vec![nested, guide(leaf(4, 10.0, 30.0), 4.0)]);
    for scale in [1.0, 1.25, 2.0] {
        let mut scene = Scene::default();
        scene
            .update(root.clone(), (160, 100), scale, RecordingMode::Retained)
            .unwrap();
        let a = scene.bounds(key(2)).unwrap().layout;
        let b = scene.bounds(key(3)).unwrap().layout;
        let c = scene.bounds(key(4)).unwrap().layout;
        assert_eq!(a.y + 2.0, b.y + 15.0);
        assert_eq!(a.y + 2.0, c.y + 4.0);
        assert_eq!(b.x, a.x + a.width + 4.0);
        let group = scene.bounds(key(1)).unwrap().layout;
        assert_eq!(a.y + 2.0, group.y + 18.0);
        let programs: Vec<_> = scene
            .frame
            .instances()
            .iter()
            .map(|i| i.program.id())
            .collect();
        scene
            .patch([Change::LayoutValues(
                key(1),
                LayoutValues::default()
                    .with::<GuideValue<VerticalCenter>>(Some(AlignmentValue::at(0.0))),
            )])
            .unwrap();
        assert_eq!(
            scene.bounds(key(1)).unwrap().layout.y,
            scene.bounds(key(4)).unwrap().layout.y + 4.0
        );
        assert_eq!(
            scene
                .frame
                .instances()
                .iter()
                .map(|i| i.program.id())
                .collect::<Vec<_>>(),
            programs
        );
        assert_eq!(scene.stats.recordings, 0);
    }
}

#[test]
fn fractional_defaults_and_vertical_stacks_align_without_explicit_guides() {
    let mut root = aligned(0, vec![leaf(1, 15.0, 10.0), leaf(2, 30.0, 20.0)]);
    root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::column::<
        HorizontalCenter,
    >(3.0)));
    let mut scene = Scene::default();
    scene
        .update(root, (160, 100), 1.25, RecordingMode::Retained)
        .unwrap();
    let a = scene.bounds(key(1)).unwrap().layout;
    let b = scene.bounds(key(2)).unwrap().layout;
    assert_eq!(a.x + a.width / 2.0, b.x + b.width / 2.0);
    assert_eq!(b.y, a.y + a.height + 3.0);
}

#[test]
fn invalid_guide_or_axis_preserves_published_geometry_and_can_recover() {
    let root = aligned(0, vec![leaf(1, 20.0, 10.0)]);
    let mut scene = Scene::default();
    scene
        .update(root.clone(), (160, 100), 1.0, RecordingMode::Retained)
        .unwrap();
    let before = scene.bounds(key(1)).unwrap().layout;
    let invalid = LayoutValues::default()
        .with::<GuideValue<VerticalCenter>>(Some(AlignmentValue::at(f32::NAN)));
    assert!(
        scene
            .patch([Change::LayoutValues(key(1), invalid)])
            .is_err()
    );
    assert_eq!(scene.bounds(key(1)).unwrap().layout, before);
    assert_eq!(
        scene.layout_failure().unwrap().error,
        LayoutError::InvalidGeometry
    );
    scene
        .patch([Change::LayoutValues(key(1), LayoutValues::default())])
        .unwrap();
    assert!(scene.layout_failure().is_none());
    assert!(
        scene
            .patch([Change::Layout(
                key(0),
                root.style.clone(),
                Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::row::<HorizontalCenter>(
                    0.0
                )))
            )])
            .is_err()
    );
    assert_eq!(
        scene.layout_failure().unwrap().error,
        LayoutError::AlignmentAxis
    );
    scene
        .patch([Change::Layout(key(0), root.style, root.algorithm)])
        .unwrap();
    assert_eq!(scene.bounds(key(1)).unwrap().layout, before);
}

struct Baseline;
impl AlignmentId for Baseline {
    const AXIS: Axis = Axis::Vertical;
    fn default_value(size: LayoutSize) -> f32 {
        size.height
    }
}

#[derive(PartialEq)]
struct InvalidExport;
impl lp::Layout for InvalidExport {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(LayoutSize::new(20.0, 10.0))
    }
    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        children.place(0, bounds)
    }
    fn explicit_alignment(
        &self,
        _: AlignmentGuide,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<Option<f32>, LayoutError> {
        children.place(0, bounds)?;
        Ok(None)
    }
}

#[test]
fn alignment_export_cannot_place_children() {
    let mut child = aligned(1, vec![leaf(2, 10.0, 10.0)]);
    child.algorithm = Algorithm::Custom(LayoutAlgorithm::new(InvalidExport));
    let mut root = aligned(0, vec![child]);
    root.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::row::<Baseline>(0.0)));
    let mut scene = Scene::default();
    assert!(
        scene
            .update(root, (100, 80), 1.0, RecordingMode::Retained)
            .is_err()
    );
    assert_eq!(
        scene.layout_failure().unwrap().error,
        LayoutError::PlacementDuringMeasurement
    );
    assert_eq!(scene.layout_failure().unwrap().container, key(1));
}
