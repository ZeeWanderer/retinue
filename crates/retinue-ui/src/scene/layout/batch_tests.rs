use super::*;
use crate::Bounds;
use crate::{
    layout::{Insets, LayoutSize, grid::UniformGrid},
    runtime::ViewTree,
    runtime::reference::prelude::{LayoutView, Text, VStack, ViewExt},
};
use retinue_render::work::{Executor, Graph, Limits, Priority};
use std::{num::NonZeroUsize, sync::Arc, time::Duration};

type Inputs = Arc<Vec<Option<Input>>>;

fn fixture(count: usize, long: bool) -> (Inputs, usize, Vec<(usize, Paint)>) {
    fixture_width(count, long, 210.5)
}

fn fixture_width(count: usize, long: bool, width: f32) -> (Inputs, usize, Vec<(usize, Paint)>) {
    let keys: Vec<_> = (0..count)
        .map(|i| Key::new("layout-cell", i as u64))
        .collect();
    let order = crate::state::KeyedOrder::new(keys.iter().copied()).unwrap();
    let cells: Vec<_> = keys.iter().enumerate().map(|(i, &key)| {
        let text = if long && i < count.div_ceil(4) {
            "Galvanized Aptitude: direct damage per status type affecting the target. Combining a\u{301} marks and wrapped text use the placed glyph run. ".repeat(4)
        } else { format!("Weapon {i}: direct damage and status chance") };
        VStack::new(4.25, (Text::new(format!("Item {i}")).font_size(18.0), Text::new(text).font_size(14.0).line_limit(4)))
            .id(key)
    }).collect();
    let grid = UniformGrid::new(
        4,
        LayoutSize::new(width, 150.25),
        LayoutSize::new(8.25, 7.75),
        Insets::all(4.25),
    )
    .unwrap();
    let mut view = ViewTree::new(LayoutView::new(grid.arrange(order), cells), || {});
    view.update((960, 800), 1.0).unwrap();
    let scene = view.scene();
    // Copy only the test fixture, before timing. Worker submissions share these
    // canonical inputs and move the existing state; no task makes a tree copy.
    let inputs = scene
        .tree
        .inputs
        .iter()
        .map(|input| {
            input.as_ref().map(|input| Input {
                key: input.key,
                layout_key: input.layout_key,
                style: style::NumericStyle::new((*input.style).clone()),
                kind: input.kind,
                custom: input.custom.clone(),
                records_layout: input.records_layout,
                layout_values: input.layout_values.clone(),
                children: input.children.clone(),
                parent: input.parent,
            })
        })
        .collect();
    let paints = scene
        .tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(slot, owner)| owner.as_ref()?.paint.clone().map(|paint| (slot, paint)))
        .collect();
    (Arc::new(inputs), scene.root.unwrap(), paints)
}

fn state(inputs: &[Option<Input>]) -> State {
    State {
        nodes: inputs
            .iter()
            .map(|input| input.as_ref().map(|_| Geometry::default()))
            .collect(),
        ..State::default()
    }
}

fn pool(width: usize) -> Executor {
    Executor::new(Limits {
        participants: NonZeroUsize::new(width).unwrap(),
        tasks: 8,
    })
}

fn run(
    pool: &Executor,
    inputs: Inputs,
    state: State,
    root: usize,
    width: usize,
    minimum: usize,
    size: Size<f32>,
) -> State {
    let (state, result) = run_checked(pool, inputs, state, root, width, minimum, size);
    result.unwrap();
    state
}

fn run_checked(
    pool: &Executor,
    inputs: Inputs,
    mut state: State,
    root: usize,
    width: usize,
    minimum: usize,
    size: Size<f32>,
) -> (State, Result<(), &'static str>) {
    state.changed.clear();
    let mut graph = Graph::default();
    let reserved = if minimum == usize::MAX { 1 } else { width };
    let mut output = graph.add_parallel_owned(
        &[],
        NonZeroUsize::new(reserved).unwrap(),
        move |_, parallel| {
            let result = solver::Computation::new(&inputs, &mut state, None)
                .batched(parallel, minimum)
                .compute(root, size);
            Ok((state, result))
        },
    );
    let (send, received) = std::sync::mpsc::channel();
    let submitted = pool
        .submit(graph, Priority::Foreground, move || {
            let _ = send.send(());
        })
        .unwrap();
    received.recv_timeout(Duration::from_secs(10)).unwrap();
    let report = submitted.try_complete().unwrap();
    assert_eq!(report.failed + report.cancelled, 0);
    output.take().unwrap().unwrap()
}

