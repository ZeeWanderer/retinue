use super::{
    benchmark::bench,
    fixtures::{self, CycleInputs, CycleList},
    graph::InstanceGraph,
    store::Handle,
    *,
};
use crate::{runtime::ViewTree, scene::Scene};
use retinue_render::{
    Color, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use serde_json::{Value as Json, json};
use std::{hint::black_box, rc::Rc, task::Poll};

enum Backend {
    Views(ViewTree),
    InstanceGraph {
        graph: InstanceGraph,
        root: Handle,
        program: Rc<Template<CycleInputs>>,
        work: Work,
    },
}

impl Backend {
    fn scene(&self) -> &Scene {
        match self {
            Self::Views(tree) => tree.scene(),
            Self::InstanceGraph { graph, .. } => graph.scene(),
        }
    }

    fn update(&mut self, items: Vec<(u64, CycleInputs)>, size: (u32, u32), scale: f32) -> Damage {
        match self {
            Self::Views(tree) => {
                tree.set_root(CycleList { items });
                tree.update(size, scale).unwrap()
            }
            Self::InstanceGraph {
                graph,
                root,
                program,
                work,
            } => {
                graph
                    .keyed(
                        *root,
                        Local(0),
                        items
                            .into_iter()
                            .map(|(id, input)| (id.into(), program.clone(), input))
                            .collect(),
                    )
                    .unwrap();
                let update = graph.advance(size, scale, None).unwrap();
                *work = update.work;
                let Poll::Ready(damage) = update.completion.unwrap() else {
                    panic!("inline layout queued")
                };
                damage
            }
        }
    }

    fn resources(&self) -> Json {
        let updated = !matches!(self, Self::InstanceGraph { work, .. } if work.instances_changed == 0 && work.nodes_materialized == 0);
        let mut result = json!({
            "scene_nodes": self.scene().geometry().len(),
            "draw_instances": self.scene().frame().instances().len(),
            "layout_computations": if updated { self.scene().stats.layout_computations } else { 0 },
            "recordings": if updated { self.scene().stats.recordings } else { 0 },
        });
        match self {
            Self::Views(tree) => {
                result["semantic_instances"] = json!(tree.node_count());
                result["bodies"] = json!(tree.stats.bodies);
                result["projected"] = json!(tree.stats.projected);
            }
            Self::InstanceGraph { graph, work, .. } => {
                result["semantic_instances"] = json!(graph.len());
                result["sources_compared"] = json!(work.sources_compared);
                result["bindings_emitted"] = json!(work.bindings_emitted);
                result["nodes_materialized"] = json!(work.nodes_materialized);
                result["topology_visited"] = json!(work.topology_visited);
            }
        }
        result
    }
}

struct Run {
    backend: Backend,
    renderer: Option<Renderer>,
    pixels: Vec<u8>,
    choices: [Vec<(u64, CycleInputs)>; 2],
}

impl Run {
    fn new(direct: bool, change: &str, size: (u32, u32), scale: f32, painting: bool) -> Self {
        let a = fixtures::list_items(32);
        let mut b = a.clone();
        match change {
            "noop" => {}
            "timer" => b[2].1.timer = "00:09:59".into(),
            "reorder" => b.rotate_left(3),
            "replace" => {
                b[2].0 = 42;
                b[2].1.name = "NEW CYCLE".into();
            }
            "filter-cycle" => b.retain(|(id, _)| id.is_multiple_of(2)),
            _ => panic!("unknown collection change"),
        }
        let backend = if direct {
            let mut graph = InstanceGraph::default();
            let root = graph.mount(fixtures::list(), ());
            graph.set_root(Some(root)).unwrap();
            Backend::InstanceGraph {
                graph,
                root,
                program: fixtures::cycle(),
                work: Work::default(),
            }
        } else {
            Backend::Views(ViewTree::new(CycleList { items: a.clone() }, || {}))
        };
        let mut run = Self {
            backend,
            renderer: painting.then(|| Renderer::new(ResidencyPolicy::desktop(0))),
            pixels: if painting {
                vec![0; size.0 as usize * size.1 as usize * 4]
            } else {
                Vec::new()
            },
            choices: [a, b],
        };
        run.backend.update(run.choices[0].clone(), size, scale);
        run.paint(Damage::Full);
        run
    }

    fn update(&mut self, change: &str, index: usize, size: (u32, u32), scale: f32) {
        if change == "filter-cycle" {
            for choice in [1, 0] {
                let damage = self
                    .backend
                    .update(self.choices[choice].clone(), size, scale);
                self.paint(damage);
            }
        } else {
            let damage = self
                .backend
                .update(self.choices[index % 2].clone(), size, scale);
            self.paint(damage);
        }
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

pub(super) fn cases(run_index: usize) -> Vec<Json> {
    let mut cases = Vec::new();
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (720.0 * scale) as u32);
        for change in ["noop", "timer", "reorder", "replace", "filter-cycle"] {
            for direct in if run_index.is_multiple_of(2) {
                [false, true]
            } else {
                [true, false]
            } {
                for painting in [false, true] {
                    let name = format!(
                        "list-{change}-{}-{}",
                        if direct { "graph" } else { "views" },
                        if painting { "frame" } else { "update" }
                    );
                    if !bench::selected(&name) {
                        continue;
                    }
                    let mut case = bench::measure(
                        &name,
                        size,
                        scale,
                        || Run::new(direct, change, size, scale, painting),
                        |run, index| run.update(change, index, size, scale),
                        |run| {
                            let mut resources = run.backend.resources();
                            resources["framebuffer_bytes"] = json!(run.pixels.len());
                            resources
                        },
                    );
                    case["items"] = json!(32);
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
    cases
}

#[test]
#[ignore = "workload for external allocation profilers, without timing queries"]
fn allocation_profile() {
    let change = std::env::var("RETINUE_ALLOC_CASE").unwrap_or_else(|_| "timer".into());
    let direct = match std::env::var("RETINUE_ALLOC_BACKEND")
        .as_deref()
        .unwrap_or("graph")
    {
        "graph" => true,
        "views" => false,
        _ => panic!("RETINUE_ALLOC_BACKEND must be graph or views"),
    };
    let count: usize = std::env::var("RETINUE_ALLOC_SAMPLES")
        .unwrap_or_else(|_| "400".into())
        .parse()
        .unwrap();
    assert!((1..=5000).contains(&count));
    let size = (516, 864);
    let scale = 1.2;
    let mut run = Run::new(direct, &change, size, scale, false);
    for index in 0..count + 6 {
        run.update(&change, index, size, scale);
    }
}
