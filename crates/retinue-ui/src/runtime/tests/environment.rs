use super::*;

struct Accent;
impl EnvironmentKey for Accent {
    type Value = u32;
    fn default_value() -> u32 {
        0x123456
    }
}

#[derive(Clone, PartialEq)]
struct Reader {
    values: Rc<RefCell<Vec<u32>>>,
}
impl View for Reader {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let color = context.environment().get::<Accent>();
        self.values.borrow_mut().push(color);
        tile(color)
    }
}

#[derive(Clone, PartialEq)]
struct Providers {
    local: State<u32>,
    a: Reader,
    b: Reader,
}
impl View for Providers {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        HStack::new(
            4.0,
            (
                self.a.clone().environment::<Accent>(self.local.get()),
                self.b.clone(),
            ),
        )
    }
}

#[test]
fn inherited_providers_update_only_consumers_and_local_overrides_isolate_siblings() {
    let a = Reader {
        values: Rc::default(),
    };
    let b = Reader {
        values: Rc::default(),
    };
    let providers = Providers {
        local: State::new(1),
        a: a.clone(),
        b: b.clone(),
    };
    let mut tree = ViewTree::new(providers.clone(), || {});
    tree.environment().set::<Accent>(2);
    update(&mut tree);
    assert_eq!(&*a.values.borrow(), &[1]);
    assert_eq!(&*b.values.borrow(), &[2]);
    tree.environment().set::<Accent>(3);
    update(&mut tree);
    assert_eq!(&*a.values.borrow(), &[1]);
    assert_eq!(&*b.values.borrow(), &[2, 3]);
    assert_eq!(tree.stats.bodies, 1);
    let animation = Animation::new(Duration::from_millis(100));
    with_animation(animation, || {
        providers.local.set(4);
    });
    update(&mut tree);
    assert_eq!(&*a.values.borrow(), &[1, 4]);
    assert_eq!(&*b.values.borrow(), &[2, 3]);
    assert_eq!(tree.stats.bodies, 2);
    let reader = tree.nodes.values().find(|node| {
        let node = node.borrow();
        matches!(&*node.input.kind, crate::view::Kind::View(view) if view.as_any().downcast_ref::<Reader>().is_some_and(|reader| Rc::ptr_eq(&reader.values, &a.values)))
    }).unwrap().borrow();
    assert_eq!(
        reader.last_mutation.unwrap().transaction.animation,
        Some(animation)
    );
}

#[derive(Clone, PartialEq)]
struct Model {
    number: State<u32>,
}

#[derive(Clone, PartialEq)]
struct ModelReader;
impl View for ModelReader {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        tile(
            context
                .environment()
                .require::<Model>()
                .unwrap()
                .number
                .get(),
        )
    }
}

#[test]
fn supplied_environment_model_uses_the_same_observation_lifecycle() {
    let model = Model {
        number: State::new(2),
    };
    let mut tree = ViewTree::new(ModelReader.environment_model(model.clone()), || {});
    update(&mut tree);
    model.number.set(3);
    update(&mut tree);
    assert_eq!(tree.stats.bodies, 1);
    assert!(!tree.needs_update());
}