fn geometry(state: &State) -> Vec<Option<taffy::Layout>> {
    state
        .nodes
        .iter()
        .map(|node| node.as_ref().map(|node| node.layout))
        .collect()
}

fn paint(state: &State, paints: &[(usize, Paint)]) -> Vec<Vec<retinue_render::Command>> {
    let mut programs = retinue_render::ProgramStore::default();
    paints
        .iter()
        .map(|(slot, paint)| {
            let node = state.nodes[*slot].as_ref().unwrap();
            paint
                .compile(&mut programs, node.layout.size, 1.2, node.result(None))
                .commands()
                .to_vec()
        })
        .collect()
}

#[test]
fn ready_grid_batches_match_serial_layout_and_preserve_caches() {
    for long in [false, true] {
        let (inputs, root, paints) = fixture(17, long);
        let mut expected = state(&inputs);
        let size = Size {
            width: 960.0,
            height: 800.0,
        };
        solver::Computation::new(&inputs, &mut expected, None)
            .compute(root, size)
            .unwrap();
        for width in [1, 2, 4] {
            let pool = pool(width);
            let initial = state(&inputs);
            let allocation = initial.nodes.as_ptr() as usize;
            let mut actual = run(&pool, inputs.clone(), initial, root, width, 0, size);
            assert_eq!(allocation, actual.nodes.as_ptr() as usize);
            assert_eq!(geometry(&actual), geometry(&expected));
            assert_eq!(actual.computed, expected.computed);
            assert!(paint(&actual, &paints) == paint(&expected, &paints));
            actual.changed.coalesce();
            expected.changed.coalesce();
            assert_eq!(
                actual.changed.iter().collect::<Vec<_>>(),
                expected.changed.iter().collect::<Vec<_>>()
            );
            actual = run(&pool, inputs.clone(), actual, root, width, 0, size);
            assert_eq!(actual.computed, 0);
            assert_eq!(actual.changed.len(), 0);
            assert!(actual.nodes.iter().all(Option::is_some));
        }
    }
}

#[test]
fn batching_threshold_keeps_the_serial_result() {
    let (inputs, root, _) = fixture(8, true);
    let pool = pool(4);
    let size = Size {
        width: 960.0,
        height: 800.0,
    };
    let serial = run(
        &pool,
        inputs.clone(),
        state(&inputs),
        root,
        4,
        usize::MAX,
        size,
    );
    let batched = run(&pool, inputs.clone(), state(&inputs), root, 4, 0, size);
    assert_eq!(geometry(&serial), geometry(&batched));
    assert_eq!(serial.computed, batched.computed);
}

#[test]
fn sparse_reordered_slots_remain_disjoint_and_use_canonical_keys() {
    let (inputs, root, mut paints) = fixture(13, true);
    let count = inputs.len();
    let remap = |slot: usize| {
        let reversed = count - 1 - slot;
        (reversed / 2 + (reversed % 2) * count.div_ceil(2)) * 2 + 1
    };
    let mut sparse: Vec<_> = (0..count * 2).map(|_| None).collect();
    for (slot, input) in Arc::try_unwrap(inputs)
        .ok()
        .unwrap()
        .into_iter()
        .enumerate()
    {
        if let Some(mut input) = input {
            input.parent = input.parent.map(remap);
            input.children = input.children.into_iter().map(remap).collect();
            sparse[remap(slot)] = Some(input);
        }
    }
    for (slot, _) in &mut paints {
        *slot = remap(*slot);
    }
    let inputs = Arc::new(sparse);
    let root = remap(root);
    let size = Size {
        width: 961.25,
        height: 800.75,
    };
    let pool = pool(4);
    let mut serial = state(&inputs);
    let mut parallel = state(&inputs);
    let resized = Size {
        width: 823.5,
        ..size
    };
    for proposal in [size, resized, resized] {
        serial = run(&pool, inputs.clone(), serial, root, 1, 0, proposal);
        parallel = run(&pool, inputs.clone(), parallel, root, 4, 0, proposal);
        assert_eq!(serial.computed, parallel.computed);
        assert_eq!(geometry(&serial), geometry(&parallel));
        assert!(paint(&serial, &paints) == paint(&parallel, &paints));
        for (input, node) in inputs.iter().zip(parallel.nodes.iter()) {
            assert_eq!(input.is_some(), node.is_some());
        }
    }
    assert_eq!(parallel.computed, 0);
}

