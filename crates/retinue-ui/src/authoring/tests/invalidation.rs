use super::*;
use crate::{Key, layout::anchors::AnchorPoint};
use modifiers::pixels;

const FRAME: Key = Key::new("text.invalidation", 1);
const TEXT: Key = Key::new("text.invalidation", 2);
const SIBLING: Key = Key::new("text.invalidation", 3);

fn mount(text: State<String>, framed: bool) -> InstanceGraph {
    let mut graph = InstanceGraph::default();
    let root = graph.mount_body(move |_| {
        ui! {
            VStack(7.0) {
                if framed {
                    Text(text.get()).named(TEXT)
                        .frame(Frame::new().width(120.0).height(60.0)).named(FRAME)
                } else {
                    Text(text.get()).named(TEXT)
                }
                Text("Untouched sibling").named(SIBLING)
            }.frame(Frame::new().width(260.0).height(180.0).alignment(AnchorPoint::TOP_LEADING))
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    graph
}

#[test]
fn text_edits_match_fresh_layout_and_paint_with_and_without_fixed_frames() {
    for framed in [false, true] {
        for scale in [1.0, 1.2, 1.5] {
            let size = ((300.0 * scale) as u32, (220.0 * scale) as u32);
            let text = State::new("9m 59s".to_owned());
            let mut graph = mount(text.clone(), framed);
            graph.advance(size, scale, None).unwrap();
            let sibling = graph
                .geometry(graph.unique(SIBLING).unwrap().unwrap())
                .unwrap()
                .layout;
            for content in [
                "9m 58s",
                "A much longer label that wraps across several lines",
                "i",
                "WWW",
                "",
            ] {
                text.set(content.to_owned());
                graph.advance(size, scale, None).unwrap();
                let mut fresh = mount(State::new(content.to_owned()), framed);
                fresh.advance(size, scale, None).unwrap();
                assert_eq!(
                    graph.scene().geometry().len(),
                    fresh.scene().geometry().len()
                );
                for (actual, expected) in graph
                    .scene()
                    .geometry()
                    .iter()
                    .zip(fresh.scene().geometry())
                {
                    assert_eq!(actual.layout, expected.layout, "{framed} {scale} {content}");
                }
                assert!(
                    pixels(graph.scene()) == pixels(fresh.scene()),
                    "{framed} {scale} {content}"
                );
                if framed {
                    assert_eq!(
                        graph
                            .geometry(graph.unique(SIBLING).unwrap().unwrap())
                            .unwrap()
                            .layout,
                        sibling
                    );
                }
            }
        }
    }
}

#[test]
fn queued_text_updates_publish_atomically_and_keep_unrelated_geometry() {
    use retinue_render::{HostPolicy, RenderHost};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let text = State::new("9m 59s".to_owned());
    let read = text.clone();
    let mut graph = InstanceGraph::default();
    let root = graph.mount_body(move |_| {
        ui! {
            VStack(2.0) {
                Text(read.get()).named(TEXT).frame(Frame::new().width(120.0).height(30.0))
                ForEach(0..40_u64, |id| *id) { |id|
                    Rectangle(Color::rgb(0x405060)).frame(Frame::new().width(40.0).height(2.0)).named(Key::new("unrelated", id))
                }
            }
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    let old_generation = advance(&mut graph).published;
    let host = RenderHost::new(HostPolicy {
        threads: 2,
        ..HostPolicy::desktop()
    });
    let (send, wake) = mpsc::channel();
    let execution = crate::scene::LayoutExecution::new(
        &host,
        Arc::new(move || {
            let _ = send.send(());
        }),
    );
    let before = pixels(graph.scene());
    text.set("9m 58s".to_owned());
    let update = graph.advance((400, 300), 1.0, Some(&execution)).unwrap();
    assert!(update.completion.unwrap().is_pending());
    assert_eq!(update.published, old_generation);
    assert!(pixels(graph.scene()) == before);
    wake.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        graph
            .advance((400, 300), 1.0, Some(&execution))
            .unwrap()
            .completion
            .unwrap()
            .is_ready()
    );
    assert!(
        graph.scene().stats.layout_computations <= 5,
        "{:?}",
        graph.scene().stats
    );
    assert!(pixels(graph.scene()) != before);
}
