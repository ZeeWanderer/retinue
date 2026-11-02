use super::*;
use crate::state::{Binding, Observation, State};
use std::{cell::Cell, rc::Rc};

struct Scale;

impl EnvironmentKey for Scale {
    type Value = u32;

    fn default_value() -> u32 {
        100
    }
}

struct Contrast;

impl EnvironmentKey for Contrast {
    type Value = bool;

    fn default_value() -> bool {
        false
    }
}

#[test]
fn nearest_provider_wins_without_invalidating_shadowed_consumers() {
    let root = Environment::default();
    root.set::<Scale>(120);
    let left = root.child();
    let right = root.child();
    left.set::<Scale>(150);
    let leaf = left.child();
    let mut a = Observation::new(|| {});
    let mut b = Observation::new(|| {});
    assert_eq!(a.track(|| leaf.get::<Scale>()), 150);
    assert_eq!(b.track(|| right.get::<Scale>()), 120);
    root.set::<Scale>(130);
    assert!(!a.is_dirty());
    assert!(b.is_dirty());
    assert_eq!(left.remove::<Scale>(), Some(150));
    assert!(a.is_dirty());
    assert_eq!(a.track(|| leaf.get::<Scale>()), 130);
    assert_eq!(b.track(|| right.get::<Scale>()), 130);
    left.set::<Scale>(140);
    assert!(a.is_dirty());
    assert!(!b.is_dirty());
}

#[test]
fn defaults_and_absent_overrides_notice_new_providers() {
    let root = Environment::default();
    let middle = root.child();
    let child = middle.child();
    let mut observer = Observation::new(|| {});
    assert_eq!(observer.track(|| child.get::<Scale>()), 100);
    root.set::<Scale>(110);
    assert!(observer.is_dirty());
    assert_eq!(observer.track(|| child.get::<Scale>()), 110);
    middle.set::<Scale>(120);
    assert!(observer.is_dirty());
    assert_eq!(observer.track(|| child.get::<Scale>()), 120);
    root.set::<Scale>(130);
    assert!(!observer.is_dirty());
    middle.remove::<Scale>();
    assert_eq!(observer.track(|| child.get::<Scale>()), 130);
    root.remove::<Scale>();
    assert_eq!(observer.track(|| child.get::<Scale>()), 100);
}

#[test]
fn unrelated_keys_and_redundant_changes_do_not_wake_consumers() {
    let root = Environment::default();
    let child = root.child();
    child.set::<Scale>(110);
    let mut observer = Observation::new(|| {});
    observer.track(|| child.get::<Scale>());
    root.set::<Contrast>(true);
    assert!(!child.set::<Scale>(110));
    assert_eq!(root.remove::<Scale>(), None);
    assert!(!observer.is_dirty());
    assert_eq!(child.remove::<Scale>(), Some(110));
    assert!(observer.is_dirty());
}

#[derive(Clone, PartialEq)]
struct Player {
    name: State<String>,
    rank: State<u32>,
}

fn player(name: &str, rank: u32) -> Player {
    Player {
        name: State::new(name.into()),
        rank: State::new(rank),
    }
}

#[test]
fn model_provider_and_model_fields_are_independently_observed() {
    let root = Environment::default();
    let old = player("old", 1);
    let new = player("new", 10);
    root.provide(old.clone());
    let child = root.child();
    let mut observer = Observation::new(|| {});
    assert_eq!(
        observer.track(|| child.require::<Player>().unwrap().rank.get()),
        1
    );
    assert!(!root.provide(old.clone()));
    assert!(!observer.is_dirty());
    old.name.set("changed".into());
    assert!(!observer.is_dirty());
    old.rank.set(2);
    assert!(observer.is_dirty());
    root.provide(new.clone());
    assert_eq!(
        observer.track(|| child.require::<Player>().unwrap().rank.get()),
        10
    );
    old.rank.set(3);
    assert!(!observer.is_dirty());
    new.rank.set(11);
    assert!(observer.is_dirty());
}

#[test]
fn models_without_value_equality_can_be_explicitly_replaced() {
    #[derive(Clone)]
    struct Service(State<u32>);

    let root = Environment::default();
    root.replace_model(Service(State::new(1)));
    let mut observer = Observation::new(|| {});
    assert_eq!(
        observer.track(|| root.require::<Service>().unwrap().0.get()),
        1
    );
    root.replace_model(Service(State::new(2)));
    assert!(observer.is_dirty());
    assert_eq!(
        observer.track(|| root.require::<Service>().unwrap().0.get()),
        2
    );
}

