use super::*;
use crate::{Bounds, Color, Command, PaintProgram, ResidencyPolicy, residency::Owner};

fn recipe() -> Recipe {
    Recipe::item(
        &PaintProgram::new(
            1,
            (24.0, 20.0),
            1.25,
            Arc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 24.0, 20.0),
                radius: 3.0,
                color: Color::rgba(30, 110, 200, 170),
            }]),
        )
        .unwrap(),
    )
}

#[test]
fn execution_results_do_not_mutate_the_plan_or_previous_completions() {
    let plan = {
        let mut plan = Preparation::default();
        plan.add(recipe(), None, Vec::new(), 1);
        plan
    };
    let mut failed_stats = PaintStats::default();
    let failed = plan.execute_with(&mut failed_stats, |_, _| None);
    let mut success_stats = PaintStats::default();
    let succeeded = plan.execute(&mut success_stats);
    assert!(failed.result(TaskId(0)).is_none());
    assert!(succeeded.result(TaskId(0)).is_some());
    assert_eq!(failed_stats.failed_preparations, 1);
    assert_eq!(success_stats.failed_preparations, 0);
    assert_eq!(success_stats.preparation_tasks, 1);
    assert_eq!(plan.tasks.len(), 1);
    assert_eq!(plan.edges, 0);
}

#[test]
fn deferred_outcomes_do_not_report_failure_or_reuse_an_earlier_result() {
    let mut plan = Preparation::default();
    let first = plan.add(recipe(), None, Vec::new(), 1);
    let second = plan.add(recipe(), None, Vec::new(), 1);
    let host = RenderHost::new(crate::HostPolicy {
        preparation_tasks: 1,
        ..crate::HostPolicy::standalone(1024 * 1024)
    });
    let pass = host.begin_pass(0);
    let completed = plan.execute_on(&host, &Arc::default());
    drop(pass);
    let mut stats = PaintStats::default();
    completed.accumulate(&mut stats);
    assert!(completed.result(first).is_some());
    assert!(completed.result(second).is_none());
    assert!(completed.deferred(second));
    assert!(completed.has_deferred());
    assert_eq!(stats.failed_preparations, 0);
    assert_eq!(stats.deferred_preparations, 1);
    let retried = plan.execute(&mut PaintStats::default());
    assert!(retried.result(second).is_some());
    assert!(completed.deferred(second));
}

#[test]
fn native_execution_uses_the_host_admitted_participant_count() {
    let host = RenderHost::new(crate::HostPolicy {
        threads: 1,
        ..crate::HostPolicy::standalone(1024 * 1024)
    });
    for preferred in [0, 1, u32::MAX] {
        let source = recipe();
        let mut plan = Preparation::default();
        let task = plan.add(source.clone(), None, Vec::new(), preferred);
        let _pass = host.begin_pass(0);
        let completed = plan.execute_budgeted(Some(&host), |task, inputs, threads| {
            assert_eq!(threads, 1);
            task.render_with_threads(inputs, NonZeroU32::new(threads).unwrap(), &Arc::default())
                .ok()
        });
        assert!(completed.result(task).is_some());
        assert_eq!(host.stats().preparation_tasks, 1);

        let mut reuse = Preparation::default();
        let task = reuse.add(
            source,
            completed.result(task).cloned(),
            Vec::new(),
            preferred,
        );
        assert!(
            reuse
                .execute_on(&host, &Arc::default())
                .result(task)
                .is_some()
        );
        assert_eq!(host.stats().preparation_tasks, 1);
    }
}

#[test]
#[should_panic(expected = "materialization dependency has not completed")]
fn unfinished_producers_are_not_treated_as_failed_optional_rasters() {
    let results = Completion::new(1);
    Input::Task(TaskId(0)).resolve(&results.outputs);
}

