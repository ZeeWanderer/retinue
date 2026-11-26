use super::*;
use crate::{ProgramId, execution::TaskId};
use std::ops::Range;

pub(super) struct Pending {
    pub frame: crate::frame::FrozenFrame,
    pub plan: Preparation,
    pub full: bool,
    pub current_layers: BTreeMap<ScopeId, Prepared>,
    pub group_ranges: Vec<Range<usize>>,
    pub changed: BTreeSet<usize>,
    pub previous: Vec<Option<RasterId>>,
    pub retired: Vec<(RasterId, Retention)>,
    pub scheduled: Vec<(usize, Owner, TaskId)>,
    pub fragment_tasks: Vec<(ProgramId, TaskId)>,
    pub layer_tasks: Vec<(ScopeId, TaskId)>,
}

pub(super) struct Completed {
    pub pending: Pending,
    pub results: Completion,
}

impl Pending {
    pub fn execute(
        self,
        host: &RenderHost,
        fonts: &Arc<crate::text::preparation::PreparedFonts>,
    ) -> Completed {
        let results = self.plan.execute_on(host, fonts);
        Completed {
            pending: self,
            results,
        }
    }
}

impl Renderer {
    pub(super) fn publish_preparation(&mut self, completed: Completed) {
        if let Some(leaf) = &mut self.leaf {
            leaf.invalidate();
        }
        let Completed { pending, results } = completed;
        let Pending {
            frame,
            mut plan,
            full,
            current_layers,
            group_ranges,
            mut changed,
            previous,
            mut retired,
            scheduled,
            fragment_tasks,
            layer_tasks,
        } = pending;
        let now = Instant::now();
        results.accumulate(&mut self.stats);
        self.deferred = results.has_deferred();
        self.fragments.publish(
            &fragment_tasks,
            &mut plan,
            &results,
            &mut self.residency,
            &mut self.stats,
        );
        let mut published = HashMap::new();
        for (index, owner, task) in scheduled {
            plan.complete(task);
            if results.deferred(task) {
                #[cfg(feature = "benchmark")]
                if !publication::Trial::active() {
                    self.retry_items.insert(index);
                }
                #[cfg(not(feature = "benchmark"))]
                self.retry_items.insert(index);
            }
            if let Some(result) = results.result(task) {
                let id = if let Some(&id) = published.get(&task) {
                    self.residency.retain(id, &result.recipe);
                    id
                } else {
                    self.stats.prepared_rasters += usize::from(plan.rasterized(task));
                    let id = self
                        .residency
                        .acquire(owner, &result.recipe)
                        .unwrap_or_else(|| {
                            self.residency
                                .insert(owner, result.clone(), results.elapsed(task))
                        });
                    published.insert(task, id);
                    id
                };
                #[cfg(feature = "benchmark")]
                if self.optimizations && publication::Trial::active() {
                    self.publication
                        .bind(&frame.chunks[index], id, frame.generation);
                    continue;
                }
                self.prepared[index] = Some(id);
            }
        }
        #[cfg(feature = "benchmark")]
        if self.optimizations && publication::Trial::active() {
            for (id, retention) in retired {
                self.residency.release(id, retention, now);
            }
            self.finish_publication(&frame, now);
            return;
        }
        if self.optimizations && !layer_tasks.is_empty() {
            let (promoted, demoted) =
                self.layers
                    .publish(&mut plan, &results, &layer_tasks, &mut self.residency, now);
            self.stats.promoted_layers += promoted;
            self.stats.demoted_layers += demoted;
            self.stats.layer_prepare_ns = layer_tasks
                .iter()
                .map(|(_, task)| results.elapsed(*task))
                .sum();
            self.refresh_coverage();
            Self::coverage_changes(&current_layers, &self.layers.prepared, &mut changed);
            for ((_, task), range) in layer_tasks.iter().zip(&group_ranges) {
                if results.result(*task).is_none() {
                    self.retry_items.extend(range.clone());
                }
            }
        }
        for &index in &changed {
            if self.coverage.covers(index, frame.chunks[index].visible)
                && !frame.chunks[index].visible.is_empty()
            {
                if let Some(id) = self.prepared[index].take() {
                    retired.push((id, Retention::Discard));
                }
            }
        }
        // Rebind every surviving consumer before releasing the preceding plan.
        for id in previous.into_iter().flatten() {
            retired.push((id, Retention::Discard));
        }
        for (id, retention) in retired {
            self.residency.release(id, retention, now);
        }
        self.stats.planned_instances = changed.len();
        for index in changed {
            let visible = frame.chunks[index].visible;
            let regions = self.coverage.remaining(index, visible);
            if regions.is_empty() {
                self.draw_plan.remove(&instance_order(index));
            } else {
                let raster = self.prepared[index].is_some();
                self.draw_plan.insert(
                    instance_order(index),
                    if regions.as_slice() != [visible] {
                        DrawStep::Clipped {
                            instance: index,
                            regions,
                            raster,
                        }
                    } else if raster {
                        DrawStep::Item { instance: index }
                    } else {
                        DrawStep::Replay { instance: index }
                    },
                );
            }
        }
        self.stats.layer_bytes = self.layers.bytes(&self.residency);
        let roots = self.draw_plan.values().filter_map(|step| match step {
            DrawStep::Replay { instance }
            | DrawStep::Clipped {
                instance,
                raster: false,
                ..
            } => {
                let chunk = &frame.chunks[*instance];
                Some((&chunk.program, chunk.origin, chunk.clip))
            }
            _ => None,
        });
        self.fragments
            .retain_for_draw(roots, &mut self.residency, now);
        drop(results);
        drop(plan);
        if full {
            let mut programs = HashSet::new();
            for chunk in frame.chunks.iter() {
                fragments::program_ids(&chunk.program, &mut programs);
            }
            let groups: HashMap<_, _> = frame.groups.iter().map(|g| (g.key, g)).collect();
            self.residency.retain_warm(|owner| match owner {
                Owner::Item(id) => programs.contains(&id),
                Owner::Group(id) => groups
                    .get(&id.group)
                    .is_some_and(|g| super::super::coverage::declared(g, id)),
            });
        }
        self.residency.maintain(now);
        self.prepared_for = Some((frame.identity, frame.generation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bounds, Command, PaintInstance, PaintProgram};
    use std::sync::Arc;

    #[test]
    fn planning_reserves_execution_produces_and_publication_installs() {
        let program = PaintProgram::new(
            1,
            (32.0, 24.0),
            1.0,
            Arc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 32.0, 24.0),
                radius: 2.0,
                color: Color::rgb(0x345678),
            }]),
        )
        .unwrap();
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 24,
        };
        let mut item = PaintInstance::new(1.into(), program, (0, 0), bounds);
        item.cache = CachePolicy::Prefer;
        let mut frame = PaintFrame::default();
        frame
            .replace((32, 24), 1.0, vec![item], Vec::new())
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let host = renderer.host().clone();
        let _pass = host.begin_pass(0);
        let pending = renderer
            .plan_preparation(&frame, RasterMode::Explicit, PaintDemand::Full)
            .unwrap();
        assert!(renderer.residency_stats().pending_bytes > 0);
        assert_eq!(renderer.residency_stats().selected_bytes, 0);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        let completed = pending.execute(&host, &renderer.text_fonts);
        assert_eq!(renderer.residency_stats().selected_bytes, 0);
        assert_eq!(renderer.stats.preparation_tasks, 0);
        renderer.publish_preparation(completed);
        assert_eq!(renderer.residency_stats().pending_bytes, 0);
        assert!(renderer.residency_stats().selected_bytes > 0);
        assert_eq!(renderer.stats.preparation_tasks, 1);
        assert_eq!(renderer.stats.prepared_rasters, 1);
        assert!(matches!(
            renderer.execution_plan().next(),
            Some(DrawStep::Item { .. })
        ));
    }
}
