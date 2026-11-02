use super::super::*;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

pub struct Payload(Vec<u8>);

#[observable]
struct Data<T> {
    value: T,
    #[observation(ignore)]
    retained: T,
}

#[test]
fn generic_models_borrow_and_mutate_without_payload_clone_or_comparison() {
    let model = Data::new(Payload(vec![7; 65_536]), Payload(vec![4; 8]));
    let copy = model.clone();
    assert!(model == copy);
    let address = model.value.read().0.as_ptr();
    assert_eq!(copy.value.read().0.as_ptr(), address);
    let mut observer = Observation::new(|| {});
    observer.track(|| assert_eq!(model.value.read().0.len(), 65_536));
    copy.value.update(|value| value.0.push(9));
    assert!(observer.is_dirty());
    observer.track(|| model.value.read().0.len());
    model.retained.update(|value| value.0.push(5));
    assert!(!observer.is_dirty());
    let previous = model.value.replace(Payload(vec![]));
    assert_eq!(previous.0.len(), 65_537);
}

#[observable]
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
struct Defaults<T = String, const N: usize = 2>
where
    T: Default,
{
    pub value: T,
    pub bytes: Vec<[u8; N]>,
    #[observation(ignore)]
    pub generation: usize,
}

#[test]
fn requested_default_debug_and_hash_keep_handle_semantics() {
    let model = Defaults::<String>::default();
    let copy = model.clone();
    let other = Defaults::<String>::default();
    assert_eq!(model, copy);
    assert_ne!(model, other);
    let hash = |model: &Defaults| {
        let mut hasher = DefaultHasher::new();
        model.hash(&mut hasher);
        hasher.finish()
    };
    let before = hash(&model);
    model.value.set("updated".into());
    assert_eq!(hash(&model), before);
    assert_eq!(hash(&copy), before);
    let mut observer = Observation::new(|| {});
    observer.track(|| assert!(format!("{model:?}").contains("updated")));
    assert_eq!(observer.dependency_count(), 0);
}

#[observable]
struct Chain {
    next: Option<Self>,
}

#[test]
fn recursive_self_fields_are_handles_to_the_original_model() {
    let tail = Chain::new(None);
    let head = Chain::new(Some(tail.clone()));
    assert!(head.next.read().as_ref().unwrap() == &tail);
}

#[observable]
struct Empty {}

#[test]
fn empty_models_still_have_distinct_retained_identities() {
    let a = Empty::new();
    assert!(a == a.clone());
    assert!(a != Empty::new());
}

pub trait Source {
    type Item: 'static;
}
pub struct OwnedSource;
impl Source for OwnedSource {
    type Item = Payload;
}

#[observable]
pub struct Associated<T: Source> {
    pub item: T::Item,
}

#[test]
fn associated_types_use_the_declared_bounds() {
    let model = Associated::<OwnedSource>::new(Payload(vec![1]));
    assert_eq!(model.item.read().0, [1]);
}

#[observable]
struct StaticReference<'a> {
    value: &'a str,
}

#[test]
fn explicitly_static_borrowed_values_remain_usable() {
    let model = StaticReference::new("static");
    assert_eq!(model.value.get(), "static");
}

mod public_model {
    use super::*;
    #[observable]
    pub struct Visible {
        pub shown: u32,
        private: u32,
    }
    impl Visible {
        pub fn sum(&self) -> u32 {
            self.shown.get() + self.private.get()
        }
    }
}

#[test]
fn declared_field_visibility_and_inherent_model_methods_are_preserved() {
    let model = public_model::Visible::new(1, 2);
    assert_eq!(model.shown.get(), 1);
    assert_eq!(model.sum(), 3);
}
