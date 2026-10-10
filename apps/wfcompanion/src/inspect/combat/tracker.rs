use serde::Serialize;
use serde_json::{Value, json};

use super::{MODULUS, Record, SLOTS, Snapshot};

#[derive(Default, Serialize)]
pub(super) struct Counts {
    pub displays: usize,
    pub revisions: usize,
    pub gaps: usize,
}

struct Pending {
    sequence: u16,
    ordinal: u64,
    changed: bool,
}

pub(super) struct Tracker {
    observed: Snapshot,
    emitted: Vec<Option<(u64, Record)>>,
    pending: Vec<Pending>,
    ordinal: u64,
    pub counts: Counts,
}

impl Tracker {
    pub fn new(baseline: Snapshot) -> Self {
        Self {
            emitted: vec![None; baseline.records.len()],
            observed: baseline,
            pending: Vec::new(),
            ordinal: 0,
            counts: Counts::default(),
        }
    }

    pub fn pending(&self) -> Vec<Value> {
        self.pending
            .iter()
            .map(|v| json!({"sequence": v.sequence, "ordinal": v.ordinal}))
            .collect()
    }

    pub fn update(&mut self, current: Snapshot) -> Vec<Value> {
        let delta = (current.counter + MODULUS - self.observed.counter) % MODULUS;
        for index in 0..delta {
            self.pending.push(Pending {
                sequence: (self.observed.counter + index) % MODULUS,
                ordinal: self.ordinal + u64::from(index),
                changed: false,
            });
        }
        self.ordinal += u64::from(delta);
        let mut events = Vec::new();
        let mut pending = Vec::new();
        for mut entry in self.pending.drain(..) {
            if self.ordinal - entry.ordinal >= u64::from(MODULUS) {
                self.counts.gaps += 1;
                events.push(json!({"kind": "gap", "sequence": entry.sequence,
                    "ordinal": entry.ordinal, "reason": "sequence generation expired before a stable read"}));
                continue;
            }
            let mut matches = current
                .records
                .iter()
                .filter(|v| v.sequence == entry.sequence && !v.text.is_empty());
            let first = matches.next();
            if first.is_none() || matches.next().is_some() {
                // The counter is published before its record; don't call that first sample lost.
                if self.ordinal - entry.ordinal <= SLOTS as u64 {
                    pending.push(entry);
                } else {
                    self.counts.gaps += 1;
                    events.push(json!({"kind": "gap", "sequence": entry.sequence,
                        "ordinal": entry.ordinal, "reason": "no unique record within the display pool window"}));
                }
                continue;
            }
            let record = first.unwrap();
            if !record.same_value(&self.observed.records[record.slot - 1]) {
                entry.changed = true;
                pending.push(entry);
                continue;
            }
            if !entry.changed {
                if self.ordinal - entry.ordinal <= SLOTS as u64 {
                    pending.push(entry);
                } else {
                    self.counts.gaps += 1;
                    events.push(json!({"kind": "gap", "sequence": entry.sequence,
                        "ordinal": entry.ordinal, "reason": "record freshness could not be established"}));
                }
                continue;
            }
            events.push(json!({"kind": "display", "ordinal": entry.ordinal, "record": record}));
            self.emitted[record.slot - 1] = Some((entry.ordinal, record.clone()));
            self.counts.displays += 1;
        }
        for record in &current.records {
            if pending.iter().any(|v| v.sequence == record.sequence) {
                continue;
            }
            let Some((ordinal, previous)) = &mut self.emitted[record.slot - 1] else {
                continue;
            };
            if record.sequence == previous.sequence
                && record.address == previous.address
                && !record.same_value(previous)
                && record.same_value(&self.observed.records[record.slot - 1])
            {
                events.push(json!({"kind": "record_update", "ordinal": ordinal, "record": record}));
                *previous = record.clone();
                self.counts.revisions += 1;
            }
        }
        self.pending = pending;
        self.observed = current;
        events
    }
}
