use super::{Policy, minimize, runner, scenarios};
use crate::{Bounds, Color, Command, optimize};
use std::rc::Rc;

#[test]
fn policy_space_respects_pass_prerequisites() {
    let policies = Policy::candidates();
    assert_eq!(policies.len(), 12);
    assert!(policies.contains(&Policy::DEFAULT));
    assert!(policies.contains(&Policy::REFERENCE));
    for p in policies {
        assert!(p.compiled || p.cache == super::Cache::None);
    }
}

#[test]
fn simplification_has_semantic_positive_and_negative_cases() {
    let fill = |alpha, radius| Command::Fill {
        bounds: Bounds::new(0.0, 0.0, 30.0, 30.0),
        radius,
        color: Color::rgba(60, 90, 120, alpha),
    };
    let (_, summary) = optimize::compile(&Rc::new(vec![fill(0, 0.0)]), 1.0);
    assert_eq!(summary.removed, 1);
    for top in [fill(180, 0.0), fill(255, 4.0)] {
        let (_, summary) = optimize::compile(&Rc::new(vec![fill(255, 0.0), top]), 1.0);
        assert_eq!(summary.removed, 0);
    }
    let (compiled, summary) =
        optimize::compile(&Rc::new(vec![fill(255, 0.0), fill(255, 0.0)]), 1.0);
    assert_eq!(summary.removed, 1);
    assert_eq!(compiled.len(), 1);
    let (again, summary) = optimize::compile(&compiled, 1.0);
    assert_eq!(summary.removed, 0);
    assert!(Rc::ptr_eq(&compiled, &again));
}

#[test]
fn pipeline_alternatives_match_source_over_complete_traces() {
    let assets = scenarios::Assets::new();
    for case in scenarios::cases().into_iter().map(scenarios::Case::small) {
        let steps = case.steps();
        let reference = runner::reference(&assets, case, &steps);
        for policy in Policy::candidates() {
            if let Err(error) = runner::check(&assets, case, &steps, policy, 2, &reference) {
                let minimal = minimize(&steps, |steps| {
                    runner::check(
                        &assets,
                        case,
                        steps,
                        policy,
                        2,
                        &runner::reference(&assets, case, steps),
                    )
                    .is_err()
                });
                panic!(
                    "{} {}: {error:?}; reduced input: {minimal:?}",
                    case.name,
                    policy.name()
                );
            }
        }
    }
}

#[test]
fn generated_inputs_are_repeatable_and_shrinking_keeps_the_failure() {
    let assets = scenarios::Assets::new();
    let mut case = scenarios::cases()[0].small();
    for seed in [1, 17, 127] {
        case.seed = seed;
        let steps = case.steps();
        assert_eq!(steps, case.steps());
        let reference = runner::reference(&assets, case, &steps);
        runner::check(&assets, case, &steps, Policy::DEFAULT, 2, &reference).unwrap();
    }
    assert_eq!(
        minimize(&[0, 1, 2, 3, 4, 5], |s| s.contains(&2) && s.contains(&4)),
        [2, 4]
    );
}

#[test]
fn source_oracle_has_independent_background_and_placement_expectations() {
    let case = scenarios::cases()
        .into_iter()
        .find(|c| c.art == scenarios::Art::Solid)
        .unwrap()
        .small();
    let steps = &case.steps()[..1];
    let pixels = runner::reference(&scenarios::Assets::new(), case, steps);
    assert_eq!(&pixels[0][..4], &[0x2a, 0x20, 0x18, 255]);
    let offset = (10 * case.size.0 as usize + 10) * 4;
    assert_eq!(&pixels[0][offset..offset + 4], &[55, 42, 30, 255]);
}

#[test]
fn scenarios_reach_fragment_reuse_layer_promotion_and_memory_pressure() {
    let assets = scenarios::Assets::new();
    let cases = scenarios::cases();
    let automatic = Policy {
        cache: super::Cache::Automatic,
        repair: super::Repair::Full,
        ..Policy::DEFAULT
    };
    let decorated = cases[0].small();
    let trace = runner::run(
        &assets,
        decorated,
        &decorated.steps(),
        automatic,
        2,
        false,
        |_, _| {},
    );
    assert!(trace.frames.iter().any(|f| f.fragment_blits > 0));
    let rows = cases.iter().find(|c| c.layers).copied().unwrap().small();
    let trace = runner::run(&assets, rows, &rows.steps(), automatic, 2, false, |_, _| {});
    assert!(trace.frames.iter().any(|f| f.promoted_layers > 0));
    let pressure = cases
        .iter()
        .find(|c| c.name == "cache-pressure")
        .copied()
        .unwrap()
        .small();
    let trace = runner::run(
        &assets,
        pressure,
        &pressure.steps(),
        Policy::DEFAULT,
        2,
        false,
        |_, _| {},
    );
    assert!(trace.peak_raster_bytes <= pressure.peak_bytes);
}
