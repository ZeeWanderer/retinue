use super::{
    Bank, Work,
    fixtures::{self, CycleInputs, CycleView},
    graph::InstanceGraph,
    store::Handle,
};
use crate::{
    runtime::ViewTree,
    scene::{RecordingMode, Scene},
};
use retinue_render::{
    Color, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use serde_json::{Value, json};
use std::{hint::black_box, task::Poll};

#[allow(dead_code)]
#[path = "../../../../tools/benchmarks/support.rs"]
pub(super) mod bench;

enum Backend {
    Templates {
        bank: Bank<CycleInputs>,
        handle: Handle,
        scene: Scene,
        work: Work,
    },
    Views(ViewTree),
    InstanceGraph {
        graph: InstanceGraph,
        handle: Handle,
        work: Work,
    },
}

#[derive(Clone, Copy)]
enum Mode {
    Views,
    Templates,
    InstanceGraph,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Templates => "template",
            Self::InstanceGraph => "graph",
        }
    }
}

impl Backend {
    fn new(mode: Mode, size: (u32, u32), scale: f32) -> Self {
        let input = CycleInputs::default();
        if matches!(mode, Mode::Templates) {
            let mut bank = Bank::default();
            let handle = bank.mount(fixtures::cycle(), input);
            let mut scene = Scene::default();
            scene
                .update(
                    bank.element(handle).unwrap(),
                    size,
                    scale,
                    RecordingMode::Retained,
                )
                .unwrap();
            Self::Templates {
                bank,
                handle,
                scene,
                work: Work::default(),
            }
        } else if matches!(mode, Mode::Views) {
            let mut tree = ViewTree::new(CycleView { input }, || {});
            tree.update(size, scale).unwrap();
            Self::Views(tree)
        } else {
            let mut graph = InstanceGraph::default();
            let handle = graph.mount(fixtures::cycle(), input);
            graph.set_root(Some(handle)).unwrap();
            assert!(
                graph
                    .advance(size, scale, None)
                    .unwrap()
                    .completion
                    .unwrap()
                    .is_ready()
            );
            Self::InstanceGraph {
                graph,
                handle,
                work: Work::default(),
            }
        }
    }

    fn scene(&self) -> &Scene {
        match self {
            Self::Templates { scene, .. } => scene,
            Self::Views(tree) => tree.scene(),
            Self::InstanceGraph { graph, .. } => graph.scene(),
        }
    }

    fn update(&mut self, input: CycleInputs, size: (u32, u32), scale: f32) -> Damage {
        match self {
            Self::Templates {
                bank,
                handle,
                scene,
                work,
            } => {
                bank.stage(*handle, input).unwrap();
                let mut damage = Damage::None;
                *work = bank
                    .submit(|changes| {
                        damage = scene.patch(changes)?;
                        Ok(())
                    })
                    .unwrap();
                damage
            }
            Self::Views(tree) => {
                tree.set_root(CycleView { input });
                tree.update(size, scale).unwrap()
            }
            Self::InstanceGraph {
                graph,
                handle,
                work,
            } => {
                graph.stage(*handle, input).unwrap();
                let update = graph.advance(size, scale, None).unwrap();
                *work = update.work;
                let Poll::Ready(damage) = update.completion.unwrap() else {
                    panic!("inline layout queued")
                };
                damage
            }
        }
    }

    fn resources(&self) -> Value {
        let updated = !matches!(self, Self::Templates { work, .. } | Self::InstanceGraph { work, .. } if work.instances_changed == 0);
        let mut result = json!({
            "scene_nodes": self.scene().geometry().len(),
            "draw_instances": self.scene().frame().instances().len(),
            "layout_computations": if updated { self.scene().stats.layout_computations } else { 0 },
            "recordings": if updated { self.scene().stats.recordings } else { 0 },
        });
        match self {
            Self::Templates { bank, work, .. } => {
                result["semantic_instances"] = json!(bank.len());
                result["inline_instance_bytes"] = json!(Bank::<CycleInputs>::instance_bytes());
                result["input_record_bytes"] = json!(std::mem::size_of::<CycleInputs>());
                result["sources_compared"] = json!(work.sources_compared);
                result["bindings_emitted"] = json!(work.bindings_emitted);
            }
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
    choices: [CycleInputs; 2],
}

impl Run {
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
#[ignore = "Direct view IR experiment; use make bench BENCH_SUITE=view-ir"]
fn view_ir_benchmark() {
    let mut cases = Vec::new();
    let run_index: usize = std::env::var("RETINUE_BENCH_RUN_INDEX")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let mut backends = [Mode::Views, Mode::Templates, Mode::InstanceGraph];
    backends.rotate_left(run_index % 3);
    if run_index % 6 >= 3 {
        backends.reverse();
    }
    for scale in [1.0_f32, 1.2, 1.5] {
        let size = ((430.0 * scale) as u32, (100.0 * scale) as u32);
        for change in ["noop", "timer", "color", "width", "offset"] {
            for mode in backends {
                for painting in [false, true] {
                    let name = format!(
                        "{change}-{}-{}",
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
                        || {
                            let a = CycleInputs::default();
                            let mut b = a.clone();
                            match change {
                                "timer" => b.timer = "00:09:59".into(),
                                "color" => b.color = Color::rgb(0x7cb895),
                                "width" => b.width = 390.0,
                                "offset" => b.offset = (7.25, 2.5),
                                _ => {}
                            }
                            let mut run = Run {
                                backend: Backend::new(mode, size, scale),
                                renderer: painting
                                    .then(|| Renderer::new(ResidencyPolicy::desktop(0))),
                                pixels: if painting {
                                    vec![0; size.0 as usize * size.1 as usize * 4]
                                } else {
                                    Vec::new()
                                },
                                choices: [a, b],
                            };
                            run.paint(Damage::Full);
                            run
                        },
                        |run, index| {
                            let damage =
                                run.backend
                                    .update(run.choices[index % 2].clone(), size, scale);
                            run.paint(damage);
                            black_box(run.backend.scene().frame());
                        },
                        |run| {
                            let mut resources = run.backend.resources();
                            resources["framebuffer_bytes"] = json!(run.pixels.len());
                            resources
                        },
                    );
                    case["actual_threads"] = json!(if painting {
                        bench::render_threads().get()
                    } else {
                        1
                    });
                    case["items"] = json!(1);
                    cases.push(case);
                }
            }
        }
    }
    cases.extend(super::collection_benchmark::cases(run_index));
    bench::report_with_scope(
        "view-ir",
        cases,
        "One matched Cycle view or 32-item keyed list update per sample, except filter-cycle which measures both 32-to-16 removal and 16-to-32 recreation together. Views uses retained ViewTree, template isolates typed-bank patches, graph includes heterogeneous banks, transactional topology and generation lifetimes. List inputs are replaced at the collection boundary in both paths, not updated by per-item observation. Replacement remounts one item, reorder rotates three positions. Both IR paths use handwritten typed bindings and fused static wrappers, not a compiler frontend. Update includes source evaluation, inline layout and command recording; frame adds damage repair and native completion with optional rasters disabled. Same output is verified separately. No observation, input routing, motion, queued layout, compositor or application migration. Font and template registration are warm. Inline instance bytes exclude owned allocations and backend storage; they are not total memory.",
    );
}
