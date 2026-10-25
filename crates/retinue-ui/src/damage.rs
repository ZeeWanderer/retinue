use crate::Rect;

/// Invalidated buffer pixels. Full repaint remains the fallback for layout changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Damage {
    #[default]
    None,
    Rect(Rect),
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
            (Self::Rect(a), Self::Rect(b)) => Self::rect(a.union(b)),
        }
    }

    pub fn bounds(self, size: (u32, u32)) -> Option<Rect> {
        let output = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let rect = match self {
            Self::None => return None,
            Self::Full => output,
            Self::Rect(rect) => rect.intersection(output),
        };
        (!rect.is_empty()).then_some(rect)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repaint {
    pub repair: Rect,
    pub damage: Rect,
}

/// Backends reporting buffer age need the damage since that buffer was presented.
pub struct AgeRepaintTracker {
    pending: Damage,
    history: std::collections::VecDeque<Damage>,
}

impl Default for AgeRepaintTracker {
    fn default() -> Self {
        Self {
            pending: Damage::Full,
            history: Default::default(),
        }
    }
}

impl AgeRepaintTracker {
    pub fn invalidate(&mut self, damage: Damage) {
        self.pending = self.pending.union(damage);
    }

    pub fn is_dirty(&self) -> bool {
        self.pending != Damage::None
    }

    pub fn reset(&mut self) {
        self.pending = Damage::Full;
        self.history.clear();
    }

    pub fn plan(&self, age: u8, size: (u32, u32)) -> Option<Repaint> {
        let damage = self.pending.bounds(size)?;
        let repair = if age == 0 || usize::from(age - 1) > self.history.len() {
            Damage::Full
        } else {
            self.history
                .iter()
                .take(usize::from(age - 1))
                .fold(self.pending, |damage, old| damage.union(*old))
        };
        Some(Repaint {
            damage,
            repair: repair.bounds(size).unwrap_or(damage),
        })
    }

    pub fn submitted(&mut self) {
        self.history.push_front(self.pending);
        self.history.truncate(8);
        self.pending = Damage::None;
    }
}

/// Tracks repairs separately from compositor damage, including unavailable buffers.
pub struct RepaintTracker {
    pending: Damage,
    buffers: Vec<Damage>,
}

impl Default for RepaintTracker {
    fn default() -> Self {
        Self {
            pending: Damage::Full,
            buffers: Vec::new(),
        }
    }
}

impl RepaintTracker {
    pub fn invalidate(&mut self, damage: Damage) {
        if damage == Damage::None {
            return;
        }
        self.pending = self.pending.union(damage);
        for buffer in &mut self.buffers {
            *buffer = buffer.union(damage);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.pending != Damage::None
    }

    pub fn add_buffer(&mut self) {
        self.buffers.push(Damage::Full);
    }

    /// Call when discarding the host's buffers, including resize and remap.
    pub fn reset(&mut self) {
        self.pending = Damage::Full;
        self.buffers.clear();
    }

    pub fn plan(&mut self, index: usize, size: (u32, u32)) -> Option<Repaint> {
        let Some(damage) = self.pending.bounds(size) else {
            self.pending = Damage::None;
            return None;
        };
        Some(Repaint {
            repair: self.buffers[index].bounds(size).unwrap_or(damage),
            damage,
        })
    }

    /// Complete after paint and submission, before processing another invalidation.
    pub fn submitted(&mut self, index: usize) {
        self.buffers[index] = Damage::None;
        self.pending = Damage::None;
    }

    pub fn failed(&mut self, index: usize) {
        self.buffers[index] = Damage::Full;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (u32, u32) = (100, 60);
    const A: Rect = Rect {
        x: 10,
        y: 20,
        width: 5,
        height: 7,
    };
    const B: Rect = Rect { x: 30, ..A };
    const C: Rect = Rect { y: 40, ..A };

    #[test]
    fn aged_buffers_repair_only_changes_since_their_last_frame() {
        let mut tracker = AgeRepaintTracker::default();
        tracker.submitted();
        tracker.invalidate(Damage::rect(A));
        assert_eq!(
            tracker.plan(1, SIZE).unwrap(),
            Repaint {
                damage: A,
                repair: A
            }
        );
        tracker.submitted();
        tracker.invalidate(Damage::rect(B));
        assert_eq!(
            tracker.plan(2, SIZE).unwrap(),
            Repaint {
                damage: B,
                repair: A.union(B)
            }
        );
        for age in [0, 10, 255] {
            assert_eq!(
                tracker.plan(age, SIZE).unwrap().repair,
                Damage::Full.bounds(SIZE).unwrap()
            );
        }
        tracker.reset();
        assert_eq!(
            tracker.plan(1, SIZE).unwrap().damage,
            Damage::Full.bounds(SIZE).unwrap()
        );
        tracker.submitted();
        assert!(!tracker.is_dirty());
        assert_eq!(tracker.plan(0, SIZE), None);
    }

    fn prepared(count: usize) -> RepaintTracker {
        let mut tracker = RepaintTracker::default();
        for index in 0..count {
            tracker.add_buffer();
            tracker.submitted(index);
        }
        tracker
    }

    #[test]
    fn repair_includes_changes_while_buffer_was_unavailable() {
        let mut tracker = prepared(3);
        tracker.invalidate(Damage::rect(A));
        assert_eq!(
            tracker.plan(0, SIZE),
            Some(Repaint {
                repair: A,
                damage: A
            })
        );
        tracker.submitted(0);
        tracker.invalidate(Damage::rect(B));
        assert_eq!(
            tracker.plan(1, SIZE),
            Some(Repaint {
                repair: A.union(B),
                damage: B
            })
        );
        tracker.submitted(1);
        tracker.invalidate(Damage::rect(C));
        assert_eq!(
            tracker.plan(2, SIZE),
            Some(Repaint {
                repair: A.union(B).union(C),
                damage: C
            })
        );
        assert_eq!(
            tracker.plan(0, SIZE),
            Some(Repaint {
                repair: B.union(C),
                damage: C
            })
        );
        tracker.submitted(2);
        assert!(!tracker.is_dirty());
    }

    #[test]
    fn new_buffer_needs_full_repair_but_not_full_compositor_damage() {
        let mut tracker = prepared(1);
        tracker.invalidate(Damage::rect(A));
        tracker.add_buffer();
        let plan = tracker.plan(1, SIZE).unwrap();
        assert_eq!(plan.repair, Damage::Full.bounds(SIZE).unwrap());
        assert_eq!(plan.damage, A);
    }

    #[test]
    fn coalesces_changes_without_a_free_buffer() {
        let mut tracker = prepared(3);
        tracker.invalidate(Damage::rect(A));
        tracker.invalidate(Damage::rect(B));
        let expected = Some(Repaint {
            repair: A.union(B),
            damage: A.union(B),
        });
        assert_eq!(tracker.plan(0, SIZE), expected);
        assert_eq!(tracker.plan(0, SIZE), expected);
        assert!(tracker.is_dirty());
    }

    #[test]
    fn failed_paint_preserves_damage_and_requires_full_repair() {
        let mut tracker = prepared(1);
        tracker.invalidate(Damage::rect(A));
        tracker.plan(0, SIZE).unwrap();
        tracker.failed(0);
        assert!(tracker.is_dirty());
        assert_eq!(
            tracker.plan(0, SIZE).unwrap(),
            Repaint {
                repair: Damage::Full.bounds(SIZE).unwrap(),
                damage: A,
            }
        );
    }

    #[test]
    fn resize_or_remap_resets_buffer_validity() {
        let mut tracker = prepared(3);
        tracker.reset();
        tracker.add_buffer();
        let full = Damage::Full.bounds((200, 120)).unwrap();
        assert_eq!(
            tracker.plan(0, (200, 120)),
            Some(Repaint {
                repair: full,
                damage: full
            })
        );
    }

    #[test]
    fn clips_damage_and_consumes_offscreen_changes() {
        let mut tracker = prepared(1);
        tracker.invalidate(Damage::rect(Rect {
            x: 95,
            y: 55,
            width: 10,
            height: 10,
        }));
        let rect = Rect {
            x: 95,
            y: 55,
            width: 5,
            height: 5,
        };
        assert_eq!(
            tracker.plan(0, SIZE),
            Some(Repaint {
                repair: rect,
                damage: rect
            })
        );
        tracker.submitted(0);
        tracker.invalidate(Damage::rect(Rect { x: u32::MAX, ..A }));
        assert_eq!(tracker.plan(0, SIZE), None);
        assert!(!tracker.is_dirty());
        assert_eq!(Damage::rect(Rect { width: 0, ..A }), Damage::None);
    }

    #[test]
    fn margin_saturates_at_output_origin_and_integer_limit() {
        let rect = Rect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        assert_eq!(
            rect.expanded(3),
            Rect {
                x: 0,
                y: 0,
                width: 6,
                height: 6
            }
        );
        let far = Rect {
            x: u32::MAX - 1,
            ..rect
        }
        .expanded(3);
        assert_eq!(far.x + far.width, u32::MAX);
    }
}
