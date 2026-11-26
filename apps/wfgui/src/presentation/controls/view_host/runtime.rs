use retinue_render::{Key, damage::Damage};
use retinue_ui::{
    authoring,
    environment::Environment,
    ir::InstanceGraph,
    runtime::{LayoutExecution, UpdateStats},
    scene::{Geometry, Scene},
    timeline::FrameTime,
};
use std::{task::Poll, time::Instant};

pub(super) struct Runtime {
    graph: InstanceGraph,
    stats: UpdateStats,
}

impl Runtime {
    pub fn compiled(view: impl authoring::View, environment: Environment) -> Self {
        let mut graph = InstanceGraph::with_host_routed_input();
        let root = graph
            .mount_view(view, environment)
            .expect("valid compiled page");
        graph.set_root(Some(root)).unwrap();
        Self {
            graph,
            stats: UpdateStats::default(),
        }
    }

    pub fn scene(&self) -> &Scene {
        self.graph.scene()
    }
    pub fn bounds(&self, name: Key) -> Option<Geometry> {
        self.graph.named_bounds(name).expect("unique page key")
    }
    pub fn authored_key(&self, key: Key) -> Option<Key> {
        self.graph.authored_key(key)
    }
    pub fn contains(&self, name: Key) -> bool {
        self.graph.has_name(name)
    }
    pub fn needs_update(&self) -> bool {
        self.graph.has_pending_inputs()
    }
    pub fn pending(&self) -> bool {
        self.graph.is_layout_pending()
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        self.graph.next_deadline()
    }
    pub fn stats(&self) -> UpdateStats {
        self.stats
    }

    #[cfg(all(test, feature = "profiling"))]
    pub fn profile(&self) -> retinue_ui::runtime::UpdateProfile {
        self.graph.profile
    }

    pub fn update(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: FrameTime,
        execution: Option<&LayoutExecution>,
    ) -> Poll<Damage> {
        let update = self
            .graph
            .advance_at(size, scale, time, execution)
            .expect("valid compiled page");
        self.stats = UpdateStats {
            bodies: update.work.owners_evaluated,
            mounted: update.work.instances_mounted,
            unmounted: update.work.instances_unmounted,
            projected: update.work.nodes_materialized,
            invalid_deadlines: update.work.invalid_deadlines,
            input_updates: update.work.input_updates,
            ..Default::default()
        };
        update.completion.expect("valid compiled page layout")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_render::Color;
    use retinue_ui::{authoring::*, state::State};

    #[derive(PartialEq)]
    struct OptionalContent {
        visible: State<bool>,
    }

    #[view]
    impl View for OptionalContent {
        fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
            ui! {
                if self.visible.get() {
                    Rectangle(Color::rgb(0x123456))
                }
            }
        }
    }

    #[test]
    fn lifecycle_counters_report_branch_changes_and_reset_on_idle() {
        let visible = State::new(false);
        let mut runtime = Runtime::compiled(
            OptionalContent {
                visible: visible.clone(),
            },
            Environment::default(),
        );
        let time = FrameTime::now();
        assert!(runtime.update((160, 100), 1.0, time, None).is_ready());
        visible.set(true);
        assert!(runtime.update((160, 100), 1.0, time, None).is_ready());
        assert!(runtime.stats().mounted > 0);
        visible.set(false);
        assert!(runtime.update((160, 100), 1.0, time, None).is_ready());
        assert!(runtime.stats().unmounted > 0);
        assert!(runtime.update((160, 100), 1.0, time, None).is_ready());
        assert_eq!(runtime.stats().mounted, 0);
        assert_eq!(runtime.stats().unmounted, 0);
    }
}
