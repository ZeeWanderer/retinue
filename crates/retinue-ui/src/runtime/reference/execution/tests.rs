use super::*;
use crate::{
    input::{Button, Event, PointerId, PointerPhase},
    runtime::reference::prelude::*,
};
use retinue_render::{
    HostPolicy, RenderHost,
    work::{Graph, Priority},
};
use std::{
    cell::Cell,
    num::NonZeroUsize,
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(10);

struct Gate(mpsc::Sender<()>);

impl Drop for Gate {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn execution() -> (RenderHost, LayoutExecution, mpsc::Receiver<()>) {
    let host = RenderHost::new(HostPolicy {
        threads: 2,
        ..HostPolicy::desktop()
    });
    let (send, receive) = mpsc::channel();
    let execution = LayoutExecution::new(
        &host,
        Arc::new(move || {
            let _ = send.send(());
        }),
    );
    (host, execution, receive)
}

fn block(host: &RenderHost) -> Gate {
    let (send, receive) = mpsc::channel();
    let (started, entered) = mpsc::channel();
    let mut graph = Graph::default();
    let width = host
        .task_threads(std::num::NonZeroU32::new(2).unwrap())
        .get();
    graph.add_owned(&[], NonZeroUsize::new(width as usize).unwrap(), move |_| {
        started.send(()).unwrap();
        receive.recv_timeout(WAIT).unwrap();
        Ok(())
    });
    host.executor()
        .submit(graph, Priority::Foreground, || {})
        .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    Gate(send)
}

fn rows(count: usize, width: f32) -> impl IntoElement {
    VStack::new(
        1.25,
        (0..count)
            .map(|id| {
                Rectangle::new(Color::rgb(0x333333))
                    .frame(width, 12.5)
                    .id(id as u64)
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn pending_layout_keeps_committed_input_and_does_not_block_dispatch() {
    let (host, execution, wake) = execution();
    let clicks = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(
        Rectangle::new(Color::rgb(0xff0000))
            .frame(40.0, 30.0)
            .input(InputOptions::new(EventHandler::new({
                let clicks = clicks.clone();
                move |event, _| {
                    if matches!(
                        event,
                        Event::Pointer {
                            phase: PointerPhase::Down(Button::Primary),
                            ..
                        }
                    ) {
                        clicks.set(clicks.get() + 1);
                    }
                }
            })))
            .id("old"),
        || {},
    );
    tree.update((200, 200), 1.0).unwrap();
    let generation = tree.frame().generation();
    let target = tree.input().hit_test((10.0, 10.0)).unwrap();
    assert_eq!(tree.input_key(target), Some("old".into()));
    let gate = block(&host);
    tree.set_root(rows(40, 90.0));
    assert!(
        tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
            .unwrap()
            .is_pending()
    );
    assert!(tree.is_layout_pending());
    assert_eq!(tree.frame().generation(), generation);
    assert_eq!(tree.input_key(target), Some("old".into()));
    assert_eq!(tree.hit_test((10.0, 10.0)), Some("old".into()));
    tree.dispatch(&Event::Pointer {
        id: PointerId(0),
        phase: PointerPhase::Down(Button::Primary),
        position: (10.0, 10.0),
        modifiers: Default::default(),
    });
    assert_eq!(clicks.get(), 1);
    assert_eq!(tree.update((200, 200), 1.0), Err("layout update pending"));
    drop(gate);
    wake.recv_timeout(WAIT).unwrap();
    assert!(
        tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
            .unwrap()
            .is_ready()
    );
    assert!(!tree.is_layout_pending());
    assert!(tree.input_key(target).is_none());
    assert!(tree.frame().generation() > generation);
}

#[test]
fn queued_model_and_viewport_changes_replace_unpublished_layout() {
    let (host, execution, wake) = execution();
    let gate = block(&host);
    let mut tree = ViewTree::new(rows(40, 90.0), || {});
    let generation = tree.frame().generation();
    assert!(
        tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
            .unwrap()
            .is_pending()
    );
    tree.set_root(rows(45, 70.0));
    assert!(
        tree.advance_at((300, 250), 1.2, FrameTime::now(), &execution)
            .unwrap()
            .is_pending()
    );
    drop(gate);
    wake.recv_timeout(WAIT).unwrap();
    assert!(
        tree.advance_at((300, 250), 1.2, FrameTime::now(), &execution)
            .unwrap()
            .is_pending()
    );
    assert_eq!(tree.frame().generation(), generation);
    wake.recv_timeout(WAIT).unwrap();
    assert!(
        tree.advance_at((300, 250), 1.2, FrameTime::now(), &execution)
            .unwrap()
            .is_ready()
    );
    let mut serial = ViewTree::new(rows(45, 70.0), || {});
    serial.update((300, 250), 1.2).unwrap();
    let geometry = |tree: &ViewTree| {
        tree.scene()
            .geometry()
            .iter()
            .map(|g| (g.layout, g.clip))
            .collect::<Vec<_>>()
    };
    assert_eq!(geometry(&tree), geometry(&serial));
    let commands = |tree: &ViewTree| {
        tree.frame()
            .instances()
            .iter()
            .map(|i| (i.visible_bounds(), i.program.commands().to_vec()))
            .collect::<Vec<_>>()
    };
    assert!(
        commands(&tree) == commands(&serial),
        "parallel publication changed paint commands"
    );
    assert!(!tree.needs_update());
}

#[test]
fn equivalent_root_submissions_do_not_invalidate_in_flight_layout() {
    #[derive(PartialEq)]
    struct Rows;

    impl View for Rows {
        fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
            rows(40, 90.0)
        }
    }

    for composite in [false, true] {
        let root = || {
            if composite {
                Rows.into_element()
            } else {
                rows(40, 90.0).into_element()
            }
        };
        let (host, execution, wake) = execution();
        let gate = block(&host);
        let mut tree = ViewTree::new(root(), || {});
        assert!(
            tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
                .unwrap()
                .is_pending()
        );
        for _ in 0..3 {
            tree.set_root(root());
            assert!(!tree.needs_evaluation());
        }
        drop(gate);
        wake.recv_timeout(WAIT).unwrap();
        tree.set_root(root());
        assert!(
            tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
                .unwrap()
                .is_ready()
        );
        tree.set_root(root());
        assert!(!tree.needs_update());
    }
}

#[test]
fn small_trees_stay_inline_and_idle_updates_do_not_submit_work() {
    let (host, execution, wake) = execution();
    let gate = block(&host);
    let mut tree = ViewTree::new(rows(1, 20.0), || {});
    assert!(
        tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution)
            .unwrap()
            .is_ready()
    );
    assert!(!tree.is_layout_pending());
    assert_eq!(
        tree.advance_at((200, 200), 1.0, FrameTime::now(), &execution),
        Ok(Poll::Ready(Damage::None))
    );
    assert!(wake.try_recv().is_err());
    drop(gate);
}
