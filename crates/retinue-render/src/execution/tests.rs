use super::*;
use crate::{Bounds, Color, Command, PaintProgram, ResidencyPolicy, residency::Owner};

fn recipe() -> Recipe {
    Recipe::item(
        &PaintProgram::new(
            1,
            (24.0, 20.0),
            1.25,
            Rc::new(vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 24.0, 20.0),
                radius: 3.0,
                color: Color::rgba(30, 110, 200, 170),
            }]),
        )
        .unwrap(),
    )
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
    let fallback = graph
        .result(parent, &results)
        .unwrap()
        .image
        .to_rgba()
        .unwrap();
    let mut oracle = Preparation::default();
    let task = oracle.add(recipe, None, Vec::new(), 1);
    let expected = oracle.execute(&mut PaintStats::default());
    assert_eq!(
        fallback,
        oracle
            .result(task, &expected)
            .unwrap()
            .image
            .to_rgba()
            .unwrap()
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
    let actual = graph
        .result(parent, &results)
        .unwrap()
        .image
        .to_rgba()
        .unwrap();
    let mut raw = Preparation::default();
    let direct = raw.add(recipe, None, Vec::new(), 1);
    let results = raw.execute(&mut PaintStats::default());
    assert_eq!(
        actual,
        raw.result(direct, &results)
            .unwrap()
            .image
            .to_rgba()
            .unwrap()
    );
}

#[test]
fn failed_named_fragment_preparation_replays_its_source() {
    let child = recipe().fragments[0].program.clone();
    let parent = PaintProgram::new(
        2,
        (24.0, 20.0),
        1.25,
        Rc::new(vec![Command::Fragment {
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
    let actual = plan
        .result(parent_task, &results)
        .unwrap()
        .image
        .to_rgba()
        .unwrap();
    let mut oracle = Preparation::default();
    let task = oracle.add(recipe, None, Vec::new(), 1);
    let results = oracle.execute(&mut PaintStats::default());
    assert_eq!(
        actual,
        oracle
            .result(task, &results)
            .unwrap()
            .image
            .to_rgba()
            .unwrap()
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
        graph
            .result(child, &results)
            .unwrap()
            .image
            .to_rgba()
            .unwrap(),
        graph
            .result(parent, &results)
            .unwrap()
            .image
            .to_rgba()
            .unwrap()
    );
}

#[test]
fn identical_recipe_reuses_immutable_surface_without_rasterization() {
    let recipe = recipe();
    let mut graph = Preparation::default();
    let task = graph.add(recipe.clone(), None, Vec::new(), 1);
    let results = graph.execute(&mut PaintStats::default());
    let old = graph.result(task, &results).unwrap().clone();
    let mut next = Preparation::default();
    let task = next.add(recipe, Some(old.clone()), Vec::new(), 1);
    let mut stats = PaintStats::default();
    let results = next.execute(&mut stats);
    assert_eq!(stats.repaired_pixels, 0);
    assert!(!next.rasterized(task));
    assert_eq!(
        old.image.id(),
        next.result(task, &results).unwrap().image.id()
    );
}

#[test]
fn clip_changes_outside_paint_need_no_output_reservation() {
    let mut recipe = recipe();
    recipe.size = (40, 40);
    let canvas = recipe.canvas();
    Rc::make_mut(&mut recipe.fragments)[0].clip = canvas;
    let mut policy = ResidencyPolicy::desktop(0);
    policy.peak_bytes = 40 * 40 * 4;
    let mut residency = Residency::new(policy);
    let mut graph = Preparation::default();
    let task = graph
        .admit(&mut residency, recipe.clone(), None, Vec::new(), 1)
        .unwrap();
    let results = graph.execute(&mut PaintStats::default());
    graph.complete(task);
    let old = graph.result(task, &results).unwrap().clone();
    residency.insert(Owner::Group(ScopeId::group(1.into())), old.clone(), 100);
    assert!(residency.reserve(&recipe).is_none());

    let mut changed = recipe.clone();
    Rc::make_mut(&mut changed.fragments)[0].clip.width -= 1;
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
    let result = next.result(task, &results).unwrap();
    assert_eq!(stats.repaired_pixels, 0);
    assert!(!next.rasterized(task));
    assert_eq!(old.image.id(), result.image.id());
    assert!(result.recipe == changed);

    Rc::make_mut(&mut changed.fragments)[0].clip.width = 15;
    assert!(
        next.admit(&mut residency, changed, Some(old), Vec::new(), 1)
            .is_none()
    );
}
