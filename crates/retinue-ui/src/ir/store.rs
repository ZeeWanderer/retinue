use super::edits::Edits;
use super::owner::Reader;
use super::{Local, Result, Structure, Template, Work, template::node_key};
use crate::{Key, scene};
use rustc_hash::FxHashSet;
use slotmap::{SlotMap, SparseSecondaryMap, new_key_type};
use std::{
    any::Any,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

new_key_type! { struct InstanceId; }
static NEXT_BANK: AtomicU64 = AtomicU64::new(1);
static NEXT_NODE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle {
    pub(super) bank: u64,
    id: InstanceId,
}

pub(super) trait Pool: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn key(&self, handle: Handle, local: Local) -> Result<Key>;
    fn outlets(&self, handle: Handle) -> Result<&[Local]>;
    fn structure(&self, handle: Handle) -> Result<&Structure>;
    fn extent(&self, handle: Handle) -> Result<std::ops::Range<u64>>;
    fn project(
        &self,
        handle: Handle,
        outlets: Vec<(Local, Vec<scene::layout::Subtree>)>,
        proposed: bool,
        out: &mut Vec<scene::layout::Subtree>,
        edits: &mut Edits,
    ) -> Result<()>;
    fn prepare(&mut self, active: &FxHashSet<Handle>, changes: &mut Edits) -> Result<Work>;
    fn changed(&self, active: &FxHashSet<Handle>) -> bool;
    fn request(&mut self, handle: Handle) -> Option<bool>;
    fn take_reader(&mut self, handle: Handle) -> Result<Option<Reader>>;
    fn restore_reader(&mut self, handle: Handle, reader: Reader);
    fn accept(&mut self);
    fn discard(&mut self);
    fn remove(&mut self, handle: Handle) -> Result<()>;
    fn state(&mut self, handle: Handle, initialize: fn() -> Box<dyn Any>) -> Result<&mut dyn Any>;
    fn take_state(&mut self, handle: Handle) -> Result<Option<Box<dyn Any>>>;
    fn restore_state(&mut self, handle: Handle, state: Box<dyn Any>);
}

struct Instance<P> {
    program: Rc<Template<P>>,
    input: P,
    pending: Option<usize>,
    base: u64,
    revision: u64,
}

struct Pending<P> {
    id: InstanceId,
    input: P,
    revision: u64,
}

pub(super) struct Bank<P> {
    id: u64,
    instances: SlotMap<InstanceId, Instance<P>>,
    owner_state: SparseSecondaryMap<InstanceId, Box<dyn Any>>,
    readers: SparseSecondaryMap<InstanceId, Reader>,
    pending: Vec<Pending<P>>,
    affected: Vec<usize>,
    prepared: bool,
}

impl<P> Default for Bank<P> {
    fn default() -> Self {
        Self {
            id: NEXT_BANK.fetch_add(1, Ordering::Relaxed),
            instances: SlotMap::with_key(),
            owner_state: SparseSecondaryMap::new(),
            readers: SparseSecondaryMap::new(),
            pending: Vec::new(),
            affected: Vec::new(),
            prepared: false,
        }
    }
}

