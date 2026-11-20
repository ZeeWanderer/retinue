use super::{
    InstanceGraph, Local, Work,
    benchmark::bench,
    fixtures::{self, CycleInputs, CycleView},
};
use crate::{
    environment::Environment,
    runtime::ViewTree,
    scene::Scene,
    state::State,
    view::{IntoElement, VStack, View, ViewContext, ViewExt},
};
use retinue_render::{
    Color, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use serde_json::{Value, json};
use std::hint::black_box;

#[derive(PartialEq)]
struct Row(State<CycleInputs>);
impl View for Row {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        CycleView {
            input: self.0.get(),
        }
        .body(context)
        .into_element()
    }
}

#[derive(PartialEq)]
struct List(Vec<State<CycleInputs>>);
impl View for List {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        VStack::new(
            4.0,
            self.0
                .iter()
                .enumerate()
                .map(|(id, source)| Row(source.clone()).id(id as u64))
                .collect::<Vec<_>>(),
        )
    }
}

enum Backend {
    Graph(InstanceGraph, Work),
    Views(ViewTree),
}

#[derive(Clone, Copy)]
enum Mode {
    Views,
    Graph,
    Context,
    Compiled,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Graph => "graph",
            Self::Context => "context",
            Self::Compiled => "compiled",
        }
    }
}

impl Backend {
    fn scene(&self) -> &Scene {
        match self {
            Self::Graph(graph, _) => graph.scene(),
            Self::Views(tree) => tree.scene(),
        }
    }

    fn update(&mut self, size: (u32, u32), scale: f32) -> Damage {
        match self {
            Self::Graph(graph, work) => {
                let update = graph.advance(size, scale, None).unwrap();
                *work = update.work;
                let std::task::Poll::Ready(damage) = update.completion.unwrap() else {
                    panic!("inline layout queued")
                };
                damage
            }
            Self::Views(tree) => tree.update(size, scale).unwrap(),
        }
    }

    fn resources(&self) -> Value {
        let (owners, sources, bindings) = match self {
            Self::Graph(_, work) => (
                work.owners_evaluated,
                work.sources_compared,
                work.bindings_emitted,
            ),
            Self::Views(tree) => (tree.stats.bodies, 0, 0),
        };
        json!({"owners_evaluated": owners, "sources_compared": sources,
            "bindings_emitted": bindings, "scene_nodes": self.scene().geometry().len(),
            "draw_instances": self.scene().frame().instances().len()})
    }
}

struct Run {
    backend: Backend,
    source: State<CycleInputs>,
    choices: [CycleInputs; 2],
    pixels: Vec<u8>,
    renderer: Option<Renderer>,
}

impl Run {
    fn new(mode: Mode, changed: bool, painting: bool, size: (u32, u32), scale: f32) -> Self {
        let models: Vec<_> = fixtures::list_items(32)
            .into_iter()
            .map(|(_, input)| State::new(input))
            .collect();
        let a = models[2].get();
        let mut b = a.clone();
        if changed {
            b.timer = "00:09:59".into();
        }
        let source = models[2].clone();
        let mut backend = if !matches!(mode, Mode::Views) {
            let mut graph = InstanceGraph::default();
            let root = graph.mount(fixtures::list(), ());
            graph.set_root(Some(root)).unwrap();
            let program = fixtures::cycle();
            let children = models
                .into_iter()
                .enumerate()
                .map(|(key, model)| {
                    if matches!(mode, Mode::Compiled) {
                        let instance = graph
                            .mount_view(fixtures::CompiledCycle { model }, Environment::default())
                            .unwrap();
                        return ((key as u64).into(), instance);
                    }
                    let instance = graph.mount(program.clone(), model.get());
                    match mode {
                        Mode::Context => graph
                            .observe_in(instance, Environment::default(), move |_| model.get())
                            .unwrap(),
                        _ => graph.observe(instance, move || model.get()).unwrap(),
                    }
                    ((key as u64).into(), instance)
                })
                .collect();
            graph.children(root, Local(0), children).unwrap();
            Backend::Graph(graph, Work::default())
        } else {
            Backend::Views(ViewTree::new(List(models), || {}))
        };
        backend.update(size, scale);
        let mut run = Self {
            backend,
            source,
            choices: [a, b],
            pixels: if painting {
                vec![0; size.0 as usize * size.1 as usize * 4]
            } else {
                Vec::new()
            },
            renderer: painting.then(|| Renderer::new(ResidencyPolicy::desktop(0))),
        };
        run.paint(Damage::Full);
        run
    }

    fn step(&mut self, index: usize, size: (u32, u32), scale: f32) {
        self.source.set(self.choices[index % 2].clone());
        let damage = self.backend.update(size, scale);
        self.paint(damage);
        black_box(self.backend.scene().frame());
    }