#[test]
fn independent_tasks_can_finish_in_reverse_order_without_rearranging_storage() {
    let mut plan = Preparation::default();
    plan.add(recipe(), None, Vec::new(), 1);
    plan.add(recipe(), None, Vec::new(), 2);
    let expected = plan.execute(&mut PaintStats::default());
    let mut completed = Completion::new(plan.tasks.len());
    let storage = completed.outputs.as_ptr();
    for index in [1, 0] {
        let (inputs, slot) = completed.outputs.split_at_mut(index);
        let task = &plan.tasks[index];
        slot[0] = task.run_with(inputs, task.threads, &mut |task, inputs, threads| {
            task.render_with_threads(inputs, NonZeroU32::new(threads).unwrap(), &Arc::default())
                .ok()
        });
    }
    assert_eq!(completed.outputs.as_ptr(), storage);
    for index in 0..2 {
        let id = TaskId(index);
        assert_eq!(
            completed.result(id).unwrap().image.to_rgba().unwrap(),
            expected.result(id).unwrap().image.to_rgba().unwrap(),
        );
    }
}

#[test]
fn repairing_translucent_content_matches_a_fresh_zeroed_target() {
    use crate::materialization::Fragment;
    let program = |revision, x, alpha| {
        PaintProgram::new(
            revision,
            (48.0, 32.0),
            1.0,
            Arc::new(vec![Command::Fill {
                bounds: Bounds::new(x, 4.0, 8.0, 12.0),
                radius: 2.0,
                color: Color::rgba(90, 160, 210, alpha),
            }]),
        )
        .unwrap()
    };
    let recipe = |p| Recipe {
        size: (48, 32),
        fragments: Arc::new(vec![Fragment {
            key: 1.into(),
            program: p,
            origin: (0, 0),
            clip: Rect {
                x: 0,
                y: 0,
                width: 48,
                height: 32,
            },
        }]),
    };
    for threads in [1, 2, 4] {
        let mut graph = Preparation::default();
        let task = graph.add(recipe(program(1, 4.0, 127)), None, Vec::new(), threads);
        let mut old = graph
            .execute(&mut PaintStats::default())
            .result(task)
            .unwrap()
            .clone();
        for (revision, x, alpha) in [(2, 18.0, 90), (3, 18.0, 0), (4, 4.0, 255)] {
            let next = recipe(program(revision, x, alpha));
            let mut graph = Preparation::default();
            let partial = graph.add(next.clone(), Some(old), Vec::new(), threads);
            let fresh = graph.add(next, None, Vec::new(), threads);
            assert!(graph.tasks[partial.0].repair.copy);
            let results = graph.execute(&mut PaintStats::default());
            let partial = results.result(partial).unwrap().clone();
            assert_eq!(
                partial.image.to_rgba().unwrap(),
                results.result(fresh).unwrap().image.to_rgba().unwrap()
            );
            old = partial;
        }
    }
}

#[test]
fn dependency_failure_replays_source_instead_of_publishing_missing_pixels() {
    let recipe = recipe();
    let mut graph = Preparation::default();
    let child = graph.add(recipe.clone(), None, Vec::new(), 1);
    let parent = graph.add(
        recipe.clone(),
        None,
        vec![Substitute {
            fragments: 0..1,
            regions: vec![SourceRegion {
                source: Input::Task(child),
                origin: (0, 0),
                clip: recipe.canvas(),
            }],
        }],
        2,
    );
    let mut calls = 0;
    let mut stats = PaintStats::default();
    let results = graph.execute_with(&mut stats, |task, results| {
        calls += 1;
        if calls == 1 {
            None
        } else {
            task.render(results).ok()
        }
    });
    assert_eq!(stats.preparation_edges, 1);
    assert_eq!(stats.failed_preparations, 1);
    let fallback = results.result(parent).unwrap().image.to_rgba().unwrap();
    let mut oracle = Preparation::default();
    let task = oracle.add(recipe, None, Vec::new(), 1);
    let expected = oracle.execute(&mut PaintStats::default());
    assert_eq!(
        fallback,
        expected.result(task).unwrap().image.to_rgba().unwrap()
    );
}

