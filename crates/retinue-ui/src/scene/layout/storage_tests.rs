use super::*;
use crate::{
    Bounds,
    layout::{Layout as LayoutProtocol, LayoutError, LayoutSize, Proposal, Subviews},
};
use retinue_render::work::{Executor, Graph, Limits, Priority, Submission};
use std::{
    cell::Cell,
    num::NonZeroUsize,
    rc::Rc,
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

fn fixture(calls: &Rc<Cell<usize>>) -> Element {
    let leaf = |key, width| {
        let mut element = Element::new(
            key,
            Style {
                size: Size {
                    width: taffy::Dimension::length(width),
                    height: taffy::Dimension::length(25.5),
                },
                ..Style::default()
            },
        );
        let calls = calls.clone();
        element.paint = Some(Paint::new(key, move |_| {
            calls.set(calls.get() + 1);
            Vec::new()
        }));
        element
    };
    let grid = Element::grid(
        2,
        Style {
            grid_template_columns: vec![
                taffy::style_helpers::fr(1.0_f32),
                taffy::style_helpers::fr(2.0_f32),
            ],
            ..Style::default()
        },
        vec![leaf(3, 40.25), leaf(4, 50.75)],
    );
    Element::column(1, 3.25, vec![grid, leaf(5, 100.0)])
}

#[derive(PartialEq)]
struct Measured;

impl LayoutProtocol for Measured {
    type Cache = Cell<usize>;
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        cache: &mut Self::Cache,
    ) -> Result<LayoutSize, LayoutError> {
        cache.set(cache.get() + 1);
        Ok(LayoutSize::new(110.25, 30.75))
    }
    fn place(
        &self,
        _: Bounds,
        _: &mut Subviews<'_, '_>,
        _: &mut Self::Cache,
    ) -> Result<(), LayoutError> {
        Ok(())
    }
}

fn shared_fixture(calls: &Rc<Cell<usize>>) -> Element {
    use crate::layout::{
        Insets,
        alignment::{AlignedStack, AlignmentValue, GuideValue, HorizontalCenter},
        grid::UniformGrid,
    };
    let mut element = fixture(calls);
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::column::<
        HorizontalCenter,
    >(3.25)));
    let grid = &mut element.children[0];
    grid.algorithm = Algorithm::Custom(LayoutAlgorithm::new(
        UniformGrid::new(
            2,
            LayoutSize::new(80.25, 42.5),
            LayoutSize::new(5.5, 3.25),
            Insets::all(2.25),
        )
        .unwrap()
        .arrange(
            crate::state::KeyedOrder::new(grid.children.iter().map(|child| child.key)).unwrap(),
        ),
    ));
    grid.layout_values = LayoutValues::default()
        .with::<GuideValue<HorizontalCenter>>(Some(AlignmentValue::at(31.5)));
    let leaf = &mut element.children[1];
    leaf.style = Style::default();
    leaf.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Measured));
    element
}

fn geometry(state: &State) -> Vec<Option<Layout>> {
    state
        .nodes
        .iter()
        .map(|node| node.as_ref().map(|node| node.layout))
        .collect()
}

#[test]
fn overlapping_invalidations_follow_current_parents_and_recycled_slots() {
    use crate::layout::alignment::{AlignedStack, HorizontalCenter};

    fn rows(range: std::ops::Range<u64>, shifted: bool, extra_height: f32) -> Element {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for i in range {
            let mut cell = Element::new(i, Style::default());
            cell.style.size.width = taffy::Dimension::length(20.25 + (i % 5) as f32);
            let height = 8.0 + (i % 3) as f32 + if i % 7 == 0 { extra_height } else { 0.0 };
            cell.style.size.height = taffy::Dimension::length(height);
            if (i % 2 == 0) ^ shifted {
                left.push(cell);
            } else {
                right.push(cell);
            }
        }
        let column = |key, children| {
            let mut node = Element::column(key, 1.0, children);
            node.algorithm = Algorithm::Custom(LayoutAlgorithm::new(AlignedStack::column::<
                HorizontalCenter,
            >(1.0)));
            node
        };
        Element::row(
            "root",
            4.0,
            vec![column("left", left), column("right", right)],
        )
    }

    let mut tree = Tree::default();
    let mut retired = None;
    for (range, shifted, width, extra_height) in [
        (0..90, false, 240.0, 0.0),
        (0..30, true, 200.0, 0.0),
        (30..90, false, 250.0, 0.0),
        (30..90, true, 200.0, 0.0),
        (30..90, true, 230.0, 3.0),
        (30..90, true, 230.0, 5.0),
    ] {
        if let Some(slot) = tree.slot(Key::from(89)) {
            tree.invalidate_content(slot);
            tree.invalidate_content(slot);
            retired = Some(slot);
        }
        let model = rows(range.clone(), shifted, extra_height);
        let root = tree.reconcile(model.clone()).unwrap();
        if range == (0..30) {
            assert!(tree.inputs[retired.unwrap()].is_none());
        } else if range == (30..90) && !shifted {
            assert!(tree.inputs[retired.unwrap()].is_some());
        }
        for id in range {
            tree.invalidate_content(tree.slot(Key::from(id)).unwrap());
        }
        let size = Size {
            width,
            height: 500.0,
        };
        tree.compute(root, size).unwrap();
        let mut fresh = Tree::default();
        let root = fresh.reconcile(model).unwrap();
        fresh.compute(root, size).unwrap();
        for (&key, &slot) in &fresh.keys {
            assert_eq!(
                tree.state.nodes[tree.slot(key).unwrap()]
                    .as_ref()
                    .unwrap()
                    .layout,
                fresh.state.nodes[slot].as_ref().unwrap().layout,
                "{key:?}"
            );
        }
    }
}

