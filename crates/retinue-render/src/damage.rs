use crate::Rect;

mod region;
pub use region::Region;
mod pending;
use pending::Pending;
pub use pending::Ticket;

/// Invalidated buffer pixels. Full repaint remains the fallback for layout changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Damage {
    #[default]
    None,
    Rect(Rect),
    Region(Region),
    Full,
}

impl Damage {
    pub fn rect(rect: Rect) -> Self {
        if rect.is_empty() {
            Self::None
        } else {
            Self::Rect(rect)
        }
    }

    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::None, damage) | (damage, Self::None) => damage,
            (Self::Rect(a), Self::Rect(b)) => {
                let bounds = a.union(b);
                if region::area(bounds) as u128
                    == region::area(a) as u128 + region::area(b) as u128
                        - region::area(a.intersection(b)) as u128
                {
                    Self::rect(bounds)
                } else {
                    Self::region(Region::new([a, b]))
                }
            }
            (Self::Region(a), Self::Region(b)) => Self::region(a.union(&b)),
            (Self::Region(a), Self::Rect(b)) | (Self::Rect(b), Self::Region(a)) => {
                Self::region(a.union_rect(b))
            }
        }
    }

    pub fn region(region: Region) -> Self {
        match region.rects() {
            [] => Self::None,
            [rect] => Self::Rect(*rect),
            _ => Self::Region(region),
        }
    }

    pub fn regions(&self, size: (u32, u32)) -> Region {
        let output = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        match self {
            Self::None => Region::default(),
            Self::Full => output.into(),
            Self::Rect(rect) => rect.intersection(output).into(),
            Self::Region(region) => region.intersection(output),
        }
    }

    pub fn bounds(&self, size: (u32, u32)) -> Option<Rect> {
        let output = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let bounds = match self {
            Self::None => return None,
            Self::Full => output,
            Self::Rect(rect) => rect.intersection(output),
            Self::Region(region) => region.bounds().intersection(output),
        };
        (!bounds.is_empty()).then_some(bounds)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repaint {
    pub repair: Region,
    pub damage: Region,
    pub ticket: Ticket,
}

/// Backends reporting buffer age need the damage since that buffer was presented.
pub struct AgeRepaintTracker {
    pending: Pending,
    history: std::collections::VecDeque<Damage>,
}

impl Default for AgeRepaintTracker {
    fn default() -> Self {
        Self {
            pending: Pending::default(),
            history: Default::default(),
        }
    }
}

impl AgeRepaintTracker {
    pub fn invalidate(&mut self, damage: Damage) {
        self.pending.invalidate(damage);
    }

    pub fn is_dirty(&self) -> bool {
        self.pending.is_dirty()
    }

    pub fn reset(&mut self) {
        self.pending.reset();
        self.history.clear();
    }

    pub fn plan(&mut self, age: u8, size: (u32, u32)) -> Option<Repaint> {
        let (ticket, damage) = self.pending.begin(None, size)?;
        let repair = if age == 0 || usize::from(age - 1) > self.history.len() {
            Damage::Full
        } else {
            self.history
                .iter()
                .take(usize::from(age - 1))
                .fold(Damage::region(damage.clone()), |damage, old| {
                    damage.union(old.clone())
                })
        };
        Some(Repaint {
            repair: repair.regions(size),
            damage,
            ticket,
        })
    }

    pub fn current(&self, ticket: Ticket) -> bool {
        self.pending.current(ticket)
    }

    pub fn submitted(&mut self, ticket: Ticket) -> bool {
        let Some(damage) = self.pending.acknowledge(ticket) else {
            return false;
        };
        self.history.push_front(damage);
        self.history.truncate(8);
        true
    }
}

/// Tracks repairs separately from compositor damage, including unavailable buffers.
pub struct RepaintTracker {
    pending: Pending,
    buffers: Vec<Damage>,
}

impl Default for RepaintTracker {
    fn default() -> Self {
        Self {
            pending: Pending::default(),
            buffers: Vec::new(),
        }
    }
}

impl RepaintTracker {
    pub fn invalidate(&mut self, damage: Damage) {
        if damage == Damage::None {
            return;
        }
        self.pending.invalidate(damage.clone());
        for buffer in &mut self.buffers {
            *buffer = std::mem::take(buffer).union(damage.clone());
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.pending.is_dirty()
    }

    pub fn add_buffer(&mut self) {
        self.buffers.push(Damage::Full);
    }

    /// Call when discarding the host's buffers, including resize and remap.
    pub fn reset(&mut self) {
        self.pending.reset();
        self.buffers.clear();
    }

    pub fn plan(&mut self, index: usize, size: (u32, u32)) -> Option<Repaint> {
        let (ticket, damage) = self.pending.begin(Some(index), size)?;
        Some(Repaint {
            repair: self.buffers[index].regions(size),
            damage,
            ticket,
        })
    }

    pub fn current(&self, index: usize, ticket: Ticket) -> bool {
        ticket.slot == Some(index) && self.pending.current(ticket)
    }

    /// Acknowledge only this plan; newer invalidations also remain on its target.
    pub fn submitted(&mut self, index: usize, ticket: Ticket) -> bool {
        if !self.current(index, ticket) {
            return false;
        }
        self.pending.acknowledge(ticket);
        self.buffers[index] = self.pending.queued.clone();
        true
    }

    pub fn failed(&mut self, index: usize) {
        self.buffers[index] = Damage::Full;
    }
}

#[cfg(test)]
mod tests;
