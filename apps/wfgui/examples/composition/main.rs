#[allow(dead_code)]
#[path = "../../../../tools/render-bench/support.rs"]
mod bench;
mod model;

use clap::Parser;
use model::{BACKGROUND, Demo};
use retinue_ui::{
    composition::{RasterMode, RecordingMode, Scene},
    damage::Damage,
    desktop::{
        self, Application,
        accesskit::{Action, ActionRequest, Node, NodeId, Role, TreeInfo, TreeUpdate},
        winit::{
            dpi::LogicalSize,
            event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
            keyboard::{Key, NamedKey},
            window::{Window, WindowAttributes},
        },
    },
    painter::{Painter, render_thread_budget, straight_rgba},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    capture: bool,
    #[arg(long)]
    benchmark: bool,
    #[arg(long, default_value = "research/retinue/ui-composition")]
    output: PathBuf,
    #[arg(long, default_value_t = 1.0)]
    scale: f32,
}

fn geometry(demo: &Demo) -> Value {
    json!({
        "scale": demo.scale, "size": demo.size,
        "stats": { "nodes": demo.scene.stats.nodes, "layout_computations": demo.scene.stats.layout_computations, "recordings": demo.scene.stats.recordings, "visible_chunks": demo.scene.stats.visible_chunks },
        "nodes": demo.scene.geometry().iter().map(|node| json!({
            "key":node.key.value, "scope":node.key.namespace, "layout":[node.layout.x,node.layout.y,node.layout.width,node.layout.height],
            "paint":[node.paint.x,node.paint.y,node.paint.width,node.paint.height],
            "hit":[node.hit.x,node.hit.y,node.hit.width,node.hit.height], "interactive":node.interactive,
        })).collect::<Vec<_>>()
    })
}

fn capture(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&args.output)?;
    for scale in [1.0, 1.25, 2.0] {
        let mut demo = Demo::new(scale);
        let now = Instant::now();
        demo.motion.select(11, now, false);
        for (name, elapsed) in [("initial", 0), ("transition", 85), ("selected", 170)] {
            demo.update(
                now + Duration::from_millis(elapsed),
                RecordingMode::Retained,
            );
            demo.renderer.prepare(&demo.scene, RasterMode::Explicit);
            let mut pixels = vec![0; demo.size.0 as usize * demo.size.1 as usize * 4];
            let mut painter = Painter::with_threads(
                &mut pixels,
                demo.size.0,
                demo.size.1,
                render_thread_budget(4),
            )?;
            demo.renderer.paint(
                &demo.scene,
                &mut painter,
                Damage::Full.bounds(demo.size).unwrap(),
                BACKGROUND,
            );
            painter.finish()?;
            let stem = format!("composition-{}-{name}", (scale * 100.0) as u32);
            let image = straight_rgba(&pixels, demo.size.0, demo.size.1)?;
            image.save(args.output.join(format!("{stem}.png")))?;
            if name == "selected" {
                for (key, group) in [(1000, "card"), (2, "tabs")] {
                    let rect = demo
                        .scene
                        .geometry()
                        .iter()
                        .find(|node| node.key.value == key)
                        .unwrap()
                        .hit;
                    retinue_ui::image::imageops::crop_imm(
                        &image,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                    )
                    .to_image()
                    .save(args.output.join(format!(
                        "composition-{}-{group}.png",
                        (scale * 100.0) as u32
                    )))?;
                }
            }
            fs::write(
                args.output.join(format!("{stem}.json")),
                serde_json::to_vec_pretty(&geometry(&demo))?,
            )?;
        }
    }
    Ok(())
}

fn benchmark() {
    let mut cases = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        for workload in ["scroll", "counter"] {
            for mode in ["rebuild", "commands-full", "commands", "raster"] {
                let name = format!("{workload}-{mode}-{}", (scale * 100.0) as u32);
                let size = ((960.0 * scale) as u32, (680.0 * scale) as u32);
                let now = Instant::now();
                cases.push(bench::measure(
                    &name,size,scale,
                    || (Demo::new(scale),vec![0;size.0 as usize*size.1 as usize*4],Vec::new()),
                    |(demo,pixels,stages),index| {
                        if workload == "scroll" { demo.scroll = (index%100) as f32*3.0; }
                        demo.counter = 2 + index as u32 % 4;
                        demo.replacement = index % 40 >= 20;
                        if mode == "rebuild" { demo.scene = Scene::default(); }
                        let start = Instant::now();
                        let damage = demo.update(now, if mode == "rebuild" {RecordingMode::Always} else {RecordingMode::Retained});
                        let update = start.elapsed().as_nanos() as u64;
                        let start = Instant::now();
                        demo.renderer.prepare(&demo.scene,if mode == "raster" {RasterMode::Explicit} else {RasterMode::Replay});
                        let prepare = start.elapsed().as_nanos() as u64;
                        let start = Instant::now();
                        let damage = if mode == "commands-full" { Damage::Full } else { damage };
                        if let Some(repair) = damage.bounds(size) {
                            let mut painter = Painter::with_threads(pixels,size.0,size.1,bench::render_threads()).unwrap();
                            demo.renderer.paint(&demo.scene,&mut painter,repair,BACKGROUND);
                            painter.finish().unwrap();
                        }
                        stages.push([update,prepare,start.elapsed().as_nanos() as u64]);
                    },
                    |(demo,_,stages)| {
                        let cache = demo.renderer.cache_stats();
                        json!({"geometry":geometry(&demo),"stages_ns":stages,"cache":{"bytes":cache.cached_bytes,"live_bytes":cache.live_bytes,"hits":cache.hits,"misses":cache.misses,"evictions":cache.evictions},"last_paint":{"commands":demo.renderer.stats.replayed_commands,"blits":demo.renderer.stats.blits,"prepared":demo.renderer.stats.prepared_rasters}})
                    },
                ));
            }
        }
    }
    bench::report_with_scope(
        "composition",
        cases,
        "Experimental composition only. Headless update, preparation and queued painting including completion. No presentation, game impact or whole-GUI before/after claim. Rebuild is the same scene recreated each frame; commands/raster retain it. Stage timings include warmup; frame distribution excludes it.",
    );
}

