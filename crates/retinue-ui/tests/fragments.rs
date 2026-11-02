use retinue_ui::{prelude::*, runtime::ViewTree};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Clone, Default, PartialEq)]
struct Probe {
    created: Rc<Cell<usize>>,
    values: Rc<RefCell<HashMap<&'static str, State<usize>>>>,
}

#[derive(PartialEq)]
struct Persistent {
    name: &'static str,
    probe: Probe,
}
impl View for Persistent {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let value = context.state("value", || {
            let value = self.probe.created.get() + 1;
            self.probe.created.set(value);
            value
        });
        self.probe
            .values
            .borrow_mut()
            .insert(self.name, value.clone());
        Text::new(format!("{}: {}", self.name, value.get()))
    }
}

#[derive(PartialEq)]
struct Form {
    extra: State<bool>,
    count: State<usize>,
    probe: Probe,
}
impl View for Form {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            VStack(8.0) {
                if self.extra.get() { Persistent { name: "extra", probe: self.probe.clone() } }
                Persistent { name: "middle", probe: self.probe.clone() }
                for i in 0..self.count.get() { Text(i.to_string()).id(i as u64) }
                Persistent { name: "tail", probe: self.probe.clone() }
            }
        }
    }
}

#[test]
fn conditional_and_loop_lengths_do_not_reset_later_sibling_state() {
    let extra = State::new(false);
    let count = State::new(0);
    let probe = Probe::default();
    let mut tree = ViewTree::new(
        Form {
            extra: extra.clone(),
            count: count.clone(),
            probe: probe.clone(),
        },
        || {},
    );
    tree.update((400, 400), 1.0).unwrap();
    let before = probe.values.borrow().clone();
    extra.set(true);
    count.set(3);
    tree.update((400, 400), 1.0).unwrap();
    for name in ["middle", "tail"] {
        assert_eq!(probe.values.borrow()[name], before[name]);
    }
    assert_eq!(probe.created.get(), 3);
    extra.set(false);
    count.set(1);
    tree.update((400, 400), 1.0).unwrap();
    for name in ["middle", "tail"] {
        assert_eq!(probe.values.borrow()[name], before[name]);
    }
}

#[derive(PartialEq)]
struct Branch(State<bool>);
impl View for Branch {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        if self.0.get() {
            Element::fragment((
                Rectangle::new(Color::rgb(0xff0000)).frame(30.0, 20.0),
                Rectangle::new(Color::rgb(0x00ff00)).frame(40.0, 20.0),
            ))
        } else {
            Rectangle::new(Color::rgb(0x0000ff)).frame(50.0, 20.0)
        }
    }
}

#[test]
fn body_can_switch_between_a_single_root_and_layout_transparent_siblings() {
    let state = State::new(false);
    let mut tree = ViewTree::new(Branch(state.clone()), || {});
    tree.update((300, 100), 1.0).unwrap();
    for value in [true, false, true, false] {
        state.set(value);
        tree.update((300, 100), 1.0).unwrap();
        assert_eq!(tree.frame().instances().len(), if value { 2 } else { 1 });
        let mut fresh = ViewTree::new(Branch(state.clone()), || {});
        fresh.update((300, 100), 1.0).unwrap();
        assert_eq!(
            tree.frame()
                .instances()
                .iter()
                .map(|instance| instance.visible_bounds())
                .collect::<Vec<_>>(),
            fresh
                .frame()
                .instances()
                .iter()
                .map(|instance| instance.visible_bounds())
                .collect::<Vec<_>>()
        );
    }
}

#[derive(PartialEq)]
struct Leaf(State<u32>);
impl View for Leaf {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Rectangle::new(Color::rgb(self.0.get())).frame(24.0, 20.0)
    }
}

#[test]
fn leaf_in_a_fragment_keeps_stack_placement_during_partial_updates() {
    let color = State::new(0x11aaff);
    let root = || {
        ZStack::new((
            Element::fragment((Leaf(color.clone()),)),
            Rectangle::new(Color::rgb(0xff1111)).frame(8.0, 8.0),
        ))
        .frame(100.0, 80.0)
    };
    let mut tree = ViewTree::new(root(), || {});
    tree.update((200, 160), 1.25).unwrap();
    let before: Vec<_> = tree
        .frame()
        .instances()
        .iter()
        .map(|instance| instance.visible_bounds())
        .collect();
    color.set(0x33bbff);
    tree.update((200, 160), 1.25).unwrap();
    assert_eq!(tree.stats.projected, 1);
    assert_eq!(
        before,
        tree.frame()
            .instances()
            .iter()
            .map(|instance| instance.visible_bounds())
            .collect::<Vec<_>>()
    );
}

#[derive(PartialEq)]
struct Tagged(State<u32>);
impl View for Tagged {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Element::fragment((Leaf(self.0.clone()).id("leaf"),))
            .role(retinue_ui::semantics::Role::Title)
            .interactive()
    }
}

#[test]
fn flattened_annotations_survive_incremental_child_updates() {
    use retinue_render::CacheHint;
    use retinue_ui::semantics::{Role, WindowAction};
    let color = State::new(0x1144ff);
    let root = || {
        Tagged(color.clone())
            .window_action(WindowAction::Move)
            .reuse(CacheHint::Stable)
    };
    let mut tree = ViewTree::new(root(), || {});
    for value in [0x1144ff, 0x22aaff, 0x3311ff] {
        color.set(value);
        tree.update((100, 80), 1.0).unwrap();
        let geometry = tree
            .geometry(tree.unique("leaf").unwrap().unwrap())
            .unwrap();
        assert_eq!(geometry.role, Role::Title);
        assert_eq!(geometry.window_action, Some(WindowAction::Move));
        assert!(geometry.interactive);
        assert!(tree.scene().hit_test((4.0, 4.0)).is_some());
        let mut fresh = ViewTree::new(root(), || {});
        fresh.update((100, 80), 1.0).unwrap();
        assert_eq!(
            geometry.layout,
            fresh
                .geometry(fresh.unique("leaf").unwrap().unwrap())
                .unwrap()
                .layout
        );
    }
}

#[derive(PartialEq)]
struct Decorated(State<bool>);
impl View for Decorated {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            Rectangle(Color::rgb(0x111111)).frame(50.0, 30.0)
                .background {
                    if self.0.get() { Rectangle(Color::rgb(0x222222)) }
                }
                .overlay {
                    if self.0.get() { Text("active") } else { Text("idle") }
                }
        }
    }
}

#[test]
fn unary_modifiers_accept_conditional_children_without_changing_primary_size() {
    let enabled = State::new(false);
    let mut tree = ViewTree::new(Decorated(enabled.clone()), || {});
    for value in [false, true, false] {
        enabled.set(value);
        tree.update((200, 160), 1.0).unwrap();
        assert!(tree.scene().layout_failure().is_none());
        let bounds = tree.geometry(tree.root().unwrap()).unwrap().layout;
        assert_eq!((bounds.width, bounds.height), (50.0, 30.0));
        assert_eq!(tree.frame().instances().len(), if value { 3 } else { 2 });
    }
}
