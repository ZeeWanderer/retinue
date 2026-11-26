//! Benchmark-only publication alternatives. The published chunk array already
//! owns order and placement; neither path builds another indexed draw plan.
use super::*;
use crate::{PaintInstance, PaintProgram};
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Policy {
    Dense,
    Edits,
    Fragments,
    Items,
}

fn selected() -> Option<Policy> {
    static POLICY: OnceLock<Option<Policy>> = OnceLock::new();
    *POLICY.get_or_init(
        || match std::env::var("RETINUE_BENCH_PUBLICATION").as_deref() {
            Err(_) | Ok("current") => None,
            Ok("dense") => Some(Policy::Dense),
            Ok("edits") => Some(Policy::Edits),
            Ok("edits-fragments") => Some(Policy::Fragments),
            Ok("edits-items") => Some(Policy::Items),
            Ok(other) => panic!("unknown publication trial: {other}"),
        },
    )
}

#[derive(Default)]
pub(super) struct Trial {
    items: FxHashMap<Key, (crate::ProgramId, RasterId, u64)>,
    waiting: Vec<Rect>,
    blocked: bool,
}

impl Trial {
    pub fn active() -> bool {
        selected().is_some()
    }

    pub fn fragments() -> bool {
        matches!(selected(), Some(Policy::Fragments | Policy::Items))
    }

    pub(super) fn bind(&mut self, chunk: &PaintInstance, id: RasterId, generation: u64) {
        self.items
            .insert(chunk.key, (chunk.program.id(), id, generation));
    }

    pub(super) fn item(&self, chunk: &PaintInstance) -> Option<RasterId> {
        if chunk.cache != CachePolicy::Prefer {
            return None;
        }
        self.items
            .get(&chunk.key)
            .filter(|(program, _, _)| *program == chunk.program.id())
            .map(|(_, id, _)| *id)
    }
}

pub(super) enum Lookup<'a> {
    Item(&'a PaintInstance),
    Fragment(&'a PaintProgram, &'a crate::OptimizationContext),
}

