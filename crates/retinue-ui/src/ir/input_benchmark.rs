use super::{InstanceGraph, Local, Recipe, Template, benchmark::bench, fixtures};
use crate::{
    input::{EventHandler, InputOptions},
    runtime::ViewTree,
    runtime::reference::prelude::*,
    scene::{Paint, Scene},
};
use retinue_render::{Command, PaintKey};
use serde_json::json;
use std::{hint::black_box, rc::Rc, sync::Arc};

fn options(value: u32) -> InputOptions {
    InputOptions::new(EventHandler::with(value, |value, _, _| {
        black_box(value);
    }))
}

fn template() -> Rc<Template<u32>> {
    Rc::new(
        Template::compile(
            Arc::new(
                super::Structure::compile(&[super::Declaration {
                    parent: None,
                    kind: super::Kind::Content,
                }])
                .unwrap(),
            ),
            vec![Recipe {
                style: taffy::Style {
                    size: taffy::Size {
                        width: taffy::Dimension::length(40.0),
                        height: taffy::Dimension::length(24.0),
                    },
                    ..Default::default()
                },
                paint: Some(Paint::new(
                    PaintKey::new(crate::Key::new("input.bench", 0), 0),
                    |bounds| {
                        vec![Command::Fill {
                            bounds: crate::Bounds::new(0.0, 0.0, bounds.width, bounds.height),
                            radius: 0.0,
                            color: Color::rgb(0x222222),
                        }]
                    },
                )),
                ..Default::default()
            }],
            vec![super::Source {
                equal: PartialEq::eq,
                bindings: Box::new([0]),
            }],
            vec![super::Binding {
                node: Local(0),
                property: super::Property::Input,
                evaluate: Rc::new(|value, _, _| super::Value::Input {
                    enabled: true,
                    options: Some(options(*value)),
                }),
            }],
        )
        .unwrap()
        .with_input_equality(PartialEq::eq),
    )
}

#[derive(PartialEq)]
struct Row {
    model: State<u32>,
}

impl View for Row {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Rectangle::new(Color::rgb(0x222222))
            .frame(40.0, 24.0)
            .input(options(self.model.get()))
    }
}

#[derive(PartialEq)]
struct List {
    models: Vec<State<u32>>,
}

impl View for List {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        VStack::new(
            4.0,
            self.models
                .iter()
                .enumerate()
                .map(|(id, model)| {
                    Row {
                        model: model.clone(),
                    }
                    .id(id as u64)
                })
                .collect::<Vec<_>>(),
        )
    }
}

enum Backend {
    Graph(InstanceGraph),
    Views(ViewTree),
}

struct Run {
    backend: Backend,
    model: State<u32>,
    changed: bool,
}

impl Run {
    fn new(graph: bool, changed: bool, count: usize) -> Self {
        let models: Vec<_> = (0..count).map(|_| State::new(0)).collect();
        let model = models[2].clone();
        let backend = if graph {
            let mut graph = InstanceGraph::default();
            let root = graph.mount(fixtures::list(), ());
            graph.set_root(Some(root)).unwrap();
            let template = template();
            let children = models
                .into_iter()
                .enumerate()
                .map(|(key, model)| {
                    let handle = graph.mount(template.clone(), model.get());
                    graph.observe(handle, move || model.get()).unwrap();
                    ((key as u64).into(), handle)
                })
                .collect();
            graph.children(root, Local(0), children).unwrap();
            Backend::Graph(graph)
        } else {
            Backend::Views(ViewTree::new(List { models }, || {}))
        };
        let mut run = Self {
            backend,
            model,
            changed,
        };
        run.step(0);
        run
    }

    fn step(&mut self, index: usize) {
        self.model
            .set(if self.changed { index as u32 % 2 } else { 0 });
        match &mut self.backend {
            Backend::Graph(graph) => {
                black_box(graph.advance((300, 600), 1.0, None).unwrap());
            }
            Backend::Views(tree) => {
                black_box(tree.update((300, 600), 1.0).unwrap());
            }
        }
    }

    fn scene(&self) -> &Scene {
        match &self.backend {
            Backend::Graph(graph) => graph.scene(),
            Backend::Views(tree) => tree.scene(),
        }
    }

    fn targets(&self) -> &[crate::input::Target] {
        match &self.backend {
            Backend::Graph(graph) => graph.input().targets(),
            Backend::Views(tree) => tree.input().targets(),
        }
    }
}

#[test]
fn published_handler_fixture_matches_geometry_without_repainting() {
    let mut graph = Run::new(true, true, 32);
    let mut views = Run::new(false, true, 32);
    let before: Vec<_> = graph
        .scene()
        .frame()
        .instances()
        .iter()
        .map(|i| i.program.id())
        .collect();
    for index in 1..4 {
        graph.step(index);
        views.step(index);
        assert_eq!(graph.targets().len(), 32);
        assert_eq!(graph.targets().len(), views.targets().len());
        for (a, b) in graph.targets().iter().zip(views.targets()) {
            assert_eq!(a.bounds, b.bounds);
            assert_eq!(a.clip, b.clip);
            assert_eq!(a.enabled, b.enabled);
            assert!(a.input == b.input);
        }
        assert_eq!(
            before,
            graph
                .scene()
                .frame()
                .instances()
                .iter()
                .map(|i| i.program.id())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
#[ignore = "release input-publication workload"]
fn input_publication_benchmark() {
    let mut cases = Vec::new();
    for count in [32, 512] {
        for changed in [false, true] {
            for graph in [false, true] {
                let name = format!(
                    "{}-{}-{count}",
                    if changed { "handler" } else { "noop" },
                    if graph { "graph" } else { "views" }
                );
                if !bench::selected(&name) {
                    continue;
                }
                let mut case = bench::measure(
                    &name,
                    (300, 600),
                    1.0,
                    || Run::new(graph, changed, count),
                    Run::step,
                    |run| json!({"input_targets": run.targets().len()}),
                );
                case["actual_threads"] = json!(1);
                case["items"] = json!(count);
                cases.push(case);
            }
        }
    }
    bench::report_with_scope(
        "view-ir-input",
        cases,
        "One observed handler payload changes among 32/512 mounted controls, or stays unchanged. Update includes observation, typed diff and input publication. No geometry change, native painting, hit-test event dispatch or application migration.",
    );
}

#[test]
#[ignore = "external allocation attribution without timing queries"]
fn allocation_profile() {
    let graph =
        std::env::var("RETINUE_ALLOC_BACKEND").unwrap_or_else(|_| "graph".into()) == "graph";
    let changed =
        std::env::var("RETINUE_ALLOC_CASE").unwrap_or_else(|_| "handler".into()) == "handler";
    let samples: usize = std::env::var("RETINUE_ALLOC_SAMPLES")
        .unwrap_or_else(|_| "400".into())
        .parse()
        .unwrap();
    assert!((1..=5000).contains(&samples));
    let mut run = Run::new(graph, changed, 32);
    for index in 0..samples + 6 {
        run.step(index);
    }
}
