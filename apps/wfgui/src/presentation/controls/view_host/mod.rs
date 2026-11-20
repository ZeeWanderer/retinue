pub(in crate::presentation) mod paint_slot;
mod root_view;
mod runtime;

use retinue_render::{CachePolicy, Command, Key, damage::Damage};
use retinue_ui::{
    drawing::DrawList, runtime::LayoutExecution, scene::Geometry, state::State, timeline::FrameTime,
};
use std::task::Poll;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub(super) const CHROME: Key = Key::new("page.controls", 0);
pub(super) const OVERLAYS: Key = Key::new("page.overlays", 0);

#[derive(Clone, PartialEq)]
struct PaintContent {
    drawing: DrawList,
    cache: CachePolicy,
}

impl Default for PaintContent {
    fn default() -> Self {
        Self {
            drawing: DrawList::default(),
            cache: CachePolicy::Never,
        }
    }
}

#[derive(Clone, Default)]
struct PaintSources(Rc<RefCell<HashMap<Key, State<PaintContent>>>>);

impl PartialEq for PaintSources {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl PaintSources {
    fn slot(&self, key: Key) -> State<PaintContent> {
        self.0.borrow_mut().entry(key).or_default().clone()
    }
}

pub(crate) struct Host {
    runtime: runtime::Runtime,
    paints: PaintSources,
    pub images: crate::domain::media::image_store::ImageStore,
    time: FrameTime,
    viewport: ((u32, u32), f32),
    pub execution: Option<LayoutExecution>,
}

impl Host {
    pub fn empty() -> Self {
        Self::compiled(root_view::Empty)
    }

    pub fn compiled(view: impl retinue_ui::authoring::View) -> Self {
        let paints = PaintSources::default();
        let images = crate::domain::media::image_store::ImageStore::default();
        let environment = retinue_ui::environment::Environment::default();
        environment.provide(paints.clone());
        environment.provide(images.clone());
        Self {
            runtime: runtime::Runtime::compiled(root_view::RootView(view), environment),
            paints,
            images,
            time: FrameTime::now(),
            viewport: ((0, 0), 1.0),
            execution: None,
        }
    }

    pub fn scene(&self) -> &retinue_ui::scene::Scene {
        self.runtime.scene()
    }
    pub fn needs_update(&self) -> bool {
        self.runtime.needs_update()
    }
    pub fn is_layout_pending(&self) -> bool {
        self.runtime.pending()
    }
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        self.runtime.next_deadline()
    }
    pub fn authored_key(&self, key: Key) -> Option<Key> {
        self.runtime.authored_key(key)
    }
    pub fn stats(&self) -> retinue_ui::runtime::UpdateStats {
        self.runtime.stats()
    }
    pub fn input_bounds(&self, name: Key) -> Option<Geometry> {
        self.runtime.bounds(name)
    }
    pub fn named_geometry(&self) -> impl Iterator<Item = Geometry> + '_ {
        self.scene().geometry().iter().filter_map(|geometry| {
            Some(Geometry {
                key: self.authored_key(geometry.key)?,
                parent: geometry.parent.and_then(|key| self.authored_key(key)),
                ..*geometry
            })
        })
    }
    #[cfg(all(test, feature = "profiling"))]
    pub fn profile(&self) -> retinue_ui::runtime::UpdateProfile {
        self.runtime.profile()
    }
    #[cfg(test)]
    pub fn stable_key(&self, name: Key) -> Option<Key> {
        self.runtime.bounds(name).map(|geometry| geometry.key)
    }

    pub fn bounds(&self, key: Key) -> Option<Geometry> {
        self.runtime.bounds(key).map(|geometry| Geometry {
            key,
            parent: geometry.parent.and_then(|key| self.authored_key(key)),
            ..geometry
        })
    }

    pub fn layout_time(&self, time: FrameTime) -> FrameTime {
        if self.is_layout_pending() {
            self.time
        } else {
            time
        }
    }

    pub fn update(&mut self, size: (u32, u32), scale: f32, time: FrameTime) -> Poll<Damage> {
        let time = self.layout_time(time);
        self.time = time;
        self.viewport = (size, scale);
        let damage = self
            .runtime
            .update(size, scale, time, self.execution.as_ref());
        if damage.is_pending() {
            return damage;
        }
        self.paints
            .0
            .borrow_mut()
            .retain(|key, _| self.runtime.contains(*key));
        damage
    }

    pub fn record(&self, key: Key, commands: Vec<Command>, cache: CachePolicy) {
        if self.runtime.contains(key) {
            self.paints.slot(key).set(PaintContent {
                drawing: commands.into(),
                cache,
            });
        }
    }

    pub fn prepare(&mut self, size: (u32, u32), scale: f32) {
        if self.is_layout_pending() {
            return;
        }
        if self.needs_update() || self.viewport != (size, scale) {
            self.viewport = (size, scale);
            assert!(self.runtime.update(size, scale, self.time, None).is_ready());
        }
    }
}
