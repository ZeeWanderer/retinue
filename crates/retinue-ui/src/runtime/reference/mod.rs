//! Reference reconciler for comparison with compiled views.

pub mod prelude {
    pub use crate::motion::TranslationTransition;
    pub use crate::prelude::common::*;
    pub use crate::view::{
        Alignment, AnimatedCanvas, Canvas, Distribution, Element, HStack, Image, IntoChildren,
        IntoElement, LayoutView, Rectangle, ScrollAxes, ScrollViewport, Switch, Text, VStack, View,
        ViewContext, ViewExt, ZStack, empty,
    };
    pub use retinue_ui_macros::ui;
}

#[cfg(test)]
mod contracts;
mod execution;
mod input;
mod inspect;
mod motion;
mod node;
mod projection;
mod queue;
mod reconcile;
#[cfg(test)]
mod tests;

use super::{LayoutExecution, UpdateStats, ViewId, instance, profile};
use crate::{
    Key,
    environment::Environment,
    scene::Scene,
    state::Mutation,
    timeline::{FrameTime, Timing},
    view::{Element, IntoElement},
};
pub use inspect::{AmbiguousKey, ViewInfo};
use node::Node;
#[cfg(feature = "profiling")]
pub use profile::UpdateProfile;
use profile::timed;
use queue::Queue;
use retinue_render::{PaintFrame, damage::Damage};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

/// UI-thread host. The wake callback schedules an update; it must not render inline.
pub struct ViewTree {
    nodes: HashMap<ViewId, Rc<RefCell<Node>>>,
    root: Option<ViewId>,
    next_id: u64,
    pending_root: Option<(Element, Option<Mutation>)>,
    environment: Environment,
    queue: Rc<Queue>,
    scene: Scene,
    changed: HashSet<ViewId>,
    structural: HashSet<ViewId>,
    full_projection: bool,
    input: crate::input::Router,
    input_nodes: HashSet<ViewId>,
    input_keys: HashMap<ViewId, Key>,
    input_dirty: bool,
    input_revision: u64,
    time: FrameTime,
    timing: Timing<ViewId>,
    epoch: u64,
    animation_scope_epoch: Option<u64>,
    frame_interval: Duration,
    motion_enabled: bool,
    presentations: HashMap<ViewId, crate::motion::presentation::Presentation>,
    names: HashMap<Key, Vec<ViewId>>,
    pub stats: UpdateStats,
    #[cfg(feature = "profiling")]
    pub profile: UpdateProfile,
}

impl ViewTree {
    pub fn new(root: impl IntoElement, wake: impl Fn() + 'static) -> Self {
        Self {
            nodes: HashMap::default(),
            root: None,
            next_id: 1,
            pending_root: Some((root.into_element(), None)),
            environment: Environment::default(),
            queue: Rc::new(Queue::new(wake)),
            scene: Scene::default(),
            changed: HashSet::default(),
            structural: HashSet::default(),
            full_projection: true,
            input: crate::input::Router::default(),
            input_nodes: HashSet::default(),
            input_keys: HashMap::default(),
            input_dirty: true,
            input_revision: 0,
            time: FrameTime::now(),
            timing: Timing::default(),
            epoch: 0,
            animation_scope_epoch: None,
            frame_interval: Duration::from_nanos(16_666_667),
            motion_enabled: true,
            presentations: HashMap::default(),
            names: HashMap::default(),
            stats: UpdateStats::default(),
            #[cfg(feature = "profiling")]
            profile: UpdateProfile::default(),
        }
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn frame(&self) -> &PaintFrame {
        self.scene.frame()
    }
    pub fn root(&self) -> Option<ViewId> {
        self.root
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn needs_update(&self) -> bool {
        self.is_layout_pending() || self.needs_evaluation()
    }

    fn needs_evaluation(&self) -> bool {
        self.pending_root.is_some()
            || !self.queue.is_empty()
            || self.full_projection
            || !self.changed.is_empty()
    }

    pub fn last_mutation(&self, view: ViewId) -> Option<Mutation> {
        self.nodes.get(&view)?.borrow().last_mutation
    }

    pub fn set_root(&mut self, root: impl IntoElement) {
        let root = root.into_element();
        let same = match &self.pending_root {
            Some((pending, _)) => pending == &root,
            None => self.root.is_some_and(|id| self.same_input(id, &root)),
        };
        if same {
            return;
        }
        self.pending_root = Some((root, Some(Mutation::capture())));
        self.queue.wake();
    }

    pub fn update(&mut self, size: (u32, u32), scale: f32) -> Result<Damage, &'static str> {
        self.update_at(size, scale, FrameTime::now())
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        if self.is_layout_pending() {
            return None;
        }
        self.timing
            .deadline()
            .into_iter()
            .chain(
                self.presentations.values().filter_map(|track| {
                    track.tween.deadline(self.time.instant, self.frame_interval)
                }),
            )
            .min()
    }

    pub fn set_frame_interval(&mut self, interval: Duration) {
        assert!(
            !interval.is_zero(),
            "animation frame interval must be positive"
        );
        self.frame_interval = interval;
    }

    pub fn update_at(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: FrameTime,
    ) -> Result<Damage, &'static str> {
        match self.perform_update(size, scale, time, None)? {
            std::task::Poll::Ready(damage) => Ok(damage),
            std::task::Poll::Pending => unreachable!("inline layout"),
        }
    }

    fn perform_update(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: FrameTime,
        execution: Option<&LayoutExecution>,
    ) -> crate::scene::execution::Update {
        if self.is_layout_pending() {
            return Err("layout update pending");
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err("view scale must be positive and finite");
        }
        self.stats = UpdateStats::default();
        #[cfg(feature = "profiling")]
        let evaluation_started = {
            self.profile = UpdateProfile::default();
            self.scene.profile = Default::default();
            Instant::now()
        };
        self.epoch = self
            .epoch
            .checked_add(1)
            .expect("view update epoch exhausted");
        let mut turn = self.queue.begin();
        let clock_changed = time.clock_changed(self.time);
        self.time = time;
        self.timing.take_due(time.instant, clock_changed, |id| {
            let mut node = self.nodes[&id].borrow_mut();
            node.timed_update = true;
            self.queue.insert(node.depth, id);
        });
        if let Some((input, cause)) = self.pending_root.take() {
            if let Some(root) = self.root.filter(|id| self.compatible(*id, &input)) {
                self.set_input(root, input, cause);
            } else {
                if let Some(root) = self.root.take() {
                    self.unmount(root);
                }
                self.root = Some(self.mount(input, None, self.environment.clone(), cause));
                self.full_projection = true;
            }
        }
        let mut evaluated = HashSet::default();
        while let Some((depth, id)) = self.queue.pop() {
            if !self.nodes.contains_key(&id) {
                continue;
            }
            if !evaluated.insert(id) {
                self.queue.defer(depth, id);
                continue;
            }
            let mut retry = self.queue.attempt(depth, id);
            self.refresh(id)?;
            retry.complete();
        }
        self.sample_presentations();
        #[cfg(feature = "profiling")]
        {
            self.profile.evaluation = evaluation_started.elapsed();
        }
        let damage = self.project(size, scale, execution)?;
        if damage.is_ready() {
            timed!(self, input, self.sync_input());
        }
        turn.complete();
        Ok(damage)
    }
}
