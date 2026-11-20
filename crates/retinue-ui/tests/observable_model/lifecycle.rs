use super::super::{Equipment, equipment};
use retinue_render::Color;
use retinue_ui::{
    authoring::*,
    ir::InstanceGraph,
    state::{Observation, State, untracked},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, PartialEq)]
struct OwnedView {
    trigger: State<bool>,
    initializations: Rc<Cell<usize>>,
    published: Rc<RefCell<Vec<Equipment>>>,
}

#[view]
impl View for OwnedView {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
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
        Rectangle(color).frame(Frame::new().width(model.total() as f32 * 10.0).height(20.0))
    }
}

#[test]
fn model_created_in_a_view_survives_body_updates_and_resets_with_identity() {
    let view = OwnedView {
        trigger: State::new(false),
        initializations: Rc::default(),
        published: Rc::default(),
    };
    let identity = State::new(1_u64);
    let key = identity.clone();
    let child = view.clone();
    let mut tree = InstanceGraph::with_root_body(
        move |_| ForEach([key.get()], |id| *id, |_| compose(child.clone())),
        Default::default(),
    )
    .unwrap();
    tree.update_inline((200, 100), 1.0).unwrap();
    let model = view.published.borrow()[0].clone();
    model.rank.set(3);
    tree.update_inline((200, 100), 1.0).unwrap();
    view.trigger.set(true);
    tree.update_inline((200, 100), 1.0).unwrap();
    assert_eq!(view.initializations.get(), 1);
    assert!(view.published.borrow().iter().all(|seen| seen == &model));
    identity.set(2);
    tree.update_inline((200, 100), 1.0).unwrap();
    assert_eq!(view.initializations.get(), 2);
    assert_ne!(view.published.borrow().last().unwrap(), &model);
    model.rank.set(9);
    assert!(!tree.has_pending_inputs());
}

#[derive(Clone, PartialEq)]
struct EnvironmentReader {
    readings: Rc<RefCell<Vec<u32>>>,
}

#[view]
impl View for EnvironmentReader {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let item = context.environment().require::<Equipment>().unwrap();
        let total = item.total();
        self.readings.borrow_mut().push(total);
        Rectangle(Color::rgb(0xaabbcc)).frame(Frame::new().width(total as f32).height(20.0))
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
    let mut tree = InstanceGraph::with_root_body({
        let (left, right, a, b) = (left.clone(), right.clone(), a.clone(), b.clone());
        move |_| ui! {
            HStack(2.0) { compose(left.clone()) compose(right.clone()).environment_model(b.clone()) }
                .environment_model(a.clone())
        }
    }, Default::default()).unwrap();
    tree.update_inline((200, 100), 1.0).unwrap();
    a.rank.set(4);
    tree.update_inline((200, 100), 1.0).unwrap();
    assert_eq!(&*left.readings.borrow(), &[3, 6]);
    assert_eq!(&*right.readings.borrow(), &[3]);
    a.rename("Boar Prime");
    assert!(!tree.has_pending_inputs());
    b.rank.set(6);
    tree.update_inline((200, 100), 1.0).unwrap();
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
