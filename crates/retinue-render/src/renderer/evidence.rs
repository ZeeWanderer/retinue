//! Publication-scoped evidence from completed native painting.
use super::{Fragments, Layers, RasterId, Rect, Residency, ScopeId};
use crate::{ProgramId, damage::Region};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(super) enum Use {
    Replay(ProgramId),
    FragmentBlit(ProgramId),
    ItemBlit(RasterId),
    Layer(ScopeId),
}

// One count per subject in the current realization, not a queue of rendered frames.
pub(super) struct Publication {
    layers: Vec<(ScopeId, Rect)>,
    counts: Mutex<FxHashMap<Use, u32>>,
}

pub(super) struct Feedback {
    owner: Arc<Publication>,
    repair: Region,
    exact: OnceLock<Vec<Use>>,
    alternate: Option<(Region, OnceLock<Vec<Use>>)>,
}

#[derive(Clone, Copy)]
enum Scope {
    Current,
    Previous,
}

pub(crate) struct PaintReceipt {
    feedback: Arc<Feedback>,
    repair: Region,
    slot: Option<Scope>,
    pub(super) recorded: Option<Vec<Use>>,
}

impl PaintReceipt {
    pub(super) fn new(feedback: &Arc<Feedback>, repair: Region) -> Self {
        let slot = if repair == feedback.repair {
            Some(Scope::Current)
        } else {
            feedback
                .alternate
                .as_ref()
                .filter(|(r, _)| *r == repair)
                .map(|_| Scope::Previous)
        };
        Self {
            feedback: feedback.clone(),
            repair,
            slot,
            recorded: feedback
                .slot(slot)
                .and_then(OnceLock::get)
                .is_none()
                .then(Vec::new),
        }
    }

    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.feedback.owner, &other.feedback.owner)
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.repair = self.repair.union(&other.repair);
        let uses = self.recorded.get_or_insert_with(|| {
            self.feedback
                .slot(self.slot)
                .unwrap()
                .get()
                .unwrap()
                .clone()
        });
        self.slot = None;
        if let Some(mut extra) = other.recorded {
            uses.append(&mut extra);
        } else {
            uses.extend_from_slice(other.feedback.slot(other.slot).unwrap().get().unwrap());
        }
    }

    pub(crate) fn complete(mut self) {
        let owner = &self.feedback.owner;
        if let Some(uses) = &mut self.recorded {
            uses.extend(
                owner
                    .layers
                    .iter()
                    .filter(|(_, bounds)| Layers::counts_as_drawn(*bounds, &self.repair))
                    .map(|(key, _)| Use::Layer(*key)),
            );
            uses.sort_unstable();
            uses.dedup();
        }
        let uses = if let Some(slot) = self.feedback.slot(self.slot) {
            slot.get_or_init(|| self.recorded.take().unwrap())
        } else {
            self.recorded.as_ref().unwrap()
        };
        if uses.is_empty() {
            return;
        }
        let mut counts = owner.counts.lock().unwrap();
        for &item in uses {
            let n = counts.entry(item).or_default();
            *n = n.saturating_add(1);
        }
    }
}

impl Feedback {
    fn slot(&self, slot: Option<Scope>) -> Option<&OnceLock<Vec<Use>>> {
        match slot? {
            Scope::Current => Some(&self.exact),
            Scope::Previous => self.alternate.as_ref().map(|pair| &pair.1),
        }
    }
}

impl Publication {
    pub(super) fn new(layers: &Layers) -> Arc<Self> {
        Arc::new(Self {
            layers: layers.draw_candidates(),
            counts: Default::default(),
        })
    }

    pub(super) fn feedback(
        self: &Arc<Self>,
        repair: &Region,
        alternate: Option<Region>,
    ) -> Arc<Feedback> {
        Arc::new(Feedback {
            owner: self.clone(),
            repair: repair.clone(),
            exact: Default::default(),
            alternate: alternate.map(|r| (r, Default::default())),
        })
    }

    pub(super) fn apply(
        &self,
        fragments: &mut Fragments,
        layers: &mut Layers,
        residency: &mut Residency,
    ) {
        for (item, n) in self.counts.lock().unwrap().drain() {
            match item {
                Use::Replay(id) => fragments.completed(id, n),
                Use::FragmentBlit(id) => {
                    if let Some(&raster) = fragments.selected.get(&id) {
                        residency.drawn(raster);
                    }
                }
                Use::ItemBlit(id) => residency.drawn(id),
                Use::Layer(id) => layers.completed(id, n, residency),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_completions_credit_their_publication_once_per_subject() {
        let published = || {
            Arc::new(Publication {
                layers: Vec::new(),
                counts: Default::default(),
            })
        };
        let old = published();
        let current = published();
        let area = Region::from(Rect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        });
        let feedback = old.feedback(&area, None);
        let subject = Use::Layer(ScopeId::group(1.into()));
        let record = || {
            let mut r = PaintReceipt::new(&feedback, area.clone());
            if let Some(recorded) = &mut r.recorded {
                recorded.extend([subject, subject]);
            }
            r
        };
        // All three workers can start against the same uncached observation.
        let started = [record(), record(), record()];
        std::thread::scope(|scope| {
            for receipt in started {
                scope.spawn(move || receipt.complete());
            }
            let mut new_receipt = PaintReceipt::new(&current.feedback(&area, None), area.clone());
            new_receipt.recorded.as_mut().unwrap().push(subject);
            new_receipt.complete();
        });
        drop(record()); // Abandoned preparation earns nothing.
        let cached = record();
        assert!(cached.recorded.is_none());
        cached.complete();
        assert_eq!(old.counts.lock().unwrap().get(&subject), Some(&4));
        assert_eq!(current.counts.lock().unwrap().get(&subject), Some(&1));
    }
}
