use super::*;

type Captures = Rc<RefCell<HashMap<u64, (ViewId, State<u32>, Weak<()>)>>>;

#[derive(Clone)]
struct Row {
    value: u64,
    captures: Captures,
    initialized: Rc<Cell<usize>>,
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && Rc::ptr_eq(&self.captures, &other.captures)
            && Rc::ptr_eq(&self.initialized, &other.initialized)
    }
}

impl View for Row {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let token = context.owned("token", || {
            self.initialized.set(self.initialized.get() + 1);
            Rc::new(())
        });
        let count = context.state("count", || 1);
        self.captures.borrow_mut().insert(
            self.value,
            (context.id(), count.clone(), Rc::downgrade(&token)),
        );
        tile(count.get())
    }
}

#[derive(Clone)]
struct Rows {
    order: State<Vec<u64>>,
    captures: Captures,
    initialized: Rc<Cell<usize>>,
}

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
            && Rc::ptr_eq(&self.captures, &other.captures)
            && Rc::ptr_eq(&self.initialized, &other.initialized)
    }
}

impl View for Rows {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        HStack::new(
            4.0,
            self.order
                .read()
                .iter()
                .map(|&value| {
                    Row {
                        value,
                        captures: self.captures.clone(),
                        initialized: self.initialized.clone(),
                    }
                    .id(value)
                })
                .collect::<Vec<_>>(),
        )
    }
}

#[test]
fn keyed_reordering_preserves_state_removal_releases_it_and_reinsertion_initializes() {
    let rows = Rows {
        order: State::new(vec![1, 2, 3]),
        captures: Rc::default(),
        initialized: Rc::default(),
    };
    let mut tree = ViewTree::new(rows.clone(), || {});
    update(&mut tree);
    let (id, count, token) = rows.captures.borrow()[&2].clone();
    assert_eq!(rows.initialized.get(), 3);
    count.set(19);
    rows.order.set(vec![3, 2, 1]);
    update(&mut tree);
    assert_eq!(rows.captures.borrow()[&2].0, id);
    assert_eq!(rows.captures.borrow()[&2].1.get(), 19);
    assert_eq!(tree.stats.mounted, 0);
    rows.order.set(vec![3, 1]);
    count.set(20);
    update(&mut tree);
    assert!(token.upgrade().is_none());
    count.set(21);
    assert!(!tree.needs_update());
    rows.order.set(vec![3, 2, 1]);
    update(&mut tree);
    assert_ne!(rows.captures.borrow()[&2].0, id);
    assert_eq!(rows.captures.borrow()[&2].1.get(), 1);
    assert_eq!(rows.initialized.get(), 4);
}

