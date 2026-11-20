use super::*;
use std::cell::RefCell;

#[derive(PartialEq)]
struct Counter {
    initial: f32,
    states: Rc<RefCell<Vec<State<f32>>>>,
    ids: Rc<RefCell<Vec<crate::runtime::ViewId>>>,
}

#[view]
impl View for Counter {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let state = context.state("value", || self.initial);
        self.states.borrow_mut().push(state.clone());
        self.ids.borrow_mut().push(context.id());
        Canvas(Fill).frame(Frame::new().width(state.get()).height(20.0))
    }
}

#[test]
fn fixed_views_share_one_template_but_not_owned_state_or_identity() {
    let mut graph = InstanceGraph::default();
    let states = Rc::new(RefCell::new(Vec::new()));
    let ids = Rc::new(RefCell::new(Vec::new()));
    let reported = states.clone();
    let identities = ids.clone();
    let root = graph
        .mount_body(
            move |context| {
                let parent = context.state("value", || 500.0_f32);
                assert_eq!(parent.get(), 500.0);
                VStack(
                    4.0,
                    (
                        compose(Counter {
                            initial: 80.0,
                            states: reported.clone(),
                            ids: identities.clone(),
                        }),
                        compose(Counter {
                            initial: 120.0,
                            states: reported.clone(),
                            ids: identities.clone(),
                        }),
                    ),
                )
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert_eq!(graph.len(), 1);
    assert_eq!(graph.templates.len(), 1);
    assert_ne!(ids.borrow()[0], ids.borrow()[1]);
    let original = ids.borrow().clone();
    assert_eq!(states.borrow()[0].get(), 80.0);
    assert_eq!(states.borrow()[1].get(), 120.0);
    // Logical composite markers have identity, not layout boxes.
    assert!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(1)
            })
            .is_none()
    );
    assert_eq!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(2)
            })
            .unwrap()
            .layout
            .width,
        80.0
    );
    states.borrow()[0].set(100.0);
    let update = advance(&mut graph);
    assert_eq!(update.work.bindings_emitted, 1);
    assert_eq!(update.work.topology_visited, 0);
    assert_eq!(&ids.borrow()[2..], &original[..1]);
    assert_eq!(states.borrow()[2].get(), 100.0);
    assert_eq!(states.borrow().len(), 3);
    assert_eq!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(2)
            })
            .unwrap()
            .layout
            .width,
        100.0
    );
}

#[derive(PartialEq)]
struct NamedChild(State<f32>);

#[view]
impl View for NamedChild {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Canvas(Fill)
            .frame(Frame::new().width(self.0.get()).height(24.0))
            .interactive()
            .named(crate::Key::new("child", 0))
    }
}

#[test]
fn unchanged_child_does_not_hide_an_outer_override_on_the_same_node() {
    let width = State::new(30.0);
    let name = State::new(crate::Key::new("parent", 1));
    let (inner, outer) = (width.clone(), name.clone());
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| compose(NamedChild(inner.clone())).named(outer.get()),
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    let first = graph.unique(name.get()).unwrap().unwrap();
    let previous = name.get();
    name.set(crate::Key::new("parent", 2));
    assert_eq!(advance(&mut graph).work.bindings_emitted, 1);
    assert!(!graph.has_name(previous));
    assert!(!graph.has_name(crate::Key::new("child", 0)));
    assert_eq!(graph.unique(name.get()).unwrap(), Some(first));
    width.set(45.0);
    advance(&mut graph);
    assert_eq!(graph.geometry(first).unwrap().layout.width, 45.0);
    assert!(graph.geometry(first).unwrap().interactive);
}

struct Generic<T> {
    value: T,
}

#[view]
impl<T: ToString + 'static> View for Generic<T> {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Text(self.value.to_string())
    }
}

#[test]
fn generic_view_return_capture_is_generated_without_borrowing_the_view() {
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_view(Generic { value: 42_u32 }, Environment::default())
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    assert!(
        graph
            .geometry(Address {
                instance: root,
                local: Local(0)
            })
            .unwrap()
            .layout
            .width
            > 0.0
    );
}

#[derive(PartialEq)]
struct Children(State<Vec<u64>>);
#[view]
impl View for Children {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            ForEach(self.0.get(), |id| *id) { |id|
                Rectangle(Color::rgb(0x334455)).frame(Frame::new().width(20.0).height(10.0))
                    .named(crate::Key::new("retry.child", id))
            }
        }
    }
}

#[derive(PartialEq)]
struct Fallible(State<bool>);
#[view]
impl View for Fallible {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        assert!(!self.0.get(), "injected body failure");
        ()
    }
}

#[test]
fn discarded_partial_body_resolution_does_not_reuse_uncommitted_children() {
    let items = State::new(Vec::new());
    let fail = State::new(false);
    let mut graph = InstanceGraph::default();
    let read = items.clone();
    let failure = fail.clone();
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    VStack(0.0) {
                        compose(Children(read.clone()))
                        compose(Fallible(failure.clone()))
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    advance(&mut graph);
    items.set(vec![1, 2]);
    fail.set(true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| advance(&mut graph))).is_err()
    );
    graph.discard();
    fail.set(false);
    advance(&mut graph);
    for id in [1, 2] {
        let address = graph
            .unique(crate::Key::new("retry.child", id))
            .unwrap()
            .unwrap();
        assert_eq!(graph.geometry(address).unwrap().layout.width, 20.0);
    }
}