fn wait(submission: &Submission) {
    let start = Instant::now();
    loop {
        match submission.try_complete() {
            Ok(report) => {
                assert_eq!(report.failed, 0);
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty)
                if start.elapsed() < Duration::from_secs(5) =>
            {
                std::thread::sleep(Duration::from_millis(1));
            }
            other => panic!("missing layout completion: {other:?}"),
        }
    }
}

#[test]
fn layout_storage_moves_through_the_executor_without_owner_callbacks_or_copies() {
    for fixture in [fixture, shared_fixture] {
        check_owned_storage(fixture);
    }
}

fn check_owned_storage(fixture: fn(&Rc<Cell<usize>>) -> Element) {
    fn send_sync<T: Send + Sync>() {}
    fn send<T: Send>() {}
    send_sync::<Vec<Option<Input>>>();
    send::<State>();

    let calls = Rc::new(Cell::new(0));
    let mut serial = Tree::default();
    let root = serial.reconcile(fixture(&calls)).unwrap();
    let size = Size {
        width: 301.25,
        height: 203.75,
    };
    serial.compute(root, size).unwrap();

    let mut tree = Tree::default();
    let root = tree.reconcile(fixture(&calls)).unwrap();
    let inputs = std::mem::take(&mut tree.inputs);
    let mut state = std::mem::take(&mut tree.state);
    let input_address = inputs.as_ptr() as usize;
    let output_address = state.nodes.as_ptr() as usize;
    let owner_address = tree.nodes.as_ptr();
    let pool = Executor::new(Limits {
        participants: NonZeroUsize::MIN,
        tasks: 8,
    });
    let mut graph = Graph::default();
    let mut output = graph.add_owned(&[], NonZeroUsize::MIN, move |_| {
        solver::Computation::new(&inputs, &mut state, None)
            .compute(root, size)
            .unwrap();
        Ok((inputs, state))
    });
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    wait(&submission);
    (tree.inputs, tree.state) = output.take().unwrap().unwrap();
    assert_eq!(tree.inputs.as_ptr() as usize, input_address);
    assert_eq!(tree.state.nodes.as_ptr() as usize, output_address);
    assert_eq!(tree.nodes.as_ptr(), owner_address);
    assert_eq!(geometry(&tree.state), geometry(&serial.state));
    for slot in 0..tree.inputs.len() {
        assert_eq!(tree.render_context(slot), serial.render_context(slot));
    }
    assert_eq!(calls.get(), 0);
    tree.state.changed.clear();
    tree.compute(root, size).unwrap();
    assert_eq!(tree.state.computed, 0);
    assert_eq!(tree.state.changed.len(), 0);
}

#[test]
fn concurrent_solves_share_read_only_inputs_and_own_their_writes() {
    for fixture in [fixture, shared_fixture] {
        check_concurrent_solves(fixture);
    }
}