#[test]
fn stable_child_order_updates_inputs_without_replacing_owned_state() {
    for count in [0, 1, 8] {
        let captures = Captures::default();
        let initialized = Rc::new(Cell::new(0));
        let content = |offset: u64| {
            HStack::new(
                4.0,
                (0..count)
                    .map(|index| {
                        let row = Row {
                            value: index + offset,
                            captures: captures.clone(),
                            initialized: initialized.clone(),
                        }
                        .into_element();
                        if index % 2 == 0 { row.id(index) } else { row }
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let mut tree = ViewTree::new(content(0), || {});
        update(&mut tree);
        let original: Vec<_> = (0..count)
            .map(|index| captures.borrow()[&index].clone())
            .collect();
        for (_, state, _) in &original {
            state.set(19);
        }
        tree.set_root(content(100));
        update(&mut tree);
        assert_eq!(initialized.get(), count as usize);
        assert_eq!(tree.stats.mounted, 0);
        assert_eq!(tree.stats.unmounted, 0);
        for (index, (id, state, token)) in original.iter().enumerate() {
            let after = &captures.borrow()[&(index as u64 + 100)];
            assert_eq!(after.0, *id);
            assert!(after.1 == *state);
            assert_eq!(after.1.get(), 19);
            assert!(token.ptr_eq(&after.2));
        }
    }
}

#[derive(Clone)]
struct ComparedInput {
    value: u32,
    comparisons: Rc<Cell<usize>>,
    bodies: Rc<RefCell<Vec<u32>>>,
}

impl PartialEq for ComparedInput {
    fn eq(&self, other: &Self) -> bool {
        self.comparisons.set(self.comparisons.get() + 1);
        self.value == other.value
            && Rc::ptr_eq(&self.comparisons, &other.comparisons)
            && Rc::ptr_eq(&self.bodies, &other.bodies)
    }
}

impl View for ComparedInput {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        self.bodies.borrow_mut().push(self.value);
        tile(self.value)
    }
}

#[test]
fn changed_input_is_not_compared_again_during_refresh() {
    let mut input = ComparedInput {
        value: 1,
        comparisons: Rc::default(),
        bodies: Rc::default(),
    };
    let mut tree = ViewTree::new(input.clone(), || {});
    update(&mut tree);
    input.comparisons.set(0);
    input.value = 2;
    tree.set_input(tree.root().unwrap(), input.clone().into_element(), None);
    assert_eq!(input.comparisons.get(), 1);
    update(&mut tree);
    assert_eq!(input.comparisons.get(), 1);
    assert_eq!(*input.bodies.borrow(), [1, 2]);
}

#[test]
fn pending_input_can_return_to_applied_value_without_reevaluating() {
    let mut input = ComparedInput {
        value: 1,
        comparisons: Rc::default(),
        bodies: Rc::default(),
    };
    let mut tree = ViewTree::new(input.clone(), || {});
    update(&mut tree);
    let root = tree.root().unwrap();
    let generation = tree.frame().generation();
    for value in [2, 3, 1] {
        input.value = value;
        tree.set_input(root, input.clone().into_element(), None);
    }
    assert_eq!(update(&mut tree), Damage::None);
    assert_eq!(*input.bodies.borrow(), [1]);
    assert_eq!(tree.frame().generation(), generation);
    assert!(!tree.needs_update());

    for value in [2, 1, 4] {
        input.value = value;
        tree.set_input(root, input.clone().into_element(), None);
    }
    update(&mut tree);
    assert_eq!(*input.bodies.borrow(), [1, 4]);
}

#[test]
fn descendant_inputs_are_compared_once_regardless_of_layout_depth() {
    for depth in [1, 8, 32] {
        let comparisons = Rc::new(Cell::new(0));
        let bodies = Rc::new(RefCell::new(Vec::new()));
        let content = |value| {
            let mut element = ComparedInput {
                value,
                comparisons: comparisons.clone(),
                bodies: bodies.clone(),
            }
            .into_element();
            for _ in 0..depth {
                element = VStack::new(0.0, (element,)).into_element();
            }
            element
        };
        let mut tree = ViewTree::new(content(1), || {});
        update(&mut tree);
        for value in [1, 2, 2, 3] {
            comparisons.set(0);
            tree.set_input(tree.root().unwrap(), content(value), None);
            update(&mut tree);
            assert_eq!(comparisons.get(), 1, "depth {depth}, value {value}");
            assert_eq!(tree.stats.mounted, 0);
            assert_eq!(tree.stats.unmounted, 0);
        }
        assert_eq!(*bodies.borrow(), [1, 2, 3]);
    }
}

#[test]
fn published_instances_do_not_retain_recursive_descriptions() {
    let input = counter(1);
    let child = input.clone().into_element();
    let weak_child = Rc::downgrade(&child.kind);
    let root = VStack::new(0.0, (child,)).into_element();
    let weak_root = Rc::downgrade(&root.kind);
    let crate::view::Kind::Layout(layout, _) = &*root.kind else {
        unreachable!()
    };
    let weak_layout = Rc::downgrade(layout);
    let mut tree = ViewTree::new(root.clone(), || {});
    update(&mut tree);
    drop(root);
    assert!(weak_root.upgrade().is_none());
    assert!(weak_child.upgrade().is_none());
    let retained = tree.nodes[&tree.root().unwrap()].borrow();
    let crate::runtime::instance::Kind::Layout(layout) = &retained.input.kind else {
        unreachable!()
    };
    assert!(Rc::ptr_eq(layout, &weak_layout.upgrade().unwrap()));
    drop(retained);
    assert!(
        tree.nodes
            .values()
            .all(|node| node.borrow().pending_children.is_none())
    );
    input.value.set(2);
    update(&mut tree);
    assert_eq!(input.calls.get(), 2);
    assert_eq!(tree.stats.mounted, 0);
}

#[test]
fn pending_child_edits_replace_each_other_without_changing_parent_properties() {
    let comparisons = Rc::new(Cell::new(0));
    let bodies = Rc::new(RefCell::new(Vec::new()));
    let content = |value| {
        VStack::new(
            0.0,
            (ComparedInput {
                value,
                comparisons: comparisons.clone(),
                bodies: bodies.clone(),
            },),
        )
        .into_element()
    };
    let mut tree = ViewTree::new(content(1), || {});
    update(&mut tree);
    let root = tree.root().unwrap();
    for value in [2, 3, 1] {
        tree.set_input(root, content(value), None);
    }
    assert_eq!(update(&mut tree), Damage::None);
    assert_eq!(*bodies.borrow(), [1]);
    assert_eq!(comparisons.get(), 1);
    for value in [1, 3, 2] {
        tree.set_input(root, content(value), None);
    }
    update(&mut tree);
    assert_eq!(*bodies.borrow(), [1, 2]);
    assert_eq!(tree.stats.mounted, 0);
}

#[test]
fn invalid_pending_children_survive_retry_without_replacing_published_children() {
    let content = |duplicate| {
        VStack::new(
            0.0,
            (tile(1).id(1), tile(2).id(if duplicate { 1 } else { 2 })),
        )
    };
    let mut tree = ViewTree::new(content(false), || {});
    update(&mut tree);
    let generation = tree.frame().generation();
    let count = tree.node_count();
    tree.set_root(content(true));
    for _ in 0..2 {
        assert_eq!(
            tree.update((200, 120), 1.0),
            Err("duplicate sibling view identity")
        );
        assert_eq!(tree.frame().generation(), generation);
        assert_eq!(tree.node_count(), count);
        assert!(tree.needs_update());
    }
    tree.set_root(content(false));
    assert_eq!(update(&mut tree), Damage::None);
    assert!(!tree.needs_update());
    assert_eq!(tree.stats.mounted, 0);
    assert_eq!(tree.stats.unmounted, 0);
}

#[test]
fn same_position_and_key_do_not_retain_an_incompatible_view_type() {
    let captures = Captures::default();
    let initialized = Rc::new(Cell::new(0));
    let row = Row {
        value: 1,
        captures: captures.clone(),
        initialized,
    };
    let mut tree = ViewTree::new(HStack::new(4.0, (row.id(1), tile(2).id(2))), || {});
    update(&mut tree);
    let (_, state, token) = captures.borrow()[&1].clone();
    tree.set_root(HStack::new(4.0, (tile(1).id(1), tile(2).id(2))));
    update(&mut tree);
    assert!(token.upgrade().is_none());
    assert!(tree.stats.unmounted > 0);
    state.set(9);
    assert!(!tree.needs_update());
}

#[test]
fn explicit_root_identity_and_view_type_reset_owned_fields() {
    let captures = Captures::default();
    let initialized = Rc::new(Cell::new(0));
    let row = Row {
        value: 1,
        captures: captures.clone(),
        initialized: initialized.clone(),
    };
    let mut tree = ViewTree::new(row.clone().id(1), || {});
    update(&mut tree);
    let first = captures.borrow()[&1].clone();
    tree.set_root(row.clone().id(1));
    update(&mut tree);
    assert_eq!(tree.stats.bodies, 0);
    tree.set_root(row.id(2));
    update(&mut tree);
    assert!(first.2.upgrade().is_none());
    assert_eq!(initialized.get(), 2);
    let second = captures.borrow()[&1].clone();
    tree.set_root(tile(1).id(2));
    update(&mut tree);
    assert!(second.2.upgrade().is_none());
}

#[derive(PartialEq)]
struct InitializedFrom {
    seed: State<u32>,
    value: Rc<RefCell<Option<State<u32>>>>,
}

impl View for InitializedFrom {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let count = context.state("count", || self.seed.get());
        *self.value.borrow_mut() = Some(count.clone());
        tile(count.get())
    }
}

#[test]
fn initialization_is_untracked_but_later_owned_reads_are_tracked() {
    let seed = State::new(5);
    let value = Rc::new(RefCell::new(None));
    let mut tree = ViewTree::new(
        InitializedFrom {
            seed: seed.clone(),
            value: value.clone(),
        },
        || {},
    );
    update(&mut tree);
    seed.set(7);
    assert!(!tree.needs_update());
    value.borrow().as_ref().unwrap().set(8);
    assert!(tree.needs_update());
}

#[test]
fn duplicate_keys_are_rejected_and_retryable() {
    let rows = Rows {
        order: State::new(vec![1, 2]),
        captures: Rc::default(),
        initialized: Rc::default(),
    };
    let wakes = Rc::new(Cell::new(0));
    let mut tree = ViewTree::new(rows.clone(), {
        let wakes = wakes.clone();
        move || wakes.set(wakes.get() + 1)
    });
    update(&mut tree);
    let generation = tree.frame().generation();
    rows.order.set(vec![1, 1]);
    assert_eq!(
        tree.update((200, 120), 1.0),
        Err("duplicate sibling view identity")
    );
    assert_eq!(tree.frame().generation(), generation);
    assert!(tree.needs_update());
    assert_eq!(
        wakes.get(),
        1,
        "a failed update must not schedule an error loop"
    );
    rows.order.set(vec![1, 3]);
    assert_eq!(wakes.get(), 2);
    update(&mut tree);
    assert!(!tree.needs_update());
    assert_eq!(rows.initialized.get(), 3);
}

#[test]
fn dropping_a_tree_drops_subscriptions_not_the_supplied_model() {
    let view = counter(1);
    let wakes = Rc::new(Cell::new(0));
    let weak = {
        let retained = Rc::new(());
        let weak = Rc::downgrade(&retained);
        let mut tree = ViewTree::new(view.clone(), {
            let wakes = wakes.clone();
            move || {
                let _ = &retained;
                wakes.set(wakes.get() + 1);
            }
        });
        update(&mut tree);
        weak
    };
    assert!(weak.upgrade().is_none());
    view.value.set(2);
    assert_eq!(wakes.get(), 0);
}
