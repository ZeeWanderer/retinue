use super::*;
use crate::{Key, runtime::ViewId};
use std::{cell::RefCell, collections::BTreeMap, panic::AssertUnwindSafe};

type Rows = Rc<RefCell<BTreeMap<u64, (ViewId, State<f32>)>>>;

struct ClockRow(State<crate::timeline::Schedule>);
impl ItemView for ClockRow {
    type Item = crate::timeline::Schedule;
    fn from_item(_: Key, item: State<Self::Item>) -> Self {
        Self(item)
    }
}
#[view]
impl View for ClockRow {
    fn body(&self, c: &mut ViewContext<'_>) -> impl Body {
        c.time(self.0.get());
        empty()
    }
}

#[test]
fn keyed_clock_is_scheduled_without_evaluating_the_window() {
    use crate::{
        state::Items,
        timeline::{FrameTime, Schedule},
    };
    use std::time::Duration;
    let start = FrameTime::now();
    let due = start.instant + Duration::from_secs(2);
    let items = Items::default();
    items
        .replace([
            (Key::from(1u64), Schedule::At(due)),
            (Key::from(2u64), Schedule::Paused),
        ])
        .unwrap();
    let mut graph = InstanceGraph::default();
    let source = items.clone();
    let root = graph
        .mount_body(
            move |_| ItemViews::<ClockRow>(source.clone()),
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let step = |g: &mut InstanceGraph, t| {
        let u = g.advance_at((400, 300), 1.0, t, None).unwrap();
        assert!(u.completion.unwrap().is_ready());
        u.work.owners_evaluated
    };
    assert_eq!(step(&mut graph, start), 3);
    assert_eq!(graph.next_deadline(), Some(due));
    assert_eq!(step(&mut graph, start), 0);
    assert_eq!(step(&mut graph, start.at(due)), 1);
    items.replace([]).unwrap();
    assert_eq!(step(&mut graph, start.at(due)), 1);
    assert_eq!(graph.next_deadline(), None);
}

struct BoundRow(Key, State<f32>);
impl ItemView for BoundRow {
    type Item = f32;
    fn from_item(key: Key, value: State<f32>) -> Self {
        Self(key, value)
    }
}
#[view]
impl View for BoundRow {
    fn body(&self, c: &mut ViewContext<'_>) -> impl Body {
        let n = self.1.get();
        let initial = c.state("initial", || n);
        empty()
            .frame(Frame::new().width(initial.get() + n).height(10.0))
            .interactive()
            .named(self.0)
    }
}

#[test]
fn keyed_row_programs_run_only_on_entry_or_invalidation() {
    use crate::state::{Items, KeyedOrder};
    let key = |i| Key::new("test.row", i);
    let order = |ids: &[u64]| KeyedOrder::new(ids.iter().copied().map(key)).unwrap();
    let items = Items::default();
    items.set_order_with(order(&[1, 2, 3]), |_, k| k.value as f32);
    let source = State::new(items.clone());
    let hits = State::new(true);
    let mut graph = InstanceGraph::default();
    let root = graph.mount_body({
        let (source, hits) = (source.clone(), hits.clone());
        move |_| ui! { VStack(0.0) { ItemViews::<BoundRow>(source.get()).hit_testing(hits.get()) } }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    assert_eq!(advance(&mut graph).work.owners_evaluated, 4);
    let retained = graph.unique(key(2)).unwrap().unwrap();
    let gone = items.item(key(1)).unwrap();
    let mut entered = Vec::new();
    items.set_order_with(order(&[3, 2, 4]), |_, k| {
        entered.push(k);
        4.0
    });
    assert_eq!(entered, [key(4)]);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 2);
    gone.set(42.0);
    assert!(!graph.has_pending_inputs());
    items.set_order_with(order(&[4, 2, 3]), |_, _| panic!("rebuilt retained input"));
    assert_eq!(advance(&mut graph).work.owners_evaluated, 1);
    items.set(key(2), 12.0);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 1);
    assert_eq!(graph.geometry(retained).unwrap().layout.width, 14.0);
    let replacement = Items::default();
    replacement.set_order_with(items.order(), |_, _| 17.0);
    source.set(replacement);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 4);
    assert_eq!(graph.unique(key(2)).unwrap(), Some(retained));
    assert_eq!(graph.geometry(retained).unwrap().layout.width, 19.0);
    items.set(key(2), 7.0);
    assert!(!graph.has_pending_inputs());
    let rect = graph.geometry(retained).unwrap().layout;
    let point = (rect.x + 1.0, rect.y + 1.0);
    assert_eq!(graph.hit_test(point), Some(key(2)));
    hits.set(false);
    advance(&mut graph);
    assert_eq!(graph.hit_test(point), None);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 0);
}

struct Lifetime(Rc<Cell<usize>>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[derive(PartialEq)]
struct Row {
    key: u64,
    rows: Rows,
    drops: Rc<Cell<usize>>,
    fail: State<Option<u64>>,
}

#[view]
impl View for Row {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let width = context.state("width", || self.key as f32 * 10.0);
        context.owned("lifetime", || Rc::new(Lifetime(self.drops.clone())));
        self.rows
            .borrow_mut()
            .insert(self.key, (context.id(), width.clone()));
        assert_ne!(self.fail.get(), Some(self.key), "row failed");
        Canvas(Fill).frame(Frame::new().width(width.get()).height(10.0))
    }
}

