use super::{Key, PaintFrame, PaintProgram, ProgramId, ResourceRead};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    rc::Rc,
};

#[derive(Default)]
pub(super) struct Dependencies {
    frame: Option<(Rc<()>, u64)>,
    programs: Vec<Instance>,
    resources: HashMap<ResourceRead, HashSet<Key>>,
    pub changed: Vec<usize>,
    pub groups: Vec<usize>,
    pub fragment_roots: BTreeSet<usize>,
    pub visited: usize,
    pub paints: usize,
    pub full: bool,
}

struct Instance {
    key: Key,
    program: ProgramId,
    resources: Vec<ResourceRead>,
}

impl Instance {
    fn new(key: Key, program: &PaintProgram) -> Self {
        Self {
            key,
            program: program.id(),
            resources: program.analysis().resources.clone(),
        }
    }
}

impl Dependencies {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, frame: &PaintFrame) {
        self.changed.clear();
        self.groups.clear();
        self.visited = 0;
        self.paints = 0;
        self.full = false;
        let previous = self
            .frame
            .as_ref()
            .filter(|(id, _)| Rc::ptr_eq(id, &frame.identity))
            .map(|(_, revision)| *revision);
        if previous == Some(frame.generation) {
            return;
        }
        let changes = previous.and_then(|generation| frame.changes_since(generation));
        let full = changes.as_ref().is_none_or(|changes| changes.topology)
            || self.programs.len() != frame.chunks.len();
        self.full = full;
        if full {
            self.programs.clear();
            self.resources.clear();
            self.fragment_roots.clear();
            for (index, instance) in frame.chunks.iter().enumerate() {
                if !instance.program.calls().is_empty() {
                    self.fragment_roots.insert(index);
                }
                self.add(instance.key, &instance.program);
                self.programs
                    .push(Instance::new(instance.key, &instance.program));
                self.changed.push(index);
            }
            self.paints = frame.chunks.len();
        } else {
            let changes = changes.as_ref().unwrap();
            self.groups.extend(&changes.groups);
            self.changed.extend(&changes.instances);
            self.changed.sort_unstable();
            self.changed.dedup();
            for n in 0..self.changed.len() {
                let index = self.changed[n];
                let instance = &frame.chunks[index];
                if self.programs[index].program != instance.program.id() {
                    if instance.program.calls().is_empty() {
                        self.fragment_roots.remove(&index);
                    } else {
                        self.fragment_roots.insert(index);
                    }
                    let old = &self.programs[index];
                    for resource in &old.resources {
                        if let Some(consumers) = self.resources.get_mut(resource) {
                            consumers.remove(&old.key);
                            if consumers.is_empty() {
                                self.resources.remove(resource);
                            }
                        }
                    }
                    self.add(instance.key, &instance.program);
                    self.programs[index] = Instance::new(instance.key, &instance.program);
                    self.paints += 1;
                }
            }
        }
        self.visited = self.changed.len();
        self.frame = Some((frame.identity.clone(), frame.generation));
    }

    fn add(&mut self, key: Key, program: &PaintProgram) {
        for resource in &program.analysis().resources {
            self.resources.entry(*resource).or_default().insert(key);
        }
    }

    pub fn consumers(&self, resource: ResourceRead) -> impl Iterator<Item = Key> + '_ {
        self.resources
            .get(&resource)
            .into_iter()
            .flat_map(|keys| keys.iter().copied())
    }
}
