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
