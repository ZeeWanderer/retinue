use ui_alias::{observable, state::Observation};

#[observable]
#[derive(Debug, Default)]
struct Model {
    count: u32,
    #[observation(ignore)]
    requests: u32,
    #[cfg(any())]
    absent: DoesNotExist,
}

#[test]
fn finds_the_runtime_through_its_cargo_dependency_alias() {
    let model = Model::new(1, 0);
    let mut observation = Observation::new(|| {});
    observation.track(|| model.count.get());
    model.requests.set(1);
    assert!(!observation.is_dirty());
    model.count.set(2);
    assert!(observation.is_dirty());
    assert_eq!(Model::default().count.get(), 0);
    assert!(format!("{model:?}").contains("count"));
}

#[ui_alias::observable(crate = ui_alias)]
struct Explicit {
    r#type: u32,
}

#[test]
fn supports_explicit_runtime_paths_and_raw_field_names() {
    assert_eq!(Explicit::new(5).r#type.get(), 5);
}

#[observable]
#[cfg(any())]
struct Removed {
    value: DoesNotExist,
}

#[derive(ui_alias::ViewProperties)]
struct Properties {
    #[owned(Model::new(3, 0))]
    model: Model,
    #[cfg(any())]
    absent: DoesNotExist,
}

#[test]
fn properties_find_the_renamed_runtime_and_respect_disabled_fields() {
    use ui_alias::prelude::*;
    #[derive(PartialEq)]
    struct Root;
    impl View for Root {
        fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
            let properties = context.properties::<Properties>();
            assert_eq!(properties.model.count.get(), 3);
            empty()
        }
    }
    ui_alias::runtime::ViewTree::new(Root, || {})
        .update((80, 60), 1.0)
        .unwrap();
}