struct App {
    demo: Demo,
    cursor: (f64, f64),
    focus: u64,
    started: Instant,
    arrived: bool,
}

impl App {
    fn refresh(&mut self, now: Instant) -> Damage {
        let damage = self.demo.update(now, RecordingMode::Retained);
        self.demo
            .renderer
            .prepare(&self.demo.scene, RasterMode::Explicit);
        damage
    }
}

impl Application for App {
    type Message = ();

    fn attributes(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title("Retinue composition prototype")
            .with_inner_size(LogicalSize::new(self.demo.size.0, self.demo.size.1))
            .with_min_inner_size(LogicalSize::new(640, 480))
    }
    fn resize(&mut self, size: (u32, u32), _: f64) {
        self.demo.size = size;
        self.refresh(Instant::now());
    }
    fn event(&mut self, event: &WindowEvent, _: &Window) -> Damage {
        let now = Instant::now();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                return Damage::None;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(key @ 10..=13) =
                    self.demo.scene.hit_test(self.cursor).map(|key| key.value)
                {
                    self.focus = key;
                    self.demo.motion.select(key, now, false);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => self.demo.scroll_by(match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * 48.0,
                MouseScrollDelta::PixelDelta(p) => -p.y as f32 / self.demo.scale,
            }),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Tab) | Key::Named(NamedKey::ArrowRight) => {
                        self.focus = 10 + (self.focus - 9) % 4;
                        self.demo.motion.select(self.focus, now, false);
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        self.focus = 10 + (self.focus - 10 + 3) % 4;
                        self.demo.motion.select(self.focus, now, false);
                    }
                    Key::Named(NamedKey::Space) => self.demo.counter += 1,
                    Key::Character(key) if key == "r" => {
                        self.demo.replacement = !self.demo.replacement
                    }
                    Key::Character(key) if key == "c" => {
                        self.demo.renderer.clear_cache();
                        return self.refresh(now).union(Damage::Full);
                    }
                    _ => return Damage::None,
                }
            }
            _ => return Damage::None,
        }
        self.refresh(now)
    }
    fn message(&mut self, _: ()) -> Damage {
        Damage::None
    }
    fn paint(&mut self, painter: &mut Painter<'_>) -> Result<(), String> {
        self.demo.renderer.paint(
            &self.demo.scene,
            painter,
            Damage::Full.bounds(self.demo.size).unwrap(),
            BACKGROUND,
        );
        Ok(())
    }
    fn background(&self) -> [u8; 4] {
        BACKGROUND.bgra()
    }
    fn accessibility(&self) -> TreeUpdate {
        let mut nodes = Vec::new();
        for geometry in self.demo.scene.geometry().iter().filter(|g| g.interactive) {
            let mut node = Node::new(Role::Button);
            node.set_label(
                ["Warframes", "Primary", "Secondary", "Melee"][(geometry.key.value - 10) as usize],
            );
            node.add_action(Action::Click);
            node.set_bounds(desktop::accesskit::Rect {
                x0: geometry.hit.x as f64,
                y0: geometry.hit.y as f64,
                x1: (geometry.hit.x + geometry.hit.width) as f64,
                y1: (geometry.hit.y + geometry.hit.height) as f64,
            });
            nodes.push((NodeId(geometry.key.value), node));
        }
        let mut root = Node::new(Role::Window);
        root.set_children(nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>());
        nodes.push((NodeId(1), root));
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(1))),
            focus: NodeId(self.focus),
            tree_id: desktop::accesskit::TreeId::ROOT,
        }
    }
    fn access(&mut self, request: ActionRequest, _: &Window) -> Damage {
        if request.action == Action::Click && (10..=13).contains(&request.target_node.0) {
            self.focus = request.target_node.0;
            self.demo.motion.select(self.focus, Instant::now(), false);
            self.refresh(Instant::now())
        } else {
            Damage::None
        }
    }
    fn tick(&mut self, now: Instant) -> Damage {
        if !self.arrived && now >= self.started + Duration::from_secs(2) {
            self.arrived = true;
            self.demo.replacement = true;
        }
        self.refresh(now)
    }
    fn deadline(&self) -> Option<Instant> {
        let motion = self.demo.motion.deadline(Instant::now());
        if self.arrived {
            motion
        } else {
            Some(motion.map_or(self.started + Duration::from_secs(2), |at| {
                at.min(self.started + Duration::from_secs(2))
            }))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.scale.is_finite() || !(0.5..=3.0).contains(&args.scale) {
        return Err("scale must be 0.5..3".into());
    }
    if args.capture || args.benchmark {
        capture(&args)?;
        if args.benchmark {
            benchmark();
        }
        println!("{}", args.output.display());
    } else {
        desktop::run(
            move |_| {
                let mut app = App {
                    demo: Demo::new(args.scale),
                    cursor: (0.0, 0.0),
                    focus: 10,
                    started: Instant::now(),
                    arrived: false,
                };
                app.refresh(Instant::now());
                Ok(app)
            },
            desktop::Options {
                capture: None,
                exit_after_capture: false,
            },
        )?;
    }
    Ok(())
}
