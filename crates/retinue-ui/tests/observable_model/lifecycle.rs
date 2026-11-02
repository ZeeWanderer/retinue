use super::super::*;
use retinue_ui::{runtime::ViewTree, state::untracked};
use std::cell::RefCell;

#[derive(Clone, PartialEq)]
struct OwnedView {
    trigger: State<bool>,
    initializations: Rc<Cell<usize>>,
    published: Rc<RefCell<Vec<Equipment>>>,
}

impl View for OwnedView {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let model = context.owned("equipment", || {
            self.initializations.set(self.initializations.get() + 1);
            equipment("Boar")
        });
        self.published.borrow_mut().push(model.clone());
        let color = if self.trigger.get() {
            Color::rgb(0x00aa00)
        } else {
            Color::rgb(0xaa0000)
        };
        Rectangle::new(color).frame(model.total() as f32 * 10.0, 20.0)
    }
}

#[test]
fn model_created_in_a_view_survives_body_updates_and_resets_with_identity() {
    let view = OwnedView {
        trigger: State::new(false),
        initializations: Rc::default(),
        published: Rc::default(),
    };
    let mut tree = ViewTree::new(view.clone().id(1), || {});
    tree.update((200, 100), 1.0).unwrap();
    let model = view.published.borrow()[0].clone();
    model.rank.set(3);
    tree.update((200, 100), 1.0).unwrap();
    view.trigger.set(true);
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(view.initializations.get(), 1);
    assert!(view.published.borrow().iter().all(|seen| seen == &model));
    tree.set_root(view.clone().id(2));
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(view.initializations.get(), 2);
    assert_ne!(view.published.borrow().last().unwrap(), &model);
    model.rank.set(9);
    assert!(!tree.needs_update());
}

#[derive(Clone, PartialEq)]
struct EnvironmentReader {
    readings: Rc<RefCell<Vec<u32>>>,
}

impl View for EnvironmentReader {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let item = context.environment().require::<Equipment>().unwrap();
        let total = item.total();
        self.readings.borrow_mut().push(total);
        Rectangle::new(Color::rgb(0xaabbcc)).frame(total as f32, 20.0)
    }
}

#[test]
fn inherited_and_overridden_models_keep_field_level_invalidation() {
    let a = equipment("Boar");
    let b = equipment("Vasto");
    let left = EnvironmentReader {
        readings: Rc::default(),
    };
    let right = EnvironmentReader {
        readings: Rc::default(),
    };
    let mut tree = ViewTree::new(
        HStack::new(
            2.0,
            (left.clone(), right.clone().environment_model(b.clone())),
        )
        .environment_model(a.clone()),
        || {},
    );
    tree.update((200, 100), 1.0).unwrap();
    a.rank.set(4);
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(&*left.readings.borrow(), &[3, 6]);
    assert_eq!(&*right.readings.borrow(), &[3]);
    assert_eq!(tree.stats.bodies, 1);
    a.rename("Boar Prime");
    assert!(!tree.needs_update());
    b.rank.set(6);
    tree.update((200, 100), 1.0).unwrap();
    assert_eq!(&*right.readings.borrow(), &[3, 8]);
}

#[test]
fn explicit_untracked_read_works_without_suppressing_normal_model_actions() {
    let model = equipment("Boar");
    let mut observer = Observation::new(|| {});
    observer.track(|| {
        untracked(|| model.total());
        model.name.read().len()
    });
    model.advance();
    assert!(!observer.is_dirty());
    model.rename("Boar Prime");
    assert!(observer.is_dirty());
}
