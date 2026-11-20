use super::*;
use crate::{
    Bounds,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews},
    scene::{Algorithm, Element, layout_protocol::LayoutAlgorithm},
};
use retinue_render::work::Limits;
use std::{sync::mpsc, time::Duration};

const SIZE: Size<f32> = Size {
    width: 400.0,
    height: 600.0,
};
const WAIT: Duration = Duration::from_secs(10);

fn execution(tasks: usize) -> (LayoutExecution, mpsc::Receiver<()>) {
    let (send, receive) = mpsc::channel();
    (
        LayoutExecution {
            executor: Arc::new(Executor::new(Limits {
                participants: NonZeroUsize::MIN,
                tasks,
            })),
            participants: NonZeroUsize::MIN,
            wake: Arc::new(move || {
                let _ = send.send(());
            }),
        },
        receive,
    )
}

fn fixture() -> Element {
    Element::column(
        1,
        2.5,
        (2..42)
            .map(|key| Element::new(key, Default::default()))
            .collect(),
    )
}

#[test]
fn rejected_admission_restores_the_same_allocations_for_inline_layout() {
    let (execution, wake) = execution(0);
    let mut tree = Tree::default();
    let root = tree.reconcile(fixture()).unwrap();
    let inputs = tree.inputs.as_ptr();
    let outputs = tree.state.nodes.as_ptr();
    assert!(tree.begin_layout(root, SIZE, &execution).is_none());
    assert_eq!(tree.inputs.as_ptr(), inputs);
    assert_eq!(tree.state.nodes.as_ptr(), outputs);
    tree.compute(root, SIZE).unwrap();
    assert!(wake.try_recv().is_err());
}

#[test]
fn cancelled_queued_layout_returns_storage_without_running_the_solver() {
    let (execution, wake) = execution(8);
    // Foreground admission is blocked by a running one-participant job.
    let (release, blocked) = mpsc::channel();
    let (started, entered) = mpsc::channel();
    let mut graph = Graph::default();
    graph.add_owned(&[], NonZeroUsize::MIN, move |_| {
        started.send(()).unwrap();
        blocked.recv_timeout(WAIT).unwrap();
        Ok(())
    });
    execution
        .executor
        .submit(graph, Priority::Foreground, || {})
        .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    let mut tree = Tree::default();
    let root = tree.reconcile(fixture()).unwrap();
    let inputs = tree.inputs.as_ptr();
    let mut pending = tree.begin_layout(root, SIZE, &execution).unwrap();
    pending.submission.as_ref().unwrap().cancel();
    release.send(()).unwrap();
    wake.recv_timeout(WAIT).unwrap();
    assert_eq!(
        pending.poll(&mut tree),
        Some(Err("layout computation cancelled"))
    );
    assert_eq!(tree.inputs.as_ptr(), inputs);
    assert_eq!(tree.state.computed, 0);
    tree.compute(root, SIZE).unwrap();
}

#[derive(PartialEq)]
struct Fault(bool);

impl Layout for Fault {
    type Cache = ();
    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        assert!(!self.0, "test layout panic");
        Err(LayoutError::InvalidGeometry)
    }
    fn place(&self, _: Bounds, _: &mut Subviews<'_, '_>, _: &mut ()) -> Result<(), LayoutError> {
        Ok(())
    }
}

#[test]
fn failed_and_panicking_workers_return_storage_for_the_next_update() {
    for panic in [false, true] {
        let (execution, wake) = execution(8);
        let mut tree = Tree::default();
        let mut element = fixture();
        element.algorithm = Algorithm::Custom(LayoutAlgorithm::new(Fault(panic)));
        let root = tree.reconcile(element).unwrap();
        let inputs = tree.inputs.as_ptr();
        let mut pending = tree.begin_layout(root, SIZE, &execution).unwrap();
        wake.recv_timeout(WAIT).unwrap();
        assert!(pending.poll(&mut tree).unwrap().is_err());
        assert_eq!(tree.inputs.as_ptr(), inputs);
        let root = tree.reconcile(fixture()).unwrap();
        tree.compute(root, SIZE).unwrap();
    }
}

#[test]
fn owner_layouts_stay_inline_even_in_a_large_tree() {
    let (execution, wake) = execution(8);
    let mut tree = Tree::default();
    let mut element = fixture();
    element.algorithm = Algorithm::Custom(LayoutAlgorithm::owner(Fault(false)));
    let root = tree.reconcile(element).unwrap();
    let inputs = tree.inputs.as_ptr();
    assert!(tree.begin_layout(root, SIZE, &execution).is_none());
    assert_eq!(tree.inputs.as_ptr(), inputs);
    assert!(wake.try_recv().is_err());
}