#[test]
fn text_layout_runs_without_owner_callbacks_and_paint_borrows_its_completed_result() {
    use crate::{
        runtime::ViewTree,
        runtime::reference::prelude::{HStack, Text, VStack},
    };
    let mut view = ViewTree::new(
        VStack::new(
            5.25,
            (
                Text::new("Wrapping and clipping use the same completed text layout.")
                    .font_size(18.5),
                HStack::new(
                    7.5,
                    (
                        Text::new("A limited paragraph with a\u{301} combining marks")
                            .line_limit(1),
                        Text::new("second\nline")
                            .alignment(crate::drawing::text::TextAlignment::Trailing),
                    ),
                ),
            ),
        ),
        || {},
    );
    view.update((241, 213), 1.0).unwrap();
    let scene = view.scene();
    let inputs = &scene.tree.inputs;
    let root = scene.root.unwrap();
    let mut state = State {
        nodes: inputs
            .iter()
            .map(|input| input.as_ref().map(|_| Geometry::default()))
            .collect(),
        ..State::default()
    };
    let address = state.nodes.as_ptr() as usize;
    let state = std::thread::scope(|threads| {
        threads
            .spawn(move || {
                solver::Computation::new(inputs, &mut state, None)
                    .compute(
                        root,
                        Size {
                            width: 241.0,
                            height: 213.0,
                        },
                    )
                    .unwrap();
                state
            })
            .join()
            .unwrap()
    });
    assert_eq!(state.nodes.as_ptr() as usize, address);
    assert_eq!(geometry(&state), geometry(&scene.tree.state));
    let mut programs = retinue_render::ProgramStore::default();
    let mut painted = 0;
    for (slot, node) in scene.tree.nodes.iter().enumerate() {
        let Some(owner) = node else { continue };
        let Some(paint) = &owner.paint else { continue };
        let state = state.nodes[slot].as_ref().unwrap();
        let program = paint.compile(&mut programs, state.layout.size, 1.0, state.result(None));
        assert!(program.commands() == owner.recorded.as_ref().unwrap().commands());
        painted += 1;
    }
    assert_eq!(painted, 3);
}

fn check_concurrent_solves(fixture: fn(&Rc<Cell<usize>>) -> Element) {
    let mut tree = Tree::default();
    let root = tree.reconcile(fixture(&Rc::new(Cell::new(0)))).unwrap();
    let inputs = Arc::new(tree.inputs);
    let pool = Executor::new(Limits {
        participants: NonZeroUsize::new(2).unwrap(),
        tasks: 8,
    });
    let barrier = Arc::new(Barrier::new(2));
    let mut graph = Graph::default();
    let mut outputs = Vec::new();
    let mut expected = Vec::new();
    for width in [300.25, 510.75] {
        let mut oracle = Tree::default();
        oracle.reconcile(fixture(&Rc::new(Cell::new(0)))).unwrap();
        let size = Size {
            width,
            height: 200.0,
        };
        oracle.compute(root, size).unwrap();
        expected.push(geometry(&oracle.state));
        let mut state = State {
            nodes: inputs
                .iter()
                .map(|node| node.as_ref().map(|_| Geometry::default()))
                .collect(),
            ..State::default()
        };
        let inputs = inputs.clone();
        let barrier = barrier.clone();
        outputs.push(graph.add_owned(&[], NonZeroUsize::MIN, move |_| {
            barrier.wait();
            solver::Computation::new(&inputs, &mut state, None)
                .compute(root, size)
                .unwrap();
            Ok(state)
        }));
    }
    let submission = pool.submit(graph, Priority::Foreground, || {}).unwrap();
    wait(&submission);
    for (mut output, expected) in outputs.into_iter().zip(expected) {
        assert_eq!(geometry(&output.take().unwrap().unwrap()), expected);
    }
}

#[test]
fn changing_layout_affinity_drops_the_previous_cache_and_remains_correct() {
    let mut tree = Tree::default();
    let mut element = Element::new(1, Style::default());
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Measured));
    let root = tree.reconcile(element).unwrap();
    let size = Size {
        width: 400.0,
        height: 300.0,
    };
    solver::Computation::new(&tree.inputs, &mut tree.state, None)
        .compute(root, size)
        .unwrap();
    assert!(
        tree.state.nodes[root]
            .as_ref()
            .unwrap()
            .shared_cache
            .is_some()
    );
    assert!(tree.nodes[root].as_ref().unwrap().custom_cache.is_none());

    tree.set_layout(
        root,
        Style::default(),
        Algorithm::Custom(LayoutAlgorithm::owner(Measured)),
    );
    assert!(
        tree.state.nodes[root]
            .as_ref()
            .unwrap()
            .shared_cache
            .is_none()
    );
    tree.compute(root, size).unwrap();
    assert!(tree.nodes[root].as_ref().unwrap().custom_cache.is_some());

    tree.set_layout(
        root,
        Style::default(),
        Algorithm::Custom(LayoutAlgorithm::new(Measured)),
    );
    assert!(tree.nodes[root].as_ref().unwrap().custom_cache.is_none());
    solver::Computation::new(&tree.inputs, &mut tree.state, None)
        .compute(root, size)
        .unwrap();
    assert_eq!(
        tree.state.nodes[root].as_ref().unwrap().layout.size,
        Size {
            width: 110.25,
            height: 30.75
        }
    );
    tree.set_layout(root, Style::default(), Algorithm::Flex);
    assert!(
        tree.state.nodes[root]
            .as_ref()
            .unwrap()
            .shared_cache
            .is_none()
    );
}

