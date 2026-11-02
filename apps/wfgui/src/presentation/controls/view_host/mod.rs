mod paint_slot;
mod root_view;

pub(in crate::presentation) use paint_slot::PaintSlot;
use retinue_render::{CachePolicy, Command, Key, damage::Damage};
use retinue_ui::{
    drawing::DrawList, prelude::*, runtime::ViewTree, scene::Geometry, timeline::FrameTime,
};
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
    pub tree: ViewTree,
    paints: PaintSources,
    pub images: crate::domain::media::image_store::ImageStore,
    time: FrameTime,
    viewport: ((u32, u32), f32),
}

impl Host {
    pub fn new(view: impl IntoElement) -> Self {
        let paints = PaintSources::default();
        let tree = ViewTree::new(root_view::RootView(view.into_element()), || {});
        tree.environment().provide(paints.clone());
        let images = crate::domain::media::image_store::ImageStore::default();
        tree.environment().provide(images.clone());
        Self {
            tree,
            paints,
            images,
            time: FrameTime::now(),
            viewport: ((0, 0), 1.0),
        }
    }

    pub fn set_view(&mut self, view: impl IntoElement) {
        self.tree.set_root(root_view::RootView(view.into_element()));
    }

    pub fn bounds(&self, key: Key) -> Option<Geometry> {
        let id = self.tree.unique(key).expect("unique page key")?;
        self.tree.geometry(id).map(|geometry| Geometry {
            key,
            parent: geometry.parent.and_then(|key| self.tree.authored_key(key)),
            ..geometry
        })
    }

    pub fn update(&mut self, size: (u32, u32), scale: f32, time: FrameTime) -> Damage {
        self.time = time;
        self.viewport = (size, scale);
        let damage = self
            .tree
            .update_at(size, scale, time)
            .expect("valid page view");
        self.paints
            .0
            .borrow_mut()
            .retain(|key, _| !self.tree.views(*key).is_empty());
        damage
    }

    pub fn record(&self, key: Key, commands: Vec<Command>, cache: CachePolicy) {
        if !self.tree.views(key).is_empty() {
            self.paints.slot(key).set(PaintContent {
                drawing: commands.into(),
                cache,
            });
        }
    }

    pub fn prepare(&mut self, size: (u32, u32), scale: f32) {
        if self.tree.needs_update() || self.viewport != (size, scale) {
            self.viewport = (size, scale);
            self.tree
                .update_at(size, scale, self.time)
                .expect("valid page paint");
        }
    }
}