fn collection(
    graph: &mut InstanceGraph,
    items: State<Vec<u64>>,
    rows: Rows,
    drops: Rc<Cell<usize>>,
    fail: State<Option<u64>>,
) -> crate::ir::Handle {
    graph.mount_body(move |_| {
        ui! {
            VStack(2.0) {
                ForEach(items.get(), |item| *item) { |item|
                    compose(Row { key: item, rows: rows.clone(), drops: drops.clone(), fail: fail.clone() })
                }
            }
        }
    }, Environment::default()).unwrap()
}

fn address(graph: &InstanceGraph, id: ViewId) -> Address {
    graph.address(Key::new("ui.ir.node", id.0)).unwrap()
}

#[test]
fn keyed_items_retain_state_when_reordered_and_release_it_when_removed() {
    let mut graph = InstanceGraph::default();
    let items = State::new(vec![1, 2, 3]);
    let rows = Rows::default();
    let drops = Rc::new(Cell::new(0));
    let root = collection(
        &mut graph,
        items.clone(),
        rows.clone(),
        drops.clone(),
        State::default(),
    );
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.len(), 4);
    let original = rows.borrow().clone();
    original[&2].1.set(75.0);
    let update = advance(&mut graph);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(update.work.topology_visited, 0);
    assert_eq!(advance(&mut graph).work.owners_evaluated, 0);

    items.set(vec![3, 1, 2]);
    advance(&mut graph);
    for key in [1, 2, 3] {
        assert_eq!(rows.borrow()[&key].0, original[&key].0);
    }
    let positions = [3, 1, 2].map(|key| {
        let mut address = address(&graph, original[&key].0);
        address.local = Local(1);
        let bounds = graph.geometry(address).unwrap().layout;
        if key == 2 {
            assert_eq!(bounds.width, 75.0);
        }
        bounds.y
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(drops.get(), 0);

    items.set(vec![3, 1]);
    advance(&mut graph);
    assert_eq!(graph.len(), 3);
    assert_eq!(drops.get(), 1);
    assert!(
        graph
            .address(Key::new("ui.ir.node", original[&2].0.0))
            .is_none()
    );
    original[&2].1.set(99.0);
    assert!(!graph.has_pending_inputs());
    items.set(vec![3, 1, 2]);
    advance(&mut graph);
    assert_ne!(rows.borrow()[&2].0, original[&2].0);
    assert_eq!(rows.borrow()[&2].1.get(), 20.0);
    graph.set_root(None).unwrap();
    advance(&mut graph);
    assert_eq!(drops.get(), 4);
}

#[test]
fn same_typed_conditional_arms_have_distinct_lifetimes() {
    let mut graph = InstanceGraph::default();
    let selected = State::new(true);
    let read = selected.clone();
    let rows = Rows::default();
    let reported = rows.clone();
    let drops = Rc::new(Cell::new(0));
    let retired = drops.clone();
    let root = graph.mount_body(move |_| {
        ui! {
            if read.get() {
                compose(Row { key: 1, rows: reported.clone(), drops: retired.clone(), fail: State::default() })
            } else {
                compose(Row { key: 1, rows: reported.clone(), drops: retired.clone(), fail: State::default() })
            }
        }
    }, Environment::default()).unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let first = rows.borrow()[&1].clone();
    first.1.set(90.0);
    advance(&mut graph);
    selected.set(false);
    advance(&mut graph);
    assert_eq!(drops.get(), 1);
    assert_ne!(rows.borrow()[&1].0, first.0);
    assert_eq!(rows.borrow()[&1].1.get(), 10.0);
    first.1.set(120.0);
    assert!(!graph.has_pending_inputs());
    selected.set(true);
    advance(&mut graph);
    assert_eq!(drops.get(), 2);
    assert_ne!(rows.borrow()[&1].0, first.0);
}

#[test]
fn nested_optional_and_match_only_observe_the_selected_content() {
    let mut graph = InstanceGraph::default();
    let mode = State::new(Some(0));
    let a = State::new("First".to_string());
    let b = State::new("Second".to_string());
    let c = State::new("Third".to_string());
    let (selected, first, second, third) = (mode.clone(), a.clone(), b.clone(), c.clone());
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    VStack(2.0) {
                        Text("Heading")
                        if let Some(mode) = selected.get() {
                            match mode {
                                0 => Text(first.get()),
                                1 => { Text(second.get()) Text("Details") },
                                _ => HStack(3.0) { Text(third.get()) Text("Tail") },
                            }
                        }
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let initial = graph.len();
    b.set("Hidden".into());
    c.set("Hidden".into());
    assert!(!graph.has_pending_inputs());
    mode.set(Some(1));
    advance(&mut graph);
    assert!(graph.len() >= initial);
    a.set("Now hidden".into());
    assert!(!graph.has_pending_inputs());
    b.set("Visible".into());
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    mode.set(None);
    advance(&mut graph);
    assert_eq!(graph.len(), 1);
    b.set("Hidden again".into());
    assert!(!graph.has_pending_inputs());
    mode.set(Some(2));
    advance(&mut graph);
    c.set("Visible".into());
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
}

#[test]
fn duplicate_keys_and_panicking_rows_keep_the_published_ui_and_can_retry() {
    let mut graph = InstanceGraph::default();
    let items = State::new(vec![1, 2]);
    let rows = Rows::default();
    let drops = Rc::new(Cell::new(0));
    let fail = State::new(None);
    let root = collection(
        &mut graph,
        items.clone(),
        rows.clone(),
        drops.clone(),
        fail.clone(),
    );
    graph.set_root(Some(root)).unwrap();
    let published = advance(&mut graph).published;
    let original = rows.borrow().clone();
    items.set(vec![1, 1]);
    assert_eq!(
        graph.advance((400, 300), 1.0, None).err(),
        Some("duplicate collection key")
    );
    assert_eq!(graph.len(), 3);
    assert!(graph.has_pending_inputs());
    items.set(vec![2, 1, 3]);
    fail.set(Some(2));
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| advance(&mut graph))).is_err());
    assert!(graph.has_pending_inputs());
    assert_eq!(graph.len(), 3);
    assert_eq!(drops.get(), 0);
    for key in [1, 2] {
        address(&graph, original[&key].0);
    }
    fail.set(None);
    let update = advance(&mut graph);
    assert!(update.published > published);
    assert_eq!(graph.len(), 4);
    for key in [1, 2] {
        assert_eq!(rows.borrow()[&key].0, original[&key].0);
    }
    assert_eq!(drops.get(), 0);
}