#[test]
fn sibling_failures_restore_owned_state_and_retry_without_cached_failure() {
    use crate::layout::{Layout as LayoutProtocol, LayoutError, Proposal, Subviews};
    use std::{
        cell::Cell,
        sync::{
            Barrier, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };

    struct Leaf {
        id: u32,
        fail: Arc<AtomicBool>,
        barrier: Arc<Barrier>,
        threads: Arc<Mutex<std::collections::HashSet<std::thread::ThreadId>>>,
    }
    impl PartialEq for Leaf {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id && Arc::ptr_eq(&self.fail, &other.fail)
        }
    }
    impl LayoutProtocol for Leaf {
        type Cache = Cell<u32>;
        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut Self::Cache,
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(80.0, 40.0))
        }
        fn place(
            &self,
            _: Bounds,
            _: &mut Subviews<'_, '_>,
            cache: &mut Self::Cache,
        ) -> Result<(), LayoutError> {
            cache.set(cache.get() + 1);
            if self.fail.load(Ordering::Relaxed) {
                self.threads
                    .lock()
                    .unwrap()
                    .insert(std::thread::current().id());
                self.barrier.wait();
                if self.id == 0 {
                    return Err(LayoutError::InvalidGeometry);
                }
            }
            Ok(())
        }
    }
    let fail = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(2));
    let threads = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let mut tree = Tree::default();
    let children: Vec<_> = (0..2)
        .map(|id| {
            let mut child = Element::new(id as u64 + 2, Style::default());
            child.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Leaf {
                id,
                fail: fail.clone(),
                barrier: barrier.clone(),
                threads: threads.clone(),
            }));
            child
        })
        .collect();
    let order = crate::state::KeyedOrder::new(children.iter().map(|child| child.key)).unwrap();
    let mut grid = Element::new(1, Style::default()).with_children(children);
    grid.algorithm = Algorithm::Custom(LayoutAlgorithm::new(
        UniformGrid::new(
            2,
            LayoutSize::new(80.0, 40.0),
            LayoutSize::ZERO,
            Insets::all(0.0),
        )
        .unwrap()
        .arrange(order),
    ));
    let root = tree.reconcile(grid).unwrap();
    let inputs = Arc::new(tree.inputs);
    let pool = pool(2);
    let size = Size {
        width: 160.0,
        height: 40.0,
    };
    let (state, result) = run_checked(&pool, inputs.clone(), tree.state, root, 2, 0, size);
    assert_eq!(result, Err("custom layout failed"));
    assert_eq!(threads.lock().unwrap().len(), 2);
    assert!(state.nodes.iter().all(Option::is_some));
    assert_eq!(state.failure.as_ref().unwrap().container, Key::from(2));
    assert!(
        state
            .nodes
            .iter()
            .flatten()
            .all(|node| node.shared_cache.as_ref().is_none_or(|cache| cache.dirty))
    );
    fail.store(false, Ordering::Relaxed);
    let state = run(&pool, inputs.clone(), state, root, 2, 0, size);
    assert!(state.failure.is_none());
    for slot in [1, 2] {
        assert_eq!(
            state.nodes[slot]
                .as_ref()
                .unwrap()
                .result(None)
                .get::<Leaf>()
                .unwrap()
                .get(),
            1
        );
    }
}