#[test]
fn partial_substitution_failure_replays_only_unavailable_pixels() {
    let recipe = recipe();
    let mut graph = Preparation::default();
    let left = graph.add(recipe.clone(), None, Vec::new(), 1);
    let right = graph.add(recipe.clone(), None, Vec::new(), 1);
    let split = recipe.size.0 / 2;
    let parent = graph.add(
        recipe.clone(),
        None,
        vec![Substitute {
            fragments: 0..1,
            regions: vec![
                SourceRegion {
                    source: Input::Task(left),
                    origin: (0, 0),
                    clip: Rect {
                        width: split,
                        ..recipe.canvas()
                    },
                },
                SourceRegion {
                    source: Input::Task(right),
                    origin: (0, 0),
                    clip: Rect {
                        x: split,
                        width: recipe.size.0 - split,
                        ..recipe.canvas()
                    },
                },
            ],
        }],
        2,
    );
    let mut calls = 0;
    let mut stats = PaintStats::default();
    let results = graph.execute_with(&mut stats, |task, results| {
        calls += 1;
        if calls == 2 {
            None
        } else {
            task.render(results).ok()
        }
    });
    assert_eq!(stats.failed_preparations, 1);
    assert_eq!(stats.preparation_edges, 2);
    assert_eq!(stats.preparation_blits, 1);
    let actual = results.result(parent).unwrap().image.to_rgba().unwrap();
    let mut raw = Preparation::default();
    let direct = raw.add(recipe, None, Vec::new(), 1);
    let results = raw.execute(&mut PaintStats::default());
    assert_eq!(
        actual,
        results.result(direct).unwrap().image.to_rgba().unwrap()
    );
}

#[test]
fn failed_named_fragment_preparation_replays_its_source() {
    let child = recipe().fragments[0].program.clone();
    let parent = PaintProgram::new(
        2,
        (24.0, 20.0),
        1.25,
        Arc::new(vec![Command::Fragment {
            program: child,
            context: crate::OptimizationContext::default().with(CacheHint::Stable),
        }]),
    )
    .unwrap();
    let child = &parent.calls()[0].program;
    let recipe = Recipe::item(&parent);
    let mut plan = Preparation::default();
    let task = plan.add(Recipe::item(child), None, Vec::new(), 1);
    plan.bind_fragment(child.id(), Input::Task(task));
    let parent_task = plan.add(recipe.clone(), None, Vec::new(), 1);
    let mut calls = 0;
    let mut stats = PaintStats::default();
    let results = plan.execute_with(&mut stats, |task, results| {
        calls += 1;
        if calls == 1 {
            None
        } else {
            task.render(results).ok()
        }
    });
    assert_eq!(stats.failed_preparations, 1);
    assert_eq!(stats.preparation_edges, 1);
    assert_eq!(stats.fragment_blits, 0);
    let actual = results
        .result(parent_task)
        .unwrap()
        .image
        .to_rgba()
        .unwrap();
    let mut oracle = Preparation::default();
    let task = oracle.add(recipe, None, Vec::new(), 1);
    let results = oracle.execute(&mut PaintStats::default());
    assert_eq!(
        actual,
        results.result(task).unwrap().image.to_rgba().unwrap()
    );
}

#[test]
fn successful_producers_complete_before_consumption() {
    let recipe = recipe();
    let mut graph = Preparation::default();
    let child = graph.add(recipe.clone(), None, Vec::new(), 2);
    let parent = graph.add(
        recipe.clone(),
        None,
        vec![Substitute {
            fragments: 0..1,
            regions: vec![SourceRegion {
                source: Input::Task(child),
                origin: (0, 0),
                clip: recipe.canvas(),
            }],
        }],
        2,
    );
    let results = graph.execute(&mut PaintStats::default());
    assert_eq!(
        results.result(child).unwrap().image.to_rgba().unwrap(),
        results.result(parent).unwrap().image.to_rgba().unwrap()
    );
}

