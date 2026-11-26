use super::*;
use retinue_render::{
    HostPolicy, RenderHost,
    work::{Graph, Priority},
};
use retinue_ui::runtime::LayoutExecution;
use std::{
    num::NonZeroUsize,
    sync::{Arc, mpsc},
};

const WAIT: Duration = Duration::from_secs(10);

struct Gate(mpsc::Sender<()>);
impl Drop for Gate {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn executor() -> (RenderHost, LayoutExecution, mpsc::Receiver<()>) {
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
    let width =
        NonZeroUsize::new(host.task_threads(std::num::NonZeroU32::MAX).get() as usize).unwrap();
    graph.add_owned(&[], width, move |_| {
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

fn finish(pages: &mut Pages, wake: &mpsc::Receiver<()>) {
    let until = Instant::now() + WAIT;
    while !pages.poll_layout() {
        wake.recv_timeout(until.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn page_layout_completion_rebinds_controls_after_resize_and_page_replacement() {
    let (host, execution, wake) = executor();
    let mut pages = pages();
    pages.set_page(Page::Inventory);
    pages.layout();
    pages.ui.set_layout_execution(execution.clone());
    pages.rail.set_layout_execution(execution);
    let buttons = |pages: &Pages| {
        pages
            .ui
            .buttons
            .iter()
            .map(|b| (b.key.clone(), b.rect))
            .collect::<Vec<_>>()
    };
    let previous = buttons(&pages);
    let gate = block(&host);
    pages.configure((1200, 850), (400, 850), 1.2);
    assert!(pages.ui.layout_pending());
    assert_eq!(buttons(&pages), previous);
    drop(gate);
    finish(&mut pages, &wake);
    assert_ne!(buttons(&pages), previous);
    assert!(!pages.ui.buttons.is_empty());

    for page in [Page::Relics, Page::Mastery, Page::Market, Page::Builds] {
        let gate = block(&host);
        pages.set_page(page);
        pages.layout();
        // A second navigation may cancel an unpublished tree while its arena is away.
        pages.set_page(Page::Inventory);
        pages.layout();
        drop(gate);
        finish(&mut pages, &wake);
        assert_eq!(pages.page, Page::Inventory);
        assert!(!pages.ui.buttons.is_empty());
    }
}

#[test]
fn foundry_layout_keeps_committed_controls_until_worker_completion() {
    let (host, execution, wake) = executor();
    let mut foundry = foundry::Foundry::new(Default::default()).unwrap();
    foundry.resize((1300, 900), 1.25);
    foundry.update(crate::data::foundry::fixture());
    assert!(foundry.poll_layout());
    foundry.set_layout_execution(execution);
    let before = foundry.controls();
    let gate = block(&host);
    foundry.resize((1450, 1000), 1.2);
    assert!(!foundry.poll_layout());
    assert_eq!(foundry.controls(), before);
    drop(gate);
    let until = Instant::now() + WAIT;
    while !foundry.poll_layout() {
        wake.recv_timeout(until.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    assert_ne!(foundry.controls(), before);
}

#[test]
fn fresh_mount_defers_the_child_until_frame_prepare_and_drops_it_on_reset() {
    use retinue_ui::{authoring::*, timeline::FrameTime};
    use std::{cell::Cell, rc::Rc};

    #[derive(PartialEq)]
    struct Grid;
    #[view]
    impl View for Grid {
        fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
            ui! {
                VStack(0.0) {
                    ForEach(0..64_u64, |id| *id) { |id|
                        Rectangle(retinue_render::Color::rgb(0x333333))
                            .frame(Frame::new().height(8.0)).named(id.into())
                    }
                }
            }
        }
    }
    let (host, execution, _) = executor();
    let mut ui = controls::ControlHost::new().unwrap();
    ui.size = (400, 500);
    ui.set_layout_execution(execution);
    let gate = block(&host);
    let polls = Rc::new(Cell::new(0));
    let child = ui.mount_fixed_view(100_u64.into(), Grid, {
        let polls = polls.clone();
        move |size| {
            polls.set(polls.get() + 1);
            size
        }
    });
    let now = FrameTime::now();
    assert!(!ui.resolve_view(now));
    assert_eq!(polls.get(), 0);
    assert!(ui.layout_pending());
    assert!(!ui.resolve_view_for(now, controls::LayoutPass::PrepareFrame));
    assert_eq!(polls.get(), 1);
    assert!(ui.view_bounds(1_u64.into()).is_none());
    ui.reset_view();
    assert!(child.upgrade().is_none());
    assert!(!ui.layout_pending());
    drop(gate);
}

#[test]
fn animated_layout_publishes_before_sampling_the_next_frame() {
    use retinue_render::Color;
    use retinue_ui::{authoring::*, state::State, timeline::FrameTime};

    #[derive(Clone, PartialEq)]
    struct Rows(State<f32>);
    #[view]
    impl View for Rows {
        fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
            ui! {
                VStack(0.0) {
                    ForEach(0..40_u64, |id| *id) { |id|
                        Rectangle(Color::rgb(0x333333))
                            .frame(Frame::new().width(self.0.get()).height(10.0))
                            .named(id.into())
                    }
                }
            }
        }
    }

    let (host, execution, wake) = executor();
    let width = State::new(20.0);
    let mut ui = controls::ControlHost::new().unwrap();
    ui.size = (400, 500);
    ui.install_compiled(Rows(width.clone()));
    ui.set_layout_execution(execution);
    let start = FrameTime::now();
    let later = start.at(start.instant + Duration::from_secs(1));
    let gate = block(&host);
    assert!(!ui.resolve_view(start));

    let sample = ui.layout_time(later);
    assert_eq!(sample, start);
    width.set(20.0 + sample.instant.duration_since(start.instant).as_secs_f32() * 10.0);
    drop(gate);
    wake.recv_timeout(WAIT).unwrap();
    assert!(ui.resolve_view(later));
    assert_eq!(ui.view_bounds(0_u64.into()).unwrap().width, 20);

    assert_eq!(ui.layout_time(later), later);
    width.set(30.0);
    assert!(!ui.resolve_view(later));
    wake.recv_timeout(WAIT).unwrap();
    assert!(ui.resolve_view(later));
    assert_eq!(ui.view_bounds(0_u64.into()).unwrap().width, 30);
}
