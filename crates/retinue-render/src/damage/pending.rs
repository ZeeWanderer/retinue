use super::{Damage, Region};
use std::sync::atomic::{AtomicU64, Ordering};

/// Identifies the damage cutoff and target of one planned submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    id: u64,
    pub(super) slot: Option<usize>,
}

struct Flight {
    ticket: Ticket,
    damage: Damage,
}

pub(super) struct Pending {
    pub queued: Damage,
    flight: Option<Flight>,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            queued: Damage::Full,
            flight: None,
        }
    }
}

impl Pending {
    pub fn invalidate(&mut self, damage: Damage) {
        self.queued = std::mem::take(&mut self.queued).union(damage);
    }

    pub fn is_dirty(&self) -> bool {
        self.flight.is_some() || self.queued != Damage::None
    }

    pub fn reset(&mut self) {
        self.queued = Damage::Full;
        self.flight = None;
    }

    pub fn begin(&mut self, slot: Option<usize>, size: (u32, u32)) -> Option<(Ticket, Region)> {
        // A replacement plan must include the unacknowledged older frame too.
        if let Some(old) = self.flight.take() {
            self.invalidate(old.damage);
        }
        let pending = std::mem::take(&mut self.queued);
        let damage = pending.regions(size);
        if damage.is_empty() {
            return None;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, 0, "damage ticket space exhausted");
        let ticket = Ticket { id, slot };
        self.flight = Some(Flight {
            ticket,
            damage: pending,
        });
        Some((ticket, damage))
    }

    pub fn current(&self, ticket: Ticket) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|flight| flight.ticket == ticket)
    }

    pub fn acknowledge(&mut self, ticket: Ticket) -> Option<Damage> {
        self.current(ticket)
            .then(|| self.flight.take().unwrap().damage)
    }
}