impl<P> Bank<P> {
    pub fn observe(&mut self, handle: Handle, reader: Reader) -> Result<()> {
        self.instance(handle)?;
        self.readers.insert(handle.id, reader);
        Ok(())
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn matches(&self, handle: Handle, program: &Rc<Template<P>>) -> bool {
        self.instance(handle)
            .is_ok_and(|instance| Rc::ptr_eq(&instance.program, program))
    }
    pub fn mount(&mut self, program: Rc<Template<P>>, input: P) -> Handle {
        let base = NEXT_NODE.fetch_add(program.structure.nodes.len() as u64, Ordering::Relaxed);
        let id = self.instances.insert(Instance {
            program,
            input,
            pending: None,
            base,
            revision: 0,
        });
        Handle { bank: self.id, id }
    }

    fn instance(&self, handle: Handle) -> Result<&Instance<P>> {
        if handle.bank != self.id {
            return Err("instance belongs to another bank");
        }
        self.instances.get(handle.id).ok_or("stale instance handle")
    }

    pub fn stage(&mut self, handle: Handle, input: P) -> Result<bool> {
        let instance = self.instance(handle)?;
        let previous = instance
            .pending
            .map_or(&instance.input, |slot| &self.pending[slot].input);
        if instance.program.same_input(previous, &input) {
            return Ok(false);
        }
        let instance = &mut self.instances[handle.id];
        self.prepared = false;
        if let Some(slot) = instance.pending {
            self.pending[slot].input = input;
        } else {
            instance.pending = Some(self.pending.len());
            self.pending.push(Pending {
                id: handle.id,
                input,
                revision: instance.revision,
            });
        }
        Ok(true)
    }

    #[cfg(test)]
    pub fn submit(
        &mut self,
        accept: impl FnOnce(Vec<scene::Change>) -> Result<()>,
    ) -> Result<Work> {
        let mut changes = Edits::default();
        let work = self.prepare(None, &mut changes)?;
        assert!(
            changes.inputs.is_empty(),
            "input bindings require InstanceGraph"
        );
        if !changes.scene.is_empty() {
            accept(changes.scene)?;
        }
        self.accept();
        Ok(work)
    }

    fn prepare(&mut self, active: Option<&FxHashSet<Handle>>, changes: &mut Edits) -> Result<Work> {
        self.prepared = false;
        let mut work = Work::default();
        for pending in &mut self.pending {
            let instance = &self.instances[pending.id];
            let revision = instance
                .revision
                .checked_add(1)
                .ok_or("instance revision exhausted")?;
            let instance_work = if active.is_none_or(|active| {
                active.contains(&Handle {
                    bank: self.id,
                    id: pending.id,
                })
            }) {
                instance.program.diff(
                    &instance.input,
                    &pending.input,
                    instance.base,
                    revision,
                    changes,
                    &mut self.affected,
                )?
            } else {
                instance.program.compare(&instance.input, &pending.input)
            };
            work.sources_compared += instance_work.sources_compared;
            work.bindings_emitted += instance_work.bindings_emitted;
            work.instances_changed += instance_work.instances_changed;
            pending.revision = if instance_work.instances_changed != 0 {
                revision
            } else {
                instance.revision
            };
        }
        self.affected.clear();
        self.prepared = true;
        Ok(work)
    }

    fn accept(&mut self) {
        assert!(self.prepared, "unprepared bank inputs cannot be accepted");
        for pending in self.pending.drain(..) {
            let instance = &mut self.instances[pending.id];
            instance.input = pending.input;
            instance.pending = None;
            instance.revision = pending.revision;
        }
        self.prepared = false;
    }

    pub fn key(&self, handle: Handle, local: Local) -> Result<Key> {
        let instance = self.instance(handle)?;
        if local.0 as usize >= instance.program.structure.nodes.len() {
            return Err("local node outside template");
        }
        Ok(node_key(instance.base, local))
    }

    #[cfg(test)]
    pub fn revision(&self, handle: Handle) -> Result<u64> {
        Ok(self.instance(handle)?.revision)
    }

    #[cfg(test)]
    pub fn element(&self, handle: Handle) -> Result<scene::Element> {
        let instance = self.instance(handle)?;
        instance
            .program
            .mount(&instance.input, instance.base, instance.revision)
    }

    pub fn remove(&mut self, handle: Handle) -> Result<()> {
        if let Some(slot) = self.instance(handle)?.pending {
            self.prepared = false;
            self.pending.swap_remove(slot);
            if let Some(moved) = self.pending.get(slot) {
                self.instances[moved.id].pending = Some(slot);
            }
        }
        self.instances.remove(handle.id);
        self.owner_state.remove(handle.id);
        self.readers.remove(handle.id);
        if self.instances.is_empty() {
            self.pending = Vec::new();
            self.affected = Vec::new();
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    #[cfg(test)]
    pub fn instance_bytes() -> usize {
        std::mem::size_of::<Instance<P>>()
    }
}

impl<P: 'static> Pool for Bank<P> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn key(&self, handle: Handle, local: Local) -> Result<Key> {
        self.key(handle, local)
    }
    fn outlets(&self, handle: Handle) -> Result<&[Local]> {
        Ok(&self.instance(handle)?.program.outlets)
    }
    fn structure(&self, handle: Handle) -> Result<&Structure> {
        Ok(&self.instance(handle)?.program.structure)
    }
    fn extent(&self, handle: Handle) -> Result<std::ops::Range<u64>> {
        let instance = self.instance(handle)?;
        Ok(instance.base..instance.base + instance.program.structure.nodes.len() as u64)
    }
    fn project(
        &self,
        handle: Handle,
        outlets: Vec<(Local, Vec<scene::layout::Subtree>)>,
        proposed: bool,
        out: &mut Vec<scene::layout::Subtree>,
        edits: &mut Edits,
    ) -> Result<()> {
        let instance = self.instance(handle)?;
        let (input, revision) = match (instance.pending, proposed) {
            (Some(index), true) => {
                assert!(self.prepared, "unprepared bank inputs cannot be projected");
                (&self.pending[index].input, self.pending[index].revision)
            }
            _ => (&instance.input, instance.revision),
        };
        instance.program.project(
            input,
            instance.base,
            revision,
            outlets,
            proposed,
            out,
            edits,
        )
    }
    fn prepare(&mut self, active: &FxHashSet<Handle>, changes: &mut Edits) -> Result<Work> {
        self.prepare(Some(active), changes)
    }
    fn changed(&self, active: &FxHashSet<Handle>) -> bool {
        self.pending.iter().any(|pending| {
            let instance = &self.instances[pending.id];
            active.contains(&Handle {
                bank: self.id,
                id: pending.id,
            }) && instance
                .program
                .compare(&instance.input, &pending.input)
                .instances_changed
                != 0
        })
    }
    fn request(&mut self, handle: Handle) -> Option<bool> {
        self.readers.get_mut(handle.id).map(Reader::request)
    }
    fn take_reader(&mut self, handle: Handle) -> Result<Option<Reader>> {
        self.instance(handle)?;
        Ok(self.readers.remove(handle.id))
    }
    fn restore_reader(&mut self, handle: Handle, reader: Reader) {
        if self.instances.contains_key(handle.id) {
            self.readers.insert(handle.id, reader);
        }
    }
    fn accept(&mut self) {
        self.accept();
    }
    fn discard(&mut self) {
        for pending in self.pending.drain(..) {
            self.instances[pending.id].pending = None;
        }
        self.prepared = false;
        self.affected.clear();
    }
    fn remove(&mut self, handle: Handle) -> Result<()> {
        self.remove(handle)
    }
    fn state(&mut self, handle: Handle, initialize: fn() -> Box<dyn Any>) -> Result<&mut dyn Any> {
        self.instance(handle)?;
        if !self.owner_state.contains_key(handle.id) {
            self.owner_state.insert(handle.id, initialize());
        }
        Ok(&mut **self.owner_state.get_mut(handle.id).unwrap())
    }
    fn take_state(&mut self, handle: Handle) -> Result<Option<Box<dyn Any>>> {
        self.instance(handle)?;
        Ok(self.owner_state.remove(handle.id))
    }
    fn restore_state(&mut self, handle: Handle, state: Box<dyn Any>) {
        if self.instances.contains_key(handle.id) {
            self.owner_state.insert(handle.id, state);
        }
    }
}
