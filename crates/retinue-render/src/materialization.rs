use super::coverage::uncovered;
use super::{Key, OptimizationContext, PaintFrame, PaintProgram, PaintStats};
use crate::{
    Rect,
    painter::{Painter, RasterImage},
};
use std::{ops::Range, rc::Rc};

#[derive(Clone)]
pub(super) struct Fragment {
    pub key: Key,
    pub program: PaintProgram,
    pub origin: (i32, i32),
    pub clip: Rect,
}

impl Fragment {
    pub(super) fn bounds(&self) -> Rect {
        self.program.bounds().visible(self.origin, self.clip)
    }

    fn same_placement(&self, other: &Self) -> bool {
        self.key == other.key && self.origin == other.origin && self.clip == other.clip
    }
}

#[derive(Clone)]
pub(super) struct Recipe {
    pub size: (u32, u32),
    pub fragments: Rc<Vec<Fragment>>,
}

impl PartialEq for Recipe {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
            && (Rc::ptr_eq(&self.fragments, &other.fragments)
                || (self.fragments.len() == other.fragments.len()
                    && self
                        .fragments
                        .iter()
                        .zip(other.fragments.iter())
                        .all(|(a, b)| a.same_placement(b) && a.program.id() == b.program.id())))
    }
}

impl Recipe {
    pub fn item(program: &PaintProgram) -> Self {
        let bounds = program.bounds();
        Self {
            size: (bounds.width, bounds.height),
            fragments: Rc::new(vec![Fragment {
                key: 0.into(),
                program: program.clone(),
                origin: (-bounds.x, -bounds.y),
                clip: Rect {
                    width: bounds.width,
                    height: bounds.height,
                    ..Rect::default()
                },
            }]),
        }
    }

    pub fn region(
        frame: &PaintFrame,
        range: Range<usize>,
        origin: (i32, i32),
        size: (u32, u32),
    ) -> Option<Self> {
        let fragments = frame.chunks[range]
            .iter()
            .map(|chunk| {
                let local = (
                    i32::try_from(i64::from(chunk.origin.0) - i64::from(origin.0)).ok()?,
                    i32::try_from(i64::from(chunk.origin.1) - i64::from(origin.1)).ok()?,
                );
                Some(Fragment {
                    key: chunk.key,
                    program: chunk.program.clone(),
                    origin: local,
                    clip: super::coverage::local_clip(chunk.clip, origin, size),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            size,
            fragments: Rc::new(fragments),
        })
    }

    pub fn same_content(&self, other: &Self) -> bool {
        self.size == other.size
            && self.fragments.len() == other.fragments.len()
            && self
                .fragments
                .iter()
                .zip(other.fragments.iter())
                .all(|(a, b)| {
                    a.key == b.key && a.origin == b.origin && a.program.id() == b.program.id()
                })
    }

    pub fn canvas(&self) -> Rect {
        Rect {
            width: self.size.0,
            height: self.size.1,
            ..Rect::default()
        }
    }

    pub fn repair(&self, old: Option<&Self>) -> Repair {
        let Some(old) = old.filter(|old| old.size == self.size) else {
            return Repair {
                regions: vec![self.canvas()],
                copy: false,
            };
        };
        let mut regions = Vec::new();
        for index in 0..self.fragments.len().max(old.fragments.len()) {
            let before = old.fragments.get(index);
            let after = self.fragments.get(index);
            if let (Some(a), Some(b)) = (before, after) {
                if a.key == b.key && a.origin == b.origin {
                    if a.clip != b.clip {
                        for (before, after) in [(a.bounds(), b.bounds()), (b.bounds(), a.bounds())]
                        {
                            for rect in uncovered(before, after) {
                                add_region(&mut regions, rect);
                            }
                        }
                    }
                    if a.program.id() == b.program.id() {
                        continue;
                    }
                    let old_commands = a.program.commands();
                    let commands = b.program.commands();
                    for op in 0..old_commands.len().max(commands.len()) {
                        if a.program.scale() == b.program.scale() {
                            let writes = super::analysis::changed_writes(
                                old_commands.get(op),
                                commands.get(op),
                                b.program.scale(),
                            );
                            for fragment in [a, b] {
                                add_region(
                                    &mut regions,
                                    writes.visible(fragment.origin, fragment.clip),
                                );
                            }
                            continue;
                        }
                        for fragment in [a, b] {
                            if let Some(effect) =
                                fragment.program.analysis().operations_effects.get(op)
                            {
                                add_region(
                                    &mut regions,
                                    effect.writes.visible(fragment.origin, fragment.clip),
                                );
                            }
                        }
                    }
                    continue;
                }
            }
            for fragment in [before, after].into_iter().flatten() {
                add_region(&mut regions, fragment.bounds());
            }
        }
        if regions.len() > 16 {
            regions = vec![self.canvas()];
        }
        let repaired = regions.iter().map(|r| area(*r)).sum::<usize>();
        Repair {
            regions,
            copy: repaired < area(self.canvas()),
        }
    }

    #[cfg(test)]
    pub fn paint_fragment(&self, index: usize, painter: &mut Painter<'_>, repair: Rect) {
        self.paint_fragment_with(
            index,
            painter,
            repair,
            &mut |_, _| None,
            &mut PaintStats::default(),
        );
    }

    pub fn paint_fragment_with(
        &self,
        index: usize,
        painter: &mut Painter<'_>,
        repair: Rect,
        fragments: &mut impl FnMut(&PaintProgram, &OptimizationContext) -> Option<Rc<RasterImage>>,
        stats: &mut PaintStats,
    ) {
        let fragment = &self.fragments[index];
        if fragment.bounds().intersection(repair).is_empty() {
            return;
        }
        painter.viewport(fragment.clip, (0, 0), |painter| {
            for (command, effect) in fragment
                .program
                .commands()
                .iter()
                .zip(&fragment.program.analysis().operations_effects)
            {
                if effect
                    .writes
                    .visible(fragment.origin, fragment.clip)
                    .intersection(repair)
                    .is_empty()
                {
                    continue;
                }
                command.paint(
                    painter,
                    fragment.program.scale(),
                    fragment.origin,
                    &mut |_, _| unreachable!("external paint cannot be materialized"),
                    false,
                    stats,
                    fragments,
                );
            }
        });
    }
}

pub(super) struct Repair {
    pub regions: Vec<Rect>,
    pub copy: bool,
}

pub(super) fn area(rect: Rect) -> usize {
    rect.width as usize * rect.height as usize
}

fn add_region(regions: &mut Vec<Rect>, mut rect: Rect) {
    if rect.is_empty() {
        return;
    }
    // Repair regions must be disjoint when contributors contain translucent paint.
    let mut index = 0;
    while index < regions.len() {
        if rect.intersection(regions[index]).is_empty() {
            index += 1;
        } else {
            rect = rect.union(regions.swap_remove(index));
            index = 0;
        }
    }
    regions.push(rect);
}

#[cfg(test)]
mod tests;
