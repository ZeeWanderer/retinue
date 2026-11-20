use super::{fixtures, *};
use retinue_render::damage::Damage;
use std::{cell::RefCell, rc::Rc, sync::Arc, task::Poll};

fn offset() -> Rc<Template<f32>> {
    Rc::new(
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
                equal: PartialEq::eq,
                bindings: Box::new([0]),
            }],
            vec![Binding {
                node: Local(0),
                property: Property::Offset,
                evaluate: Rc::new(|input: &f32, _, _| Value::Offset((*input, 0.0))),
            }],
        )
        .unwrap()
        .with_input_equality(PartialEq::eq),
    )
}

#[test]
fn equal_inputs_are_not_staged_but_a_pending_change_is_preserved() {
    let mut bank = Bank::default();
    let id = bank.mount(offset(), 0.0);
    assert_eq!(bank.stage(id, 0.0), Ok(false));
    assert_eq!(bank.submit(|_| panic!()).unwrap(), Work::default());

    assert_eq!(bank.stage(id, 2.0), Ok(true));
    assert_eq!(bank.stage(id, 2.0), Ok(false));
    assert_eq!(bank.submit(|_| Err("busy")), Err("busy"));
    assert_eq!(bank.stage(id, 2.0), Ok(false));
    assert_eq!(bank.submit(|_| Ok(())).unwrap().bindings_emitted, 1);
    assert_eq!(bank.element(id).unwrap().offset, (2.0, 0.0));
    assert_eq!(bank.stage(id, 2.0), Ok(false));
    assert_eq!(bank.submit(|_| panic!()).unwrap(), Work::default());
}

#[test]
fn restaging_the_accepted_value_cancels_a_pending_change() {
    let mut bank = Bank::default();
    let id = bank.mount(offset(), 0.0);
    assert_eq!(bank.stage(id, 2.0), Ok(true));
    assert_eq!(bank.stage(id, 0.0), Ok(true));
    assert_eq!(bank.stage(id, 0.0), Ok(false));
    let work = bank.submit(|_| panic!("coalesced no-op painted")).unwrap();
    assert_eq!(work.instances_changed, 0);
    assert_eq!(bank.revision(id), Ok(0));
    assert_eq!(bank.element(id).unwrap().offset, (0.0, 0.0));
}

#[test]
fn unbound_input_fields_are_accepted_without_painting() {
    struct Input {
        version: u32,
        drops: Rc<RefCell<Vec<u32>>>,
    }
    impl Drop for Input {
        fn drop(&mut self) {
            self.drops.borrow_mut().push(self.version);
        }
    }
    let drops = Rc::new(RefCell::new(Vec::new()));
    let input = |version| Input {
        version,
        drops: drops.clone(),
    };
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
            vec![],
            vec![],
        )
        .unwrap()
        .with_input_equality(|a: &Input, b| a.version == b.version),
    );
    let mut bank = Bank::default();
    let id = bank.mount(template, input(1));
    assert_eq!(bank.stage(id, input(2)), Ok(true));
    assert_eq!(bank.submit(|_| panic!()).unwrap(), Work::default());
    assert_eq!(&*drops.borrow(), &[1]);
    assert_eq!(bank.stage(id, input(2)), Ok(false));
    assert_eq!(&*drops.borrow(), &[1, 2]);
    bank.remove(id).unwrap();
    assert_eq!(&*drops.borrow(), &[1, 2, 2]);
}

#[test]
fn stable_collection_only_diffs_changed_inputs() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(fixtures::list(), ());
    graph.set_root(Some(root)).unwrap();
    let template = fixtures::cycle();
    let items = fixtures::list_items(32);
    let staged = |items: &[(u64, fixtures::CycleInputs)]| {
        items
            .iter()
            .map(|(key, input)| ((*key).into(), template.clone(), input.clone()))
            .collect()
    };
    let order: Vec<_> = graph
        .keyed(root, Local(0), staged(&items))
        .unwrap()
        .iter()
        .map(|child| child.handle)
        .collect();
    let before = graph.advance((430, 720), 1.2, None).unwrap();
    assert!(
        graph
            .keyed(root, Local(0), staged(&items))
            .unwrap()
            .iter()
            .map(|child| child.handle)
            .eq(order)
    );
    let noop = graph.advance((430, 720), 1.2, None).unwrap();
    assert_eq!(noop.work, Work::default());
    assert_eq!(noop.published, before.published);
    assert_eq!(noop.completion, Ok(Poll::Ready(Damage::None)));

    let mut changed = items.clone();
    changed[2].1.timer = "00:09:59".into();
    graph.keyed(root, Local(0), staged(&changed)).unwrap();
    graph.keyed(root, Local(0), staged(&changed)).unwrap();
    let result = graph.advance((430, 720), 1.2, None).unwrap();
    assert_eq!(result.work.sources_compared, 8);
    assert_eq!(result.work.bindings_emitted, 1);
    assert_eq!(result.work.instances_changed, 1);
    assert_eq!(result.work.nodes_materialized, 0);
    assert_eq!(result.work.topology_visited, 0);
}

#[test]
fn input_guard_does_not_cancel_a_rejected_update_or_bypass_stale_handles() {
    let mut graph = InstanceGraph::default();
    let root = graph.mount(offset(), 0.0);
    graph.set_root(Some(root)).unwrap();
    let before = graph.advance((100, 100), 1.0, None).unwrap();
    graph.stage(root, f32::INFINITY).unwrap();
    assert!(graph.advance((100, 100), 1.0, None).is_err());
    graph.stage(root, f32::INFINITY).unwrap();
    assert!(graph.advance((100, 100), 1.0, None).is_err());
    graph.discard();
    graph.stage(root, 0.0_f32).unwrap();
    let noop = graph.advance((100, 100), 1.0, None).unwrap();
    assert_eq!(noop.work, Work::default());
    assert_eq!(noop.published, before.published);
    graph.set_root(None).unwrap();
    graph.advance((100, 100), 1.0, None).unwrap();
    assert!(graph.stage(root, 0.0_f32).is_err());
}
