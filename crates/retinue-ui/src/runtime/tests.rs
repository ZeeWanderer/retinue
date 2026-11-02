use super::*;
use crate::{prelude::*, transition::Animation};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
    time::Duration,
};

mod environment;
mod identity;
mod projection;

fn update(tree: &mut ViewTree) -> Damage {
    tree.update((200, 120), 1.0).unwrap()
}
fn tile(value: u32) -> Element {
    Rectangle::new(Color::rgb(value)).frame(24.0, 20.0)
}

#[derive(Clone, PartialEq)]
struct Counter {
    value: State<u32>,
    calls: Rc<Cell<usize>>,
}

impl View for Counter {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        self.calls.set(self.calls.get() + 1);
        tile(self.value.get())
    }
}

fn counter(value: u32) -> Counter {
    Counter {
        value: State::new(value),
        calls: Rc::default(),
    }
}

#[test]
fn only_dirty_bodies_run_and_idle_does_not_touch_the_scene() {
    let a = counter(0xaa0000);
    let b = counter(0x00aa00);
    let wakes = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(HStack::new(4.0, (a.clone(), b.clone())), {
        let wakes = wakes.clone();
        move || wakes.set(wakes.get() + 1)
    });
    update(&mut tree);
    let generation = tree.frame().generation();
    assert!(tree.scene().stats.layout_computations > 0);
    let visible = tree.scene().stats.visible_chunks;
    assert_eq!(update(&mut tree), Damage::None);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert_eq!(tree.scene().stats.recordings, 0);
    assert_eq!(tree.scene().stats.nodes, 0);
    assert_eq!(tree.scene().stats.visible_chunks, visible);
    assert_eq!(tree.frame().generation(), generation);
    assert_eq!((a.calls.get(), b.calls.get()), (1, 1));
    assert!(!tree.needs_update());
    a.value.set(0xabcdef);
    a.value.set(0x111111);
    assert_eq!(wakes.get(), 1);
    update(&mut tree);
    assert_eq!((a.calls.get(), b.calls.get()), (2, 1));
    assert_eq!(tree.stats.bodies, 1);
    assert_eq!(tree.stats.projected, 1);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert_eq!(tree.frame().generation(), generation + 1);
    assert_eq!(update(&mut tree), Damage::default());
    assert_eq!(tree.stats.bodies, 0);
    assert_eq!(tree.stats.projected, 0);
    assert!(!tree.needs_update());
}

#[derive(Clone, PartialEq)]
struct Conditional {
    choose: State<bool>,
    left: Counter,
    right: Counter,
}

impl View for Conditional {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        if self.choose.get() {
            self.left.clone()
        } else {
            self.right.clone()
        }
    }
}

#[test]
fn active_branch_replaces_dependencies_and_dead_changes_do_not_wake() {
    let view = Conditional {
        choose: State::new(true),
        left: counter(1),
        right: counter(2),
    };
    let mut tree = ViewTree::new(view.clone(), || {});
    update(&mut tree);
    view.choose.set(false);
    view.left.value.set(3);
    update(&mut tree);
    assert_eq!((view.left.calls.get(), view.right.calls.get()), (1, 1));
    view.left.value.set(4);
    assert!(!tree.needs_update());
    view.right.value.set(5);
    assert!(tree.needs_update());
}

#[derive(Clone, PartialEq)]
struct Parent {
    tick: State<u32>,
    child: Counter,
}

impl View for Parent {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        VStack::new(self.tick.get() as f32, (self.child.clone(), tile(0x0000aa)))
    }
}

#[test]
fn equal_child_inputs_skip_body_on_parent_change() {
    let parent = Parent {
        tick: State::new(1),
        child: counter(8),
    };
    let mut tree = ViewTree::new(parent.clone(), || {});
    update(&mut tree);
    parent.tick.set(4);
    update(&mut tree);
    assert_eq!(tree.stats.bodies, 1);
    assert_eq!(parent.child.calls.get(), 1);
    assert_eq!(tree.stats.mounted, 0);
    assert_eq!(tree.stats.unmounted, 0);
}

#[derive(Clone, PartialEq)]
struct Mutating {
    value: State<u32>,
}

impl View for Mutating {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let value = self.value.get();
        if value < 2 {
            self.value.set(value + 1);
        }
        tile(value)
    }
}

#[test]
fn body_writes_defer_instead_of_recursing_or_disappearing() {
    let value = State::new(0);
    let mut tree = ViewTree::new(
        Mutating {
            value: value.clone(),
        },
        || {},
    );
    update(&mut tree);
    assert_eq!(value.get(), 1);
    assert_eq!(tree.stats.bodies, 1);
    assert!(tree.needs_update());
    update(&mut tree);
    assert_eq!(value.get(), 2);
    assert!(tree.needs_update());
    update(&mut tree);
    assert!(!tree.needs_update());
}

#[derive(Clone, PartialEq)]
struct Panicking {
    value: State<u32>,
    fail: Rc<Cell<bool>>,
}

impl View for Panicking {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let _owned = context.state("owned", || 1);
        let value = self.value.get();
        assert!(!self.fail.replace(false), "intentional body failure");
        tile(value)
    }
}

#[test]
fn panic_keeps_previous_frame_and_retries_the_same_view() {
    let view = Panicking {
        value: State::new(3),
        fail: Rc::new(Cell::new(false)),
    };
    let mut tree = ViewTree::new(view.clone(), || {});
    update(&mut tree);
    let root = tree.root();
    let generation = tree.frame().generation();
    view.fail.set(true);
    view.value.set(7);
    assert!(catch_unwind(AssertUnwindSafe(|| update(&mut tree))).is_err());
    assert_eq!(tree.frame().generation(), generation);
    assert!(tree.needs_update());
    update(&mut tree);
    assert!(!tree.needs_update());
    assert_eq!(tree.root(), root);
    assert_eq!(tree.stats.bodies, 1);
}

#[test]
fn deferred_body_keeps_the_latest_contributing_transaction() {
    let view = counter(1);
    let mut tree = ViewTree::new(view.clone(), || {});
    update(&mut tree);
    let animation = Animation::new(Duration::from_millis(150));
    with_animation(animation, || {
        view.value.set(2);
    });
    update(&mut tree);
    assert_eq!(
        tree.last_mutation(tree.root().unwrap())
            .unwrap()
            .transaction
            .animation,
        Some(animation)
    );
    with_animation(animation, || {
        view.value.set(3);
    });
    view.value.set(4);
    update(&mut tree);
    assert_eq!(
        tree.last_mutation(tree.root().unwrap())
            .unwrap()
            .transaction
            .animation,
        None
    );
}