#[test]
fn measurement_after_a_completed_batch_cannot_replace_placed_text() {
    use crate::layout::{Layout as LayoutProtocol, LayoutError, Proposal, Subviews};
    #[derive(PartialEq)]
    struct ProbingLayout;
    impl LayoutProtocol for ProbingLayout {
        type Cache = ();
        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(440.0, 140.0))
        }
        fn place(
            &self,
            _: Bounds,
            children: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            children
                .place_each(|index, _| Ok(Bounds::new(index as f32 * 220.0, 0.0, 210.0, 140.0)))?;
            children.measure(0, Proposal::bounded(480.0, 140.0))?;
            Ok(())
        }
    }
    let (mut inputs, root, paints) = fixture(2, true);
    let input = Arc::get_mut(&mut inputs)
        .unwrap()
        .iter_mut()
        .flatten()
        .find(|input| {
            input.custom.as_ref().is_some_and(|layout| {
                LayoutAlgorithm::from(layout.clone())
                    .name()
                    .ends_with("::GridLayout")
            })
        })
        .unwrap();
    input.custom = Some(LayoutAlgorithm::new(ProbingLayout).split().unwrap());
    let pool = pool(2);
    let size = Size {
        width: 440.0,
        height: 140.0,
    };
    let serial = run(&pool, inputs.clone(), state(&inputs), root, 1, 0, size);
    let parallel = run(&pool, inputs.clone(), state(&inputs), root, 2, 0, size);
    assert_eq!(geometry(&serial), geometry(&parallel));
    assert!(paint(&serial, &paints) == paint(&parallel, &paints));
}

#[allow(dead_code)]
#[path = "../../../../../tools/benchmarks/support.rs"]
mod bench;

#[test]
#[ignore = "layout batch comparison; use make bench BENCH_SUITE=layout-batches"]
fn layout_batch_benchmark() {
    let participants = bench::render_threads().get() as usize;
    let mut cases = Vec::new();
    for (label, count, long) in [
        ("small", 4, false),
        ("cards", 48, false),
        ("skewed", 48, true),
        ("reflow", 48, false),
    ] {
        for mode in ["serial", "queued", "batched"] {
            let name = format!("{label}-{mode}");
            if !bench::selected(&name) {
                continue;
            }
            let (inputs, root, _) = fixture(count, long);
            let alternate = if label == "reflow" {
                fixture_width(count, long, 172.25).0
            } else {
                inputs.clone()
            };
            let variants = [inputs, alternate];
            let size = Size {
                width: 960.0,
                height: 800.0,
            };
            let expected = variants.each_ref().map(|inputs| {
                let mut result = state(inputs);
                solver::Computation::new(inputs, &mut result, None)
                    .compute(root, size)
                    .unwrap();
                geometry(&result)
            });
            let grid = variants[0]
                .iter()
                .position(|input| {
                    input.as_ref().is_some_and(|input| {
                        input.custom.as_ref().is_some_and(|layout| {
                            LayoutAlgorithm::from(layout.clone())
                                .name()
                                .ends_with("::GridLayout")
                        })
                    })
                })
                .unwrap();
            let mut case = bench::measure_with_warmup(
                &name,
                (960, 800),
                1.0,
                10,
                || (pool(participants), state(&variants[0]), 0),
                |(pool, result, selected), frame| {
                    *selected = frame % 2;
                    let inputs = &variants[*selected];
                    result.changed.clear();
                    if label == "reflow" {
                        let mut current = Some(grid);
                        while let Some(slot) = current {
                            let node = result.nodes[slot].as_mut().unwrap();
                            node.cache.clear();
                            if slot == grid
                                && let Some(cache) = &mut node.shared_cache
                            {
                                cache.dirty = true;
                            }
                            current = inputs[slot].as_ref().unwrap().parent;
                        }
                    } else {
                        *result = state(inputs);
                    }
                    if mode == "serial" {
                        solver::Computation::new(inputs, result, None)
                            .compute(root, size)
                            .unwrap();
                    } else {
                        *result = run(
                            pool,
                            inputs.clone(),
                            std::mem::take(result),
                            root,
                            participants,
                            if mode == "batched" { 0 } else { usize::MAX },
                            size,
                        );
                    }
                },
                |(_, result, selected)| {
                    assert_eq!(geometry(result), expected[*selected]);
                    serde_json::json!({"nodes": variants[0].len(), "items": count, "computations": result.computed,
                        "participants": participants, "cold_state": label != "reflow", "mode": mode})
                },
            );
            case["actual_threads"] =
                serde_json::json!(if mode == "batched" { participants } else { 1 });
            cases.push(case);
        }
    }
    bench::report_with_scope(
        "layout-batches",
        cases,
        "Real grid/stack/Text solve, batch partition and completion. Small/cards/skewed allocate cold state; reflow retains shaping and alternates cell widths. Skewed clusters long text in the first quarter. Fonts/inputs and worker pool warm; serial caller versus queued serial and cooperative batch. No semantic evaluation, recording, painting or presentation; benchmark caller only waits.",
    );
}