#[test]
fn optional_and_required_model_access_share_missing_provider_tracking() {
    let root = Environment::default();
    let child = root.child();
    let mut optional = Observation::new(|| {});
    let mut required = Observation::new(|| {});
    assert!(optional.track(|| child.model::<Player>()).is_none());
    let error = required.track(|| child.require::<Player>()).err().unwrap();
    assert!(error.name.ends_with("Player"));
    root.provide(player("root", 1));
    assert!(optional.is_dirty());
    assert!(required.is_dirty());
    assert!(optional.track(|| child.model::<Player>()).is_some());
    root.remove_model::<Player>();
    assert!(optional.is_dirty());
}

struct Primary;
struct Secondary;

impl EnvironmentKey for Primary {
    type Value = Option<Player>;
    fn default_value() -> Self::Value {
        None
    }
}
impl EnvironmentKey for Secondary {
    type Value = Option<Player>;
    fn default_value() -> Self::Value {
        None
    }
}

#[test]
fn named_roles_of_the_same_type_do_not_collide_with_object_injection() {
    let root = Environment::default();
    root.set::<Primary>(Some(player("primary", 1)));
    root.set::<Secondary>(Some(player("secondary", 2)));
    root.provide(player("shared", 3));
    assert_eq!(root.get::<Primary>().unwrap().rank.get(), 1);
    assert_eq!(root.get::<Secondary>().unwrap().rank.get(), 2);
    assert_eq!(root.require::<Player>().unwrap().rank.get(), 3);
}

struct DefaultState;

impl EnvironmentKey for DefaultState {
    type Value = State<u32>;
    fn default_value() -> Self::Value {
        State::new(1)
    }
}

#[test]
fn defaults_have_root_scoped_identity_instead_of_a_new_model_per_read() {
    let root = Environment::default();
    let a = root.child();
    let b = root.child();
    let shared = root.get::<DefaultState>();
    assert_eq!(a.get::<DefaultState>(), shared);
    assert_eq!(b.get::<DefaultState>(), shared);
    assert_ne!(Environment::default().get::<DefaultState>(), shared);
    let mut observer = Observation::new(|| {});
    assert_eq!(observer.track(|| a.get::<DefaultState>().get()), 1);
    shared.set(2);
    assert!(observer.is_dirty());
    assert_eq!(observer.track(|| a.get::<DefaultState>().get()), 2);
    a.set::<DefaultState>(State::new(3));
    assert_eq!(observer.track(|| a.get::<DefaultState>().get()), 3);
    shared.set(4);
    assert!(!observer.is_dirty());
    a.remove::<DefaultState>();
    assert_eq!(observer.track(|| a.get::<DefaultState>().get()), 4);
}

#[test]
fn binding_resolves_the_current_environment_model_when_editing() {
    let root = Environment::default();
    let first = player("first", 1);
    let second = player("second", 2);
    let child = root.child();
    root.provide(first.clone());
    let scope = child.clone();
    let binding = Binding::resolve(move || scope.model::<Player>().map(|player| player.rank));
    child.provide(second.clone());
    binding.set(3).unwrap();
    assert_eq!(first.rank.get(), 1);
    assert_eq!(second.rank.get(), 3);
    child.remove_model::<Player>();
    binding.set(4).unwrap();
    assert_eq!(first.rank.get(), 4);
    root.remove_model::<Player>();
    assert!(!binding.is_available());
}

#[test]
fn notifications_release_provider_map_borrows_before_waking() {
    let root = Environment::default();
    let read = root.clone();
    let output = Rc::new(Cell::new(0));
    let received = output.clone();
    let mut observer = Observation::new(move || received.set(read.get::<Scale>()));
    observer.track(|| root.get::<Scale>());
    root.set::<Scale>(120);
    assert_eq!(output.get(), 120);
}

#[test]
fn provider_mutation_and_debug_do_not_subscribe_the_caller() {
    let root = Environment::default();
    let mut observer = Observation::new(|| {});
    observer.track(|| {
        root.set::<Scale>(120);
        root.remove::<Scale>();
        root.replace::<Scale>(130);
        root.provide(player("shared", 1));
        root.remove_model::<Player>();
        assert!(format!("{root:?}").contains("Environment"));
    });
    assert_eq!(observer.dependency_count(), 0);
}

#[test]
fn dropped_scopes_release_providers_even_with_a_retained_observer() {
    let root = Environment::default();
    let owner = Rc::new(());
    let mut observer = Observation::new(|| {});
    {
        let child = root.child();
        child.provide(owner.clone());
        observer.track(|| assert!(child.model::<Rc<()>>().is_some()));
        assert_eq!(Rc::strong_count(&owner), 2);
    }
    assert_eq!(Rc::strong_count(&owner), 1);
}
