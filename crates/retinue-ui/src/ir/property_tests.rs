use super::*;
use crate::{
    layout::values::{LayoutValueKey, LayoutValues},
    scene::Algorithm,
    semantics::{Role, WindowAction},
};
use retinue_render::CacheHint;
use std::{rc::Rc, sync::Arc};

struct Weight;
impl LayoutValueKey for Weight {
    type Value = u32;
    fn default_value() -> u32 {
        0
    }
}

fn properties() -> Rc<Template<bool>> {
    let structure = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Content,
        }])
        .unwrap(),
    );
    let bindings = [
        Property::LayoutValues,
        Property::Clip,
        Property::Interactive,
        Property::Role,
        Property::WindowAction,
        Property::Reuse,
    ]
    .into_iter()
    .map(|property| Binding {
        node: Local(0),
        property,
        evaluate: Rc::new(move |enabled: &bool, _, _| match property {
            Property::LayoutValues => {
                Value::LayoutValues(LayoutValues::default().with::<Weight>(u32::from(*enabled)))
            }
            Property::Clip => Value::Clip(*enabled),
            Property::Interactive => Value::Interactive(*enabled),
            Property::Role => Value::Role(if *enabled { Role::Title } else { Role::None }),
            Property::WindowAction => Value::WindowAction(enabled.then_some(WindowAction::Move)),
            Property::Reuse => Value::Reuse(if *enabled {
                CacheHint::Stable
            } else {
                CacheHint::Dynamic
            }),
            _ => unreachable!(),
        }),
    })
    .collect::<Vec<_>>();
    let source = Source {
        equal: |a, b| a == b,
        bindings: (0..bindings.len()).collect(),
    };
    Rc::new(
        Template::compile(
            structure,
            vec![Recipe {
                style: taffy::Style {
                    size: taffy::Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(50.0),
                    },
                    ..Default::default()
                },
                algorithm: Algorithm::Flex,
                paint: None,
            }],
            vec![source],
            bindings,
        )
        .unwrap()
        .with_attributes(vec![(
            Local(0),
            Attributes {
                name: Some("title".into()),
                offset: (5.0, 7.0),
                role: Role::Control,
                ..Default::default()
            },
        )])
        .unwrap(),
    )
}

#[test]
fn attributes_and_dynamic_properties_share_mount_and_update_semantics() {
    let program = properties();
    let mut graph = graph::InstanceGraph::default();
    let handle = graph.mount(program.clone(), false);
    graph.set_root(Some(handle)).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    let key = graph.key(handle, Local(0)).unwrap();
    let address = graph.address(key).unwrap();
    assert_eq!(
        address,
        graph::Address {
            instance: handle,
            local: Local(0)
        }
    );
    assert!(graph.address(crate::Key::new("other", key.value)).is_none());
    for enabled in [true, false, true] {
        graph.stage(handle, enabled).unwrap();
        let update = graph.advance((200, 100), 1.0, None).unwrap();
        assert_eq!(update.work.bindings_emitted, 6);
        let expected = program.mount(&enabled, 900_000, 0).unwrap();
        assert_eq!(expected.layout_key, Some("title".into()));
        assert_eq!(expected.layout_values.get::<Weight>(), u32::from(enabled));
        let actual = graph.geometry(address).unwrap();
        assert_eq!((actual.layout.x, actual.layout.y), (5.0, 7.0));
        assert_eq!(actual.role, expected.role);
        assert_eq!(actual.window_action, expected.window_action);
        assert_eq!(actual.interactive, expected.interactive);
        assert_eq!(graph.scene().frame().groups().len(), usize::from(enabled));
    }
    graph.set_root(None).unwrap();
    graph.advance((200, 100), 1.0, None).unwrap();
    assert!(graph.geometry(address).is_none());
    assert!(graph.address(key).is_none());
}

#[test]
fn static_attributes_validate_local_addresses() {
    let structure = Arc::new(
        Structure::compile(&[Declaration {
            parent: None,
            kind: Kind::Content,
        }])
        .unwrap(),
    );
    let make = || {
        Template::<()>::compile(structure.clone(), vec![Recipe::default()], vec![], vec![]).unwrap()
    };
    assert!(
        make()
            .with_attributes(vec![(Local(1), Attributes::default())])
            .is_err()
    );
    assert!(
        make()
            .with_attributes(vec![
                (Local(0), Attributes::default()),
                (Local(0), Attributes::default())
            ])
            .is_err()
    );
}