#[test]
fn parallel_preparation_matches_serial_with_shared_and_substituted_inputs() {
    let source = recipe();
    let mut plan = Preparation::default();
    let producers: Vec<_> = (0..8)
        .map(|_| plan.add(source.clone(), None, vec![], 4))
        .collect();
    for producer in producers {
        plan.add(
            source.clone(),
            None,
            vec![Substitute {
                fragments: 0..1,
                regions: vec![SourceRegion {
                    source: Input::Task(producer),
                    origin: (0, 0),
                    clip: source.canvas(),
                }],
            }],
            4,
        );
    }
    let serial = plan.execute(&mut PaintStats::default());
    for participants in [2, 4] {
        let host = RenderHost::new(crate::HostPolicy {
            threads: participants,
            ..crate::HostPolicy::standalone(1024 * 1024)
        });
        let _pass = host.begin_pass(0);
        let actual = plan.execute_on(&host, &Arc::default());
        for index in 0..plan.tasks.len() {
            let id = TaskId(index);
            assert!(
                actual.result(id).unwrap().image.to_rgba().unwrap()
                    == serial.result(id).unwrap().image.to_rgba().unwrap(),
                "parallel raster mismatch: participants={participants}, task={index}"
            );
        }
        let mut stats = PaintStats::default();
        actual.accumulate(&mut stats);
        assert_eq!(stats.preparation_blits, 8);
        assert_eq!(stats.preparation_edges, 8);
        assert_eq!(stats.failed_preparations, 0);
        assert_eq!(host.stats().execution.retained, 0);
    }
}

#[test]
fn active_native_pass_keeps_preparation_on_its_admitted_caller() {
    let host = RenderHost::new(crate::HostPolicy::desktop());
    let _frame = host.begin_frame(NonZeroU32::new(4).unwrap());
    assert!(host.preparation_executor().is_none());
    let mut plan = Preparation::default();
    plan.add(recipe(), None, vec![], 4);
    plan.add(recipe(), None, vec![], 4);
    let actual = plan.execute_on(&host, &Arc::default());
    assert!(actual.result(TaskId(0)).is_some());
    assert_eq!(host.stats().execution.retained, 0);
}

#[test]
fn identical_recipe_reuses_immutable_surface_without_rasterization() {
    let recipe = recipe();
    let mut graph = Preparation::default();
    let task = graph.add(recipe.clone(), None, Vec::new(), 1);
    let results = graph.execute(&mut PaintStats::default());
    let old = results.result(task).unwrap().clone();
    let mut next = Preparation::default();
    let task = next.add(recipe, Some(old.clone()), Vec::new(), 1);
    let mut stats = PaintStats::default();
    let results = next.execute(&mut stats);
    assert_eq!(stats.repaired_pixels, 0);
    assert!(!next.rasterized(task));
    assert_eq!(old.image.id(), results.result(task).unwrap().image.id());
}

#[test]
fn clip_changes_outside_paint_need_no_output_reservation() {
    let mut recipe = recipe();
    recipe.size = (40, 40);
    let canvas = recipe.canvas();
    Arc::make_mut(&mut recipe.fragments)[0].clip = canvas;
    let mut policy = ResidencyPolicy::desktop(0);
    policy.peak_bytes = 40 * 40 * 4;
    let mut residency = Residency::new(policy);
    let mut graph = Preparation::default();
    let task = graph
        .admit(&mut residency, recipe.clone(), None, Vec::new(), 1)
        .unwrap();
    let results = graph.execute(&mut PaintStats::default());
    graph.complete(task);
    let old = results.result(task).unwrap().clone();
    residency.insert(Owner::Group(ScopeId::group(1.into())), old.clone(), 100);
    assert!(residency.reserve(&recipe).is_none());

    let mut changed = recipe.clone();
    Arc::make_mut(&mut changed.fragments)[0].clip.width -= 1;
    let mut next = Preparation::default();
    let task = next
        .admit(
            &mut residency,
            changed.clone(),
            Some(old.clone()),
            Vec::new(),
            1,
        )
        .unwrap();
    assert_eq!(residency.stats().pending_bytes, 0);
    let mut stats = PaintStats::default();
    let results = next.execute(&mut stats);
    let result = results.result(task).unwrap();
    assert_eq!(stats.repaired_pixels, 0);
    assert!(!next.rasterized(task));
    assert_eq!(old.image.id(), result.image.id());
    assert!(result.recipe == changed);

    Arc::make_mut(&mut changed.fragments)[0].clip.width = 15;
    assert!(
        next.admit(&mut residency, changed, Some(old), Vec::new(), 1)
            .is_none()
    );
}