    fn paint(&mut self, damage: Damage) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let scene = self.backend.scene();
        let size = scene.frame().size();
        let Some(bounds) = damage.bounds(size) else {
            return;
        };
        renderer.prepare(scene.frame(), RasterMode::Replay);
        let mut painter =
            Painter::with_threads(&mut self.pixels, size.0, size.1, bench::render_threads())
                .unwrap();
        painter.repair(&damage.regions(size));
        renderer.paint(scene.frame(), &mut painter, bounds, Color::rgb(0x202020));
        painter.finish().unwrap();
    }
}

#[test]
fn observed_inputs_match_retained_geometry_and_pixels() {
    fn check(graph: &Run, views: &Run, scale: f32, step: usize) {
        let geometry = |run: &Run| {
            let mut bounds = run
                .backend
                .scene()
                .geometry()
                .iter()
                .map(|node| node.layout)
                .collect::<Vec<_>>();
            // Retained modifier wrappers repeat their child's rectangle.
            bounds.dedup();
            bounds
        };
        let (actual, expected) = (geometry(graph), geometry(views));
        assert_eq!(actual.len(), expected.len(), "scale {scale}, step {step}");
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(
                actual, expected,
                "scale {scale}, step {step}, rectangle {index}"
            );
        }
        assert_eq!(graph.pixels.len(), views.pixels.len());
        let mismatch = graph
            .pixels
            .iter()
            .zip(&views.pixels)
            .position(|(a, b)| a != b);
        assert_eq!(
            mismatch, None,
            "scale {scale}, step {step}, first differing byte"
        );
    }
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (720.0 * scale) as u32);
        for mode in [Mode::Graph, Mode::Context, Mode::Compiled] {
            let mut graph = Run::new(mode, true, true, size, scale);
            let mut views = Run::new(Mode::Views, true, true, size, scale);
            check(&graph, &views, scale, 0);
            for index in 1..5 {
                graph.step(index, size, scale);
                views.step(index, size, scale);
                check(&graph, &views, scale, index);
                let Backend::Graph(_, work) = &graph.backend else {
                    unreachable!()
                };
                assert_eq!(work.owners_evaluated, 1);
                assert_eq!(work.bindings_emitted, 1);
                assert_eq!(work.topology_visited, 0);
            }
        }
    }
}

#[test]
#[ignore = "release timing experiment"]
fn observed_input_benchmark() {
    let mut cases = Vec::new();
    let run: usize = std::env::var("RETINUE_BENCH_RUN_INDEX")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (720.0 * scale) as u32);
        for changed in [false, true] {
            for mode in if run.is_multiple_of(2) {
                [Mode::Views, Mode::Graph, Mode::Context, Mode::Compiled]
            } else {
                [Mode::Compiled, Mode::Context, Mode::Graph, Mode::Views]
            } {
                for painting in [false, true] {
                    let name = format!(
                        "{}-{}-{}",
                        if changed { "timer" } else { "noop" },
                        mode.name(),
                        if painting { "frame" } else { "update" }
                    );
                    if !bench::selected(&name) {
                        continue;
                    }
                    let mut case = bench::measure(
                        &name,
                        size,
                        scale,
                        || Run::new(mode, changed, painting, size, scale),
                        |run, index| run.step(index, size, scale),
                        |run| run.backend.resources(),
                    );
                    case["actual_threads"] = json!(if painting {
                        bench::render_threads().get()
                    } else {
                        1
                    });
                    cases.push(case);
                }
            }
        }
    }
    bench::report_with_scope(
        "view-ir-observed",
        cases,
        "32 retained rows; one shared State changes, without restaging the collection. Graph uses a handwritten template and plain reader; Context adds ViewContext without active ticks; Compiled uses typed authoring and shared generated templates; Views uses the existing Element runtime. Update includes model mutation, body/value evaluation, inline layout and recording; frame adds repair/native completion. No topology changes, dynamic branches, active timeline/motion, event routing, compositor or application migration. Warm fonts, optional rasters disabled.",
    );
}

#[test]
#[ignore = "workload for external allocation profilers, without timing queries"]
fn allocation_profile() {
    let backend = std::env::var("RETINUE_ALLOC_BACKEND").unwrap_or_else(|_| "graph".into());
    let mode = match backend.as_str() {
        "graph" => Mode::Graph,
        "context" => Mode::Context,
        "compiled" => Mode::Compiled,
        "views" => Mode::Views,
        _ => panic!("expected graph, context, compiled or views"),
    };
    let case = std::env::var("RETINUE_ALLOC_CASE").unwrap_or_else(|_| "timer".into());
    assert!(matches!(case.as_str(), "noop" | "timer"));
    let samples = std::env::var("RETINUE_ALLOC_SAMPLES")
        .unwrap_or_else(|_| "400".into())
        .parse::<usize>()
        .unwrap();
    assert!((1..=5000).contains(&samples));
    let size = (516, 864);
    let mut run = Run::new(mode, case == "timer", false, size, 1.2);
    for index in 0..samples + 6 {
        run.step(index, size, 1.2);
    }
}