pub(super) fn draw(
    chunks: &[PaintInstance],
    painter: &mut Painter<'_>,
    stats: &mut PaintStats,
    external: &mut impl FnMut(Key, &mut Painter<'_>),
    lookup: &mut impl FnMut(Lookup<'_>) -> Option<Arc<crate::painter::RasterImage>>,
) {
    let sparse = painter.has_multiple_clips();
    let repair = painter.visible_bounds();
    for c in chunks {
        if !painter.intersects(c.visible) {
            continue;
        }
        let cull = sparse || c.visible.intersection(repair) != c.visible;
        painter.viewport(c.clip, (0, 0), |p| {
            if c.cache == CachePolicy::Prefer
                && let Some(image) = lookup(Lookup::Item(c))
            {
                let b = c.program.bounds();
                let (w, h) = image.dimensions();
                p.translated((c.origin.0 + b.x, c.origin.1 + b.y), |p| {
                    p.draw_image(&image, 0, 0, w, h)
                });
                stats.blits += 1;
            } else {
                c.program
                    .replay(p, c.origin, external, true, cull, stats, &mut |p, ctx| {
                        lookup(Lookup::Fragment(p, ctx))
                    });
            }
        });
    }
}

fn roots<'a>(
    chunks: &'a [PaintInstance],
    trial: &'a Trial,
    mode: RasterMode,
) -> impl Iterator<Item = (&'a PaintProgram, (i32, i32), Rect)> {
    chunks
        .iter()
        .filter(|c| !c.visible.is_empty() && !c.program.calls().is_empty())
        .filter(move |c| mode != RasterMode::Explicit || trial.item(c).is_none())
        .map(|c| (&c.program, c.origin, c.clip))
}

impl Renderer {
    pub(super) fn plan_publication(
        &mut self,
        frame: &PaintFrame,
        mode: RasterMode,
        demand: PaintDemand<'_>,
        now: Instant,
    ) -> Option<Pending> {
        let generation = Some((frame.identity, frame.generation));
        if self.prepared_for != generation || self.text_fonts.has_failures() {
            let exact = self.prepared_for == Some((frame.identity, frame.changes.from));
            let changes = (selected() != Some(Policy::Dense) && exact).then_some(&frame.changes);
            self.dependencies
                .content
                .update(frame, changes, &mut self.text_fonts, &mut self.stats);
            self.stats.inspected_instances = self.stats.content_visits;
        }
        if Trial::fragments() && self.materializations {
            self.fragments.begin_frame();
            self.residency.maintain(now);
            let reclaimed = self.residency.reclaimed();
            let retry = self.publication.blocked && self.reclaimed != reclaimed;
            self.reclaimed = reclaimed;
            let mode_changed = self.mode != mode;
            self.mode = mode;
            if self.prepared_for != generation
                || mode_changed
                || retry
                || self.deferred
                || self
                    .publication
                    .waiting
                    .iter()
                    .any(|r| demand.intersects(*r))
                || self.fragments.needs_preparation(demand)
            {
                let mut plan = Preparation::default();
                let (tasks, _) = self.fragments.prepare(
                    roots(frame.instances(), &self.publication, mode),
                    demand,
                    &mut self.residency,
                    &mut plan,
                    self.layers
                        .policy
                        .unwrap_or_else(crate::LayerPolicy::desktop)
                        .stable_repaints,
                    now,
                    &mut self.stats,
                );
                let mut retired = Vec::new();
                let mut scheduled = Vec::new();
                if selected() == Some(Policy::Items) {
                    self.publication.waiting.clear();
                    self.publication.blocked = false;
                    let mut shared: HashMap<_, _> = tasks.iter().copied().collect();
                    for (index, c) in frame.instances().iter().enumerate() {
                        if c.cache != CachePolicy::Prefer || mode != RasterMode::Explicit {
                            continue;
                        }
                        let old = self.publication.items.get_mut(&c.key);
                        if let Some((program, _, seen)) = old
                            && *program == c.program.id()
                            && !c.visible.is_empty()
                        {
                            *seen = frame.generation;
                            continue;
                        }
                        let old = self.publication.items.remove(&c.key);
                        if let Some((program, id, _)) = old {
                            retired.push((
                                id,
                                if program == c.program.id() && c.visible.is_empty() {
                                    Retention::Nearby
                                } else {
                                    Retention::Discard
                                },
                            ));
                        }
                        if c.visible.is_empty() || c.program.analysis().effects.barrier {
                            continue;
                        }
                        if !demand.intersects(c.visible) {
                            self.publication.waiting.push(c.visible);
                            continue;
                        }
                        let recipe = Recipe::item(&c.program);
                        let owner = Owner::Item(c.program.id());
                        if let Some(id) = self.residency.acquire(owner, &recipe) {
                            self.publication.bind(c, id, frame.generation);
                        } else {
                            match self.schedule_item(
                                &c.program,
                                recipe,
                                old.map(|(_, id, _)| id),
                                &mut plan,
                                &mut shared,
                            ) {
                                Ok(task) => scheduled.push((index, owner, task)),
                                Err(can_retry) => self.publication.blocked |= can_retry,
                            }
                        }
                    }
                    self.publication.items.retain(|_, &mut (_, id, seen)| {
                        if mode == RasterMode::Explicit && seen == frame.generation {
                            return true;
                        }
                        retired.push((id, Retention::Discard));
                        false
                    });
                }
                if !tasks.is_empty() || !scheduled.is_empty() {
                    return Some(Pending {
                        frame: frame.freeze(),
                        plan,
                        fragment_tasks: tasks,
                        full: false,
                        current_layers: Default::default(),
                        group_ranges: vec![],
                        changed: Default::default(),
                        previous: vec![],
                        retired,
                        scheduled,
                        layer_tasks: vec![],
                    });
                }
                for (id, retention) in retired {
                    self.residency.release(id, retention, now);
                }
                self.deferred = false;
                self.fragments.retain_for_draw(
                    roots(frame.instances(), &self.publication, mode),
                    &mut self.residency,
                    now,
                );
            }
        }
        self.prepared_for = generation;
        None
    }

    pub(super) fn finish_publication(&mut self, frame: &crate::frame::FrozenFrame, now: Instant) {
        self.fragments.retain_for_draw(
            roots(&frame.chunks, &self.publication, self.mode),
            &mut self.residency,
            now,
        );
        self.residency.maintain(now);
        self.prepared_for = Some((frame.identity, frame.generation));
    }
}
