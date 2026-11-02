use retinue_ui::{prelude::*, runtime::ViewTree, state::Observation};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[observable]
#[derive(Default, Debug)]
struct Model {
    value: usize,
}

struct Accent;
impl EnvironmentKey for Accent {
    type Value = f32;
    fn default_value() -> f32 {
        1.0
    }
}

#[derive(ViewProperties)]
struct Properties {
    #[owned(Model::new(7))]
    model: Model,
    counter: State<usize>,
    #[environment]
    shared: Model,
    #[environment(Accent)]
    accent: f32,
    #[environment]
    optional: Option<State<String>>,
}

#[derive(Clone, PartialEq)]
struct Probe {
    trigger: State<usize>,
    calls: Rc<Cell<usize>>,
    output: Rc<RefCell<Option<(Model, State<usize>, usize, f32, Option<String>)>>>,
}

impl Probe {
    fn new() -> Self {
        Self {
            trigger: State::new(0),
            calls: Rc::default(),
            output: Rc::default(),
        }
    }
}

impl View for Probe {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        self.calls.set(self.calls.get() + 1);
        self.trigger.get();
        let p = context.properties::<Properties>();
        let value = p.model.value.get() + p.counter.get() + p.shared.value.get();
        *self.output.borrow_mut() = Some((
            p.model,
            p.counter,
            value,
            p.accent,
            p.optional.map(|s| s.get()),
        ));
        Text::new(value.to_string())
    }
}

#[test]
fn owned_members_survive_recreation_and_environment_members_follow_the_nearest_provider() {
    let shared = Model::new(10);
    let nested = Model::new(30);
    let a = Probe::new();
    let b = Probe::new();
    let mut tree = ViewTree::new(
        HStack::new(
            8.0,
            (
                a.clone(),
                b.clone()
                    .environment_model(nested.clone())
                    .environment::<Accent>(2.0),
            ),
        ),
        || {},
    );
    tree.environment().provide(shared.clone());
    tree.update((400, 200), 1.0).unwrap();
    let first = a.output.borrow().as_ref().unwrap().clone();
    assert_eq!(first.2, 17);
    assert_eq!(b.output.borrow().as_ref().unwrap().2, 37);
    assert_ne!(first.0, b.output.borrow().as_ref().unwrap().0);
    first.0.value.set(8);
    first.1.set(4);
    a.trigger.set(1);
    tree.update((400, 200), 1.0).unwrap();
    assert_eq!(a.output.borrow().as_ref().unwrap().0, first.0);
    assert_eq!(a.output.borrow().as_ref().unwrap().2, 22);
    assert_eq!(tree.stats.bodies, 1);
    shared.value.set(11);
    tree.environment().set::<Accent>(3.0);
    tree.environment()
        .provide(State::new(String::from("available")));
    tree.update((400, 200), 1.0).unwrap();
    assert_eq!(a.output.borrow().as_ref().unwrap().3, 3.0);
    assert_eq!(b.output.borrow().as_ref().unwrap().3, 2.0);
    assert_eq!(
        b.output.borrow().as_ref().unwrap().4.as_deref(),
        Some("available")
    );
    tree.environment().provide(Model::new(50));
    tree.update((400, 200), 1.0).unwrap();
    let calls = a.calls.get();
    shared.value.set(99);
    assert!(!tree.needs_update());
    assert_eq!(a.calls.get(), calls);
}

#[derive(ViewProperties)]
struct Generic<T> {
    value: State<Option<T>>,
}

#[derive(PartialEq)]
struct GenericProbe;
impl View for GenericProbe {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        let a = cx.properties::<Generic<u32>>();
        let b = cx.properties::<Generic<String>>();
        assert_eq!(a.value.get(), None);
        assert_eq!(b.value.get(), None);
        empty()
    }
}

#[test]
fn generic_properties_have_distinct_storage_slots() {
    let mut tree = ViewTree::new(GenericProbe, || {});
    tree.update((80, 60), 1.0).unwrap();
}

#[derive(ViewProperties)]
struct Retained {
    value: Rc<()>,
}

#[test]
fn owned_members_are_released_on_unmount_and_reinitialized_on_remount() {
    #[derive(Clone)]
    struct Owner(Rc<RefCell<std::rc::Weak<()>>>);
    impl PartialEq for Owner {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }
    impl View for Owner {
        fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
            let p = cx.properties::<Retained>();
            *self.0.borrow_mut() = Rc::downgrade(&p.value);
            empty()
        }
    }
    let owner = Owner(Rc::default());
    let mut tree = ViewTree::new(owner.clone(), || {});
    tree.update((80, 60), 1.0).unwrap();
    let old = owner.0.borrow().clone();
    assert!(old.upgrade().is_some());
    tree.set_root(empty());
    tree.update((80, 60), 1.0).unwrap();
    assert!(old.upgrade().is_none());
    tree.set_root(owner.clone());
    tree.update((80, 60), 1.0).unwrap();
    assert!(owner.0.borrow().upgrade().is_some());
    assert!(!old.ptr_eq(&owner.0.borrow()));
}

#[test]
fn properties_are_optional_plain_members_and_models_work_without_a_view() {
    let props = Properties {
        model: Model::new(1),
        counter: State::new(0),
        shared: Model::new(2),
        accent: 1.0,
        optional: None,
    };
    let mut observation = Observation::new(|| {});
    observation.track(|| props.model.value.get());
    props.counter.set(5);
    assert!(!observation.is_dirty());
    props.model.value.set(3);
    assert!(observation.is_dirty());
}