#[derive(PartialEq)]
struct OwnerLayout(Rc<Cell<usize>>);

impl LayoutProtocol for OwnerLayout {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        self.0.set(self.0.get() + 1);
        Ok(LayoutSize::new(30.0, 20.0))
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
}

#[test]
fn owner_only_layout_is_rejected_before_any_worker_state_changes() {
    let calls = Rc::new(Cell::new(0));
    let mut element = Element::new(1, Style::default());
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::owner(OwnerLayout(calls.clone())));
    let mut tree = Tree::default();
    let root = tree.reconcile(element).unwrap();
    let before = geometry(&tree.state);
    let dirties: Vec<_> = tree.state.changed.iter().collect();
    let size = Size {
        width: 300.0,
        height: 200.0,
    };
    assert_eq!(
        solver::Computation::new(&tree.inputs, &mut tree.state, None).compute(root, size),
        Err("custom layout requires owner callbacks")
    );
    assert_eq!(geometry(&tree.state), before);
    assert_eq!(tree.state.changed.iter().collect::<Vec<_>>(), dirties);
    assert_eq!(calls.get(), 0);
    tree.compute(root, size).unwrap();
    assert!(calls.get() > 0);
}

#[test]
fn removal_and_slot_reuse_replace_all_storage_domains() {
    let mut tree = Tree::default();
    let calls = Rc::new(Cell::new(0));
    let mut old = Element::new(2, Style::default());
    old.algorithm = Algorithm::Custom(LayoutAlgorithm::owner(OwnerLayout(calls.clone())));
    let root = tree.reconcile(Element::row(1, 0.0, vec![old])).unwrap();
    tree.compute(
        root,
        Size {
            width: 200.0,
            height: 100.0,
        },
    )
    .unwrap();
    let slot = tree.slot(2.into()).unwrap();
    assert!(tree.nodes[slot].as_ref().unwrap().custom_cache.is_some());
    tree.replace_many(vec![Element::row(1, 0.0, Vec::new()).into()])
        .unwrap();
    assert!(tree.inputs[slot].is_none());
    assert!(tree.nodes[slot].is_none());
    assert!(tree.state.nodes[slot].is_none());
    assert_eq!(Rc::strong_count(&calls), 1);
    tree.replace_many(vec![
        Element::row(1, 0.0, vec![Element::new(3, Style::default())]).into(),
    ])
    .unwrap();
    assert_eq!(tree.slot(3.into()), Some(slot));
    assert_eq!(tree.inputs[slot].as_ref().unwrap().parent, Some(root));
    assert_eq!(
        tree.state.nodes[slot].as_ref().unwrap().layout,
        Layout::default()
    );
    assert!(tree.nodes[slot].as_ref().unwrap().custom_cache.is_none());
    tree.compute(
        root,
        Size {
            width: 200.0,
            height: 100.0,
        },
    )
    .unwrap();
    assert_eq!(
        tree.state.nodes[slot].as_ref().unwrap().layout.size,
        Size::ZERO
    );
}

#[test]
fn unsupported_styles_are_rejected_before_scene_mutation() {
    use crate::scene::{Change, RecordingMode, Scene};
    let calls = Rc::new(Cell::new(0));
    let mut scene = Scene::default();
    scene
        .update(fixture(&calls), (400, 300), 1.0, RecordingMode::Retained)
        .unwrap();
    let bounds = scene.bounds(1.into()).unwrap().layout;
    let mut invalid = fixture(&calls);
    invalid.children[0].style.padding.left =
        taffy::LengthPercentage::calc(std::ptr::without_provenance(8));
    assert_eq!(
        scene.update(invalid.clone(), (400, 300), 1.0, RecordingMode::Retained),
        Err("unsupported layout calc handle")
    );
    assert_eq!(
        scene.tree.replace_many(vec![invalid.into()]),
        Err("unsupported layout calc handle")
    );
    let mut style = Style::default();
    style.flex_basis = taffy::Dimension::calc(std::ptr::without_provenance(8));
    assert_eq!(
        scene.patch([
            Change::Offset(1.into(), (50.0, 50.0)),
            Change::Layout(2.into(), style.clone(), Algorithm::Flex),
        ]),
        Err("unsupported layout calc handle")
    );
    let mut moved = Element::new(1, Style::default());
    moved.offset = (50.0, 50.0);
    assert_eq!(
        scene.patch_elements(vec![moved, Element::new(2, style)], (400, 300), 1.0),
        Err("unsupported layout calc handle")
    );
    scene.resize((400, 300), 1.0).unwrap();
    assert_eq!(scene.bounds(1.into()).unwrap().layout, bounds);
}