#[test]
fn outer_hit_policy_crosses_dynamic_boundaries_but_stops_at_layout_roots() {
    use crate::input::{EventHandler, InputOptions};
    for physical_root in [false, true] {
        let mut graph = InstanceGraph::default();
        let enabled = State::new(true);
        let read = enabled.clone();
        let root = graph.mount_body(move |_| {
            let content = ui! {
                if physical_root {
                    VStack(0.0) {
                        ForEach([0_u64], |key| *key) { |_|
                            Canvas(Fill).frame(Frame::new().width(80.0).height(20.0)).hit_testing(false)
                        }
                    }
                } else {
                    ForEach([0_u64], |key| *key) { |_|
                        Canvas(Fill).frame(Frame::new().width(80.0).height(20.0)).hit_testing(false)
                    }
                }
            };
            content.hit_testing(read.get())
        }, Environment::default()).unwrap();
        graph.set_root(Some(root)).unwrap();
        advance(&mut graph);
        let painted = graph.scene().frame().instances()[0].program.key().owner;
        let target = graph.address(painted).unwrap();
        graph
            .bind_input(
                target,
                true,
                Some(InputOptions::new(EventHandler::new(|_, _| {}))),
            )
            .unwrap();
        advance(&mut graph);
        assert_eq!(
            graph.input().hit_test((10.0, 10.0)).is_some(),
            !physical_root
        );
        enabled.set(false);
        advance(&mut graph);
        assert!(graph.input().hit_test((10.0, 10.0)).is_none());
        enabled.set(true);
        advance(&mut graph);
        assert_eq!(
            graph.input().hit_test((10.0, 10.0)).is_some(),
            !physical_root
        );
    }
}

#[test]
fn conditional_collection_pixels_match_direct_layout_across_updates_and_scales() {
    use crate::{runtime::ViewTree, view as reference};
    use reference::ViewExt as _;

    #[derive(PartialEq)]
    struct Reference {
        mode: State<u32>,
    }
    impl reference::View for Reference {
        fn body(&self, _: &mut ViewContext<'_>) -> impl reference::IntoElement {
            let mode = self.mode.get();
            crate::runtime::reference::prelude::ui! {
                reference::VStack::new(3.0) {
                    Text("Header")
                    if mode != 0 {
                        for index in 0..mode {
                            reference::HStack::new(4.0) {
                                Text(format!("Item {index}"))
                                Text("Pending")
                            }.id(index as u64)
                        }
                    }
                    Text("Footer")
                }
            }
        }
    }
    let mode = State::new(2);
    let read = mode.clone();
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| {
                let mode = read.get();
                ui! {
                    VStack(3.0) {
                        Text("Header")
                        if mode != 0 {
                            ForEach(0..mode, |index| *index as u64) { |index|
                                HStack(4.0) {
                                    Text(format!("Item {index}"))
                                    Text("Pending")
                                }
                            }
                        }
                        Text("Footer")
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    let mut reference = ViewTree::new(Reference { mode: mode.clone() }, || {});
    for scale in [1.0, 1.2, 1.5] {
        for count in [2, 0, 3, 1] {
            mode.set(count);
            graph.advance((400, 300), scale, None).unwrap();
            reference.update((400, 300), scale).unwrap();
            assert!(
                super::modifiers::pixels(graph.scene())
                    == super::modifiers::pixels(reference.scene()),
                "mode {count}, scale {scale}"
            );
        }
    }
}
