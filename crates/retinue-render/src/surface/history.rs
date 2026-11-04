use super::Snapshot;
use std::{collections::VecDeque, rc::Rc};

#[derive(Default)]
pub(crate) struct History {
    frames: VecDeque<Rc<Snapshot>>,
}

impl History {
    pub fn clear(&mut self) {
        self.frames.clear();
    }
    pub fn latest(&self) -> Option<&Snapshot> {
        self.frames.front().map(Rc::as_ref)
    }
    pub fn latest_owned(&self) -> Option<Rc<Snapshot>> {
        self.frames.front().cloned()
    }
    pub fn at_age(&self, age: u8) -> Option<&Snapshot> {
        age.checked_sub(1)
            .and_then(|n| self.frames.get(usize::from(n)))
            .map(Rc::as_ref)
    }
    pub fn submitted(&mut self, scene: Rc<Snapshot>) {
        self.frames.push_front(scene);
        self.frames.truncate(9);
    }
}

#[derive(Default)]
pub(crate) struct SlotHistory {
    slots: Vec<Option<Rc<Snapshot>>>,
    latest: Option<Rc<Snapshot>>,
}

impl SlotHistory {
    pub fn clear(&mut self) {
        self.slots.clear();
        self.latest = None;
    }
    pub fn add_buffer(&mut self) {
        self.slots.push(None);
    }
    pub fn latest(&self) -> Option<&Snapshot> {
        self.latest.as_deref()
    }
    pub fn take(&mut self, index: usize) -> Option<Rc<Snapshot>> {
        self.slots[index].take()
    }
    pub fn at(&self, index: usize) -> Option<Rc<Snapshot>> {
        self.slots.get(index)?.clone()
    }
    pub fn submitted(&mut self, index: usize, scene: Option<Rc<Snapshot>>) {
        self.slots[index] = scene.clone();
        self.latest = scene;
    }
}
