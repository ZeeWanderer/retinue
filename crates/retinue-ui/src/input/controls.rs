use super::{Activation, Button, Event, InputOptions, PointerId, PointerPhase, Router, Target};
use crate::{Bounds, runtime::ViewId};
use rustc_hash::FxHashMap as HashMap;
use std::hash::Hash;

/// Named input regions for custom drawing and hosts with app-owned actions.
pub struct ControlRegion<K> {
    pub key: K,
    pub bounds: Bounds,
    pub clip: Bounds,
    pub enabled: bool,
    pub input: InputOptions,
}

impl<K> ControlRegion<K> {
    pub fn new(key: K, bounds: Bounds) -> Self {
        Self {
            key,
            bounds,
            clip: bounds,
            enabled: true,
            input: InputOptions {
                activation: Activation::Release,
                blocks_when_disabled: true,
                ..Default::default()
            },
        }
    }

    pub fn geometry(key: K, geometry: crate::scene::Geometry) -> Self {
        Self {
            clip: geometry.clip,
            enabled: geometry.interactive,
            ..Self::new(key, geometry.layout)
        }
    }
}

pub struct ControlDispatch<K> {
    pub target: Option<K>,
    pub activated: Option<K>,
}

pub struct Controls<K> {
    router: Router,
    ids: HashMap<K, ViewId>,
    keys: HashMap<ViewId, K>,
    next: u64,
}

impl<K> Default for Controls<K> {
    fn default() -> Self {
        Self {
            router: Router::default(),
            ids: HashMap::default(),
            keys: HashMap::default(),
            next: 0,
        }
    }
}

impl<K: Eq + Hash + Clone> Controls<K> {
    /// Publish a complete layout snapshot in back-to-front order. Omitted regions
    /// lose capture/focus; moving a retained key preserves its press ownership.
    pub fn update(&mut self, regions: impl IntoIterator<Item = ControlRegion<K>>) {
        let regions: Vec<_> = regions.into_iter().collect();
        let same_order = regions.len() == self.router.targets().len()
            && regions
                .iter()
                .zip(self.router.targets())
                .all(|(region, target)| self.keys.get(&target.id) == Some(&region.key));
        if same_order {
            let targets = regions
                .into_iter()
                .zip(self.router.targets())
                .map(|(region, old)| Target {
                    id: old.id,
                    parent: None,
                    bounds: region.bounds,
                    clip: region.clip,
                    enabled: region.enabled,
                    input: region.input,
                })
                .collect();
            self.router.update(targets);
            return;
        }
        let mut ids = HashMap::default();
        let mut keys = HashMap::default();
        let targets = regions
            .into_iter()
            .map(|region| {
                let id = self.ids.get(&region.key).copied().unwrap_or_else(|| {
                    self.next = self.next.checked_add(1).expect("input identity exhausted");
                    ViewId(self.next)
                });
                assert!(
                    ids.insert(region.key.clone(), id).is_none(),
                    "duplicate control key"
                );
                keys.insert(id, region.key);
                Target {
                    id,
                    parent: None,
                    bounds: region.bounds,
                    clip: region.clip,
                    enabled: region.enabled,
                    input: region.input,
                }
            })
            .collect();
        self.router.update(targets);
        self.ids = ids;
        self.keys = keys;
    }

    pub fn hit(&self, position: (f32, f32)) -> Option<&K> {
        self.keys.get(&self.router.hit_test(position)?)
    }

    pub fn captured(&self) -> Option<&K> {
        self.keys.get(&self.router.captured(PointerId::MOUSE)?)
    }

    pub fn pointer(&mut self, phase: PointerPhase, position: (f32, f32)) -> ControlDispatch<K> {
        let result = self.router.dispatch(&Event::Pointer {
            id: PointerId::MOUSE,
            phase,
            position,
            modifiers: Default::default(),
        });
        ControlDispatch {
            target: result.target.and_then(|id| self.keys.get(&id).cloned()),
            activated: result.activated.and_then(|id| self.keys.get(&id).cloned()),
        }
    }

    pub fn button(&mut self, down: bool, position: (f32, f32)) -> ControlDispatch<K> {
        self.pointer(
            if down {
                PointerPhase::Down(Button::Primary)
            } else {
                PointerPhase::Up(Button::Primary)
            },
            position,
        )
    }

    pub fn cancel(&mut self) {
        self.pointer(PointerPhase::Cancel, (0.0, 0.0));
    }

    pub fn focused(&self) -> Option<&K> {
        self.keys.get(&self.router.focused()?)
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.router
            .targets()
            .iter()
            .filter_map(|target| self.keys.get(&target.id))
    }

    pub fn focus_next(&mut self, reverse: bool) {
        self.router.dispatch(&Event::Key {
            key: super::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: super::Modifiers {
                shift: reverse,
                ..Default::default()
            },
        });
    }

    pub fn focus(&mut self, key: Option<&K>) {
        self.router
            .focus(key.and_then(|key| self.ids.get(key).copied()));
    }

    pub fn dispatch(&mut self, event: &Event) -> super::Dispatch {
        self.router.dispatch(event)
    }
}
