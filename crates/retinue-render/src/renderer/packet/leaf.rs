use super::*;
use crate::renderer::evidence::{Feedback, Publication};

#[derive(Default)]
pub(in crate::renderer) struct LeafPackets {
    pub(super) last: Option<Arc<DrawPacket>>,
    pub(super) sweep: Option<Rect>,
    probe: Option<Region>,
    repeats: u8,
    backoff: u8,
    hits: u8,
    #[cfg(any(test, feature = "benchmark"))]
    pub(in crate::renderer) forced: bool,
    #[cfg(feature = "benchmark")]
    pub(in crate::renderer) counts: [usize; 3],
    owner: Option<Arc<Publication>>,
}

pub(super) enum Choice {
    Live,
    Retain { alternate: Option<Region> },
    Hit(Arc<DrawPacket>),
}

impl LeafPackets {
    pub(super) fn choose(
        &mut self,
        repair: &Region,
        fonts: &Arc<crate::text::preparation::PreparedFonts>,
        has_external: impl FnOnce() -> bool,
    ) -> Choice {
        if let Some(packet) = &self.last
            && self.covers(packet.repair.as_ref().unwrap(), repair)
            && Arc::ptr_eq(&packet.fonts, fonts)
        {
            self.hits = self.hits.saturating_add(1);
            #[cfg(feature = "benchmark")]
            {
                self.counts[0] += 1;
            }
            return Choice::Hit(packet.clone());
        }
        self.retire();
        let prior = self.probe.replace(repair.clone());
        let sweep = prior.as_ref().and_then(|r| Self::sweep(r, repair));
        let compatible = sweep.is_some() || prior.as_ref() == Some(repair);
        // Two compatible repeats in one publication: the first establishes a
        // potential hit, the second shows it wasn't just a one-off.
        self.repeats = if compatible {
            self.repeats.saturating_add(1)
        } else {
            0
        };
        let admit = self.repeats >= (2 << self.backoff);
        #[cfg(any(test, feature = "benchmark"))]
        let admit = admit || self.forced;
        if !admit {
            self.sweep = None;
            return Choice::Live;
        }
        self.sweep = sweep.filter(|_| !has_external());
        Choice::Retain {
            alternate: self.sweep.and(prior),
        }
    }

    fn retire(&mut self) {
        if self.last.take().is_some() {
            // Validity dies with this packet; usefulness survives it. Bound the
            // retry delay so a view that settles can recover within eight repeats.
            match self.hits {
                0 => {
                    self.repeats = 0;
                    self.backoff = (self.backoff + 1).min(2);
                }
                2.. => self.backoff = self.backoff.saturating_sub(1),
                _ => {}
            }
            self.hits = 0;
        }
    }

    fn covers(&self, previous: &Region, requested: &Region) -> bool {
        previous == requested
            || matches!((self.sweep, previous.rects(), requested.rects()),
            (Some(sweep), [a], [b]) if (a.width, a.height) == (b.width, b.height)
                && sweep.intersection(*b) == *b)
    }

    pub(super) fn sweep(previous: &Region, requested: &Region) -> Option<Rect> {
        let ([a], [b]) = (previous.rects(), requested.rects()) else {
            return None;
        };
        if a == b
            || (a.width, a.height) != (b.width, b.height)
            || (a.x != b.x && a.y != b.y)
            || a.intersection(*b).is_empty()
        {
            return None;
        }
        let next = |p, q, extent| {
            (2 * i64::from(q) - i64::from(p)).clamp(0, i64::from(u32::MAX - extent)) as u32
        };
        Some(a.union(*b).union(Rect {
            x: next(a.x, b.x, b.width),
            y: next(a.y, b.y, b.height),
            ..*b
        }))
    }

    pub(super) fn feedback(
        &mut self,
        layers: &Layers,
        repair: &Region,
        alternate: Option<Region>,
    ) -> Arc<Feedback> {
        #[cfg(feature = "benchmark")]
        {
            self.counts[1] += 1;
        }
        self.owner
            .get_or_insert_with(|| Publication::new(layers))
            .feedback(repair, alternate)
    }

    pub(in crate::renderer) fn invalidate(&mut self) {
        // A late native completion can only credit the publication it painted.
        #[cfg(feature = "benchmark")]
        {
            self.counts[2] += usize::from(self.last.is_some());
        }
        self.retire();
        self.sweep = None;
        self.probe = None;
        self.repeats = 0;
        self.owner = None;
    }

    pub(in crate::renderer) fn reset(&mut self) {
        self.invalidate();
        self.backoff = 0;
    }

    pub(in crate::renderer) fn apply(
        &mut self,
        fragments: &mut Fragments,
        layers: &mut Layers,
        residency: &mut Residency,
    ) {
        if let Some(owner) = &self.owner {
            owner.apply(fragments, layers, residency);
        }
    }
}
