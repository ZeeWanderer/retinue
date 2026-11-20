use super::*;
use crate::ir::{Binding, Declaration, Property, Recipe, Source, Structure, Value};
use std::sync::Arc;

#[test]
fn transaction_storage_is_reused_but_rejected_payloads_are_not_retained() {
    let resource = Rc::new(());
    let template = Rc::new(
        Template::compile(
            Arc::new(
                Structure::compile(&[Declaration {
                    parent: None,
                    kind: Kind::Content,
                }])
                .unwrap(),
            ),
            vec![Recipe::default()],
            vec![Source {
                equal: |a: &(Rc<()>, f32), b| a.1 == b.1,
                bindings: Box::new([0, 1, 2]),
            }],
            vec![
                Binding {
                    node: Local(0),
                    property: Property::Paint,
                    evaluate: Rc::new(|input: &(Rc<()>, f32), key, revision| {
                        let resource = input.0.clone();
                        Value::Paint(Some(scene::Paint::new(
                            retinue_render::PaintKey::new(key, revision),
                            move |_| {
                                std::hint::black_box(&resource);
                                Vec::new()
                            },
                        )))
                    }),
                },
                Binding {
                    node: Local(0),
                    property: Property::Offset,
                    evaluate: Rc::new(|input: &(Rc<()>, f32), _, _| Value::Offset((input.1, 0.0))),
                },
                Binding {
                    node: Local(0),
                    property: Property::Input,
                    evaluate: Rc::new(|input: &(Rc<()>, f32), _, _| {
                        let resource = input.0.clone();
                        Value::Input {
                            enabled: true,
                            options: Some(crate::input::InputOptions::new(
                                crate::input::EventHandler::new(move |_, _| {
                                    std::hint::black_box(&resource);
                                }),
                            )),
                        }
                    }),
                },
            ],
        )
        .unwrap(),
    );
    let mut graph = InstanceGraph::default();
    let root = graph.mount(template, (resource.clone(), 0.0));
    graph.set_root(Some(root)).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    graph.stage(root, (resource.clone(), 1.0_f32)).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    let pointer = graph.changes.scene.as_ptr();
    let input_pointer = graph.changes.inputs.as_ptr();
    let capacity = graph.changes.scene.capacity();
    let references = Rc::strong_count(&resource);
    for offset in [2.0_f32, 3.0, 4.0] {
        graph.stage(root, (resource.clone(), offset)).unwrap();
        graph.advance((100, 100), 1.0, None).unwrap();
        assert!(graph.changes.scene.is_empty());
        assert!(graph.changes.inputs.is_empty());
        assert_eq!(graph.changes.scene.as_ptr(), pointer);
        assert_eq!(graph.changes.inputs.as_ptr(), input_pointer);
        assert_eq!(graph.changes.scene.capacity(), capacity);
        assert_eq!(Rc::strong_count(&resource), references);
    }
    graph.stage(root, (resource.clone(), f32::NAN)).unwrap();
    assert!(graph.advance((100, 100), 1.0, None).is_err());
    assert!(graph.changes.scene.is_empty());
    assert!(graph.changes.inputs.is_empty());
    assert_eq!(Rc::strong_count(&resource), references + 1);
    graph.discard();
    assert_eq!(Rc::strong_count(&resource), references);
    graph.set_root(None).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(graph.changes.scene.capacity(), 0);
    assert_eq!(Rc::strong_count(&resource), 1);
}
