use ui_alias::{
    authoring::{Body, ForEach, HStack, Text, VStack, View, ViewContext, ui, view},
    environment::Environment,
    ir::InstanceGraph,
    state::State,
};

struct QueueView {
    expanded: State<bool>,
    jobs: State<Vec<(u64, &'static str)>>,
}

#[view]
impl View for QueueView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            VStack(8.0) {
                Text("Queue").font_size(20.0)
                if self.expanded.get() {
                    ForEach(self.jobs.get(), |job| job.0) { |(_, name)|
                        let label = name.to_owned();
                        HStack(6.0) {
                            Text(label)
                            Text("Waiting")
                        }
                    }
                }
                match self.jobs.get().len() {
                    0 => Text("No pending jobs"),
                    1 => Text("One pending job"),
                    _ => Text("Several pending jobs"),
                }
            }
        }
    }
}

#[test]
fn compiled_dsl_uses_the_renamed_runtime_for_dynamic_content() {
    let expanded = State::new(true);
    let jobs = State::new(vec![(1, "First"), (2, "Second")]);
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_view(
            QueueView {
                expanded: expanded.clone(),
                jobs: jobs.clone(),
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph.advance((400, 300), 1.0, None).unwrap();
    let populated = graph.len();
    expanded.set(false);
    graph.advance((400, 300), 1.0, None).unwrap();
    assert!(graph.len() < populated);
    jobs.set(vec![(2, "Second")]);
    expanded.set(true);
    graph.advance((400, 300), 1.0, None).unwrap();
    assert!(graph.scene().frame().instances().len() >= 3);
}

#[test]
fn large_child_blocks_flatten_without_a_solver_container_per_tuple() {
    use ui_alias::{
        Key,
        authoring::{Frame, Rectangle, ViewExt},
    };
    let mut graph = InstanceGraph::default();
    let color = ui_alias::prelude::Color::rgb(0xffffff);
    let root = graph
        .mount_body(
            move |_| {
                ui! {
                    VStack(1.0) {
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0))
                        Rectangle(color).frame(Frame::new().height(10.0)).named(Key::new("last", 0))
                    }
                }
            },
            Environment::default(),
        )
        .unwrap();
    graph.set_root(Some(root)).unwrap();
    graph.advance((400, 300), 1.0, None).unwrap();
    let last = graph
        .geometry(graph.unique(Key::new("last", 0)).unwrap().unwrap())
        .unwrap();
    assert_eq!(last.layout.y, 88.0);
}
