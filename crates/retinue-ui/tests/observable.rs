use retinue_ui::{
    prelude::*,
    state::{Observation, Unavailable},
};
use std::{cell::Cell, rc::Rc};

mod observable_model {
    pub mod lifecycle;
    pub mod types;
}

#[observable]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Equipment {
    name: String,
    rank: u32,
    bonus: u32,
    #[observation(ignore)]
    request_generation: usize,
}

impl Equipment {
    fn total(&self) -> u32 {
        self.rank.get() + self.bonus.get()
    }
    fn rename(&self, name: &str) {
        self.name.set(name.to_owned());
    }
    fn advance(&self) {
        self.rank.update(|rank| *rank += 1);
    }
}

fn equipment(name: &str) -> Equipment {
    Equipment::new(name.to_owned(), 1, 2, 0)
}

#[test]
fn fields_computed_methods_and_actions_use_one_registrar() {
    let model = equipment("Boar");
    let wakes = Rc::new(Cell::new(0));
    let mut observer = Observation::new({
        let wakes = wakes.clone();
        move || wakes.set(wakes.get() + 1)
    });
    assert_eq!(observer.track(|| model.total()), 3);
    assert_eq!(observer.dependency_count(), 2);
    model.rename("Boar Prime");
    model.request_generation.set(1);
    assert!(!observer.is_dirty());
    model.advance();
    model.bonus.set(3);
    assert_eq!(wakes.get(), 1);
    assert_eq!(observer.track(|| model.total()), 5);
    assert!(!model.rank.set(2));
    assert!(!observer.is_dirty());
    let shared = model.clone();
    assert_eq!(model, shared);
    assert_ne!(model, Equipment::new("Boar Prime".into(), 2, 3, 1));
    shared.rank.set(5);
    assert_eq!(model.rank.get(), 5);
}

#[test]
fn property_lookup_bindings_and_debug_do_not_observe() {
    let model = equipment("Boar");
    let mut observer = Observation::new(|| {});
    let binding = observer.track(|| {
        let _property = &model.rank;
        let _copy = model.clone();
        let _debug = format!("{model:?}");
        model.name.binding()
    });
    assert_eq!(observer.dependency_count(), 0);
    binding.set("Boar Prime".into()).unwrap();
    assert!(!observer.is_dirty());
    observer.track(|| binding.with(|name| assert_eq!(name, "Boar Prime")).unwrap());
    model.rename("Boar");
    assert!(observer.is_dirty());
}

#[observable]
struct Selection {
    item: Option<Equipment>,
    use_bonus: bool,
}

#[test]
fn conditional_reads_follow_the_selected_model_without_old_dependencies() {
    let a = equipment("Boar");
    let b = equipment("Vasto");
    let selection = Selection::new(Some(a.clone()), false);
    let mut observer = Observation::new(|| {});
    let read = || {
        let selected = selection.item.read();
        let item = selected.as_ref().unwrap();
        if selection.use_bonus.get() {
            item.total()
        } else {
            item.rank.get()
        }
    };
    assert_eq!(observer.track(read), 1);
    a.bonus.set(9);
    assert!(!observer.is_dirty());
    selection.use_bonus.set(true);
    assert_eq!(observer.track(read), 10);
    selection.item.set(Some(b.clone()));
    assert_eq!(observer.track(read), 3);
    a.rank.set(5);
    assert!(!observer.is_dirty());
    b.bonus.set(7);
    assert!(observer.is_dirty());
}

#[test]
fn editing_a_nested_binding_follows_replacement_and_missing_targets() {
    let a = equipment("Boar");
    let b = equipment("Vasto");
    let selection = Selection::new(Some(a.clone()), false);
    let name = selection
        .item
        .binding()
        .try_follow(|item| item.as_ref().map(|item| item.name.clone()));
    name.set("Boar Prime".into()).unwrap();
    assert_eq!(&*a.name.read(), "Boar Prime");
    selection.item.set(Some(b.clone()));
    name.update(|value| value.push_str(" Prime")).unwrap();
    assert_eq!(&*b.name.read(), "Vasto Prime");
    assert_eq!(&*a.name.read(), "Boar Prime");
    selection.item.set(None);
    assert_eq!(name.set("Lost edit".into()), Err(Unavailable));
    let mut observer = Observation::new(|| {});
    observer.track(|| assert!(!name.is_available()));
    selection.item.set(Some(a.clone()));
    assert!(observer.is_dirty());
    assert_eq!(name.get().unwrap(), "Boar Prime");
}

#[observable]
struct Collection {
    items: Vec<(u64, Equipment)>,
}

#[test]
fn keyed_binding_does_not_edit_the_replacement_array_occupant() {
    let a = equipment("Boar");
    let b = equipment("Vasto");
    let model = Collection::new(vec![(1, a.clone()), (2, b.clone())]);
    let rank = model.items.binding().try_follow(|items| {
        items
            .iter()
            .find(|(id, _)| *id == 2)
            .map(|(_, item)| item.rank.clone())
    });
    model.items.update(|items| items.reverse());
    rank.set(20).unwrap();
    assert_eq!(b.rank.get(), 20);
    assert_eq!(a.rank.get(), 1);
    model.items.update(|items| {
        items.remove(0);
    });
    assert_eq!(rank.set(30), Err(Unavailable));
    assert_eq!(a.rank.get(), 1);
}

#[observable]
struct IgnoredChild {
    #[observation(ignore)]
    item: Equipment,
}

#[test]
fn ignored_fields_do_not_suppress_transitive_observable_reads() {
    let item = equipment("Boar");
    let model = IgnoredChild::new(item.clone());
    let mut observer = Observation::new(|| {});
    observer.track(|| model.item.read().total());
    assert_eq!(observer.dependency_count(), 2);
    item.rank.set(2);
    assert!(observer.is_dirty());
    observer.track(|| model.item.read().total());
    model.item.set(equipment("Vasto"));
    assert!(!observer.is_dirty());
}

#[test]
fn generated_field_bindings_preserve_animation_provenance() {
    let model = equipment("Boar");
    let mut observer = Observation::new(|| {});
    observer.track(|| model.rank.get());
    let animation = retinue_ui::transition::Animation::new(std::time::Duration::from_millis(80));
    with_animation(animation, || {
        model.rank.binding().set(10).unwrap();
    });
    assert_eq!(
        observer.pending_mutation().unwrap().transaction.animation,
        Some(animation)
    );
}
