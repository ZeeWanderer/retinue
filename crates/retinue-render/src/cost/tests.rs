use super::*;
use crate::{Bounds, Color, Command};
use std::rc::Rc;

fn fill(bounds: Bounds) -> Command {
    Command::Fill {
        bounds,
        radius: 0.0,
        color: Color::rgb(0x104020),
    }
}

#[test]
fn summaries_preserve_coverage_not_command_count_times_parent_area() {
    let commands = [
        fill(Bounds::new(0.0, 0.0, 10.0, 10.0)),
        fill(Bounds::new(90.0, 90.0, 10.0, 10.0)),
    ];
    let [total, direct] = summarize(&commands, 1.0);
    assert_eq!(total.solid, 200.0);
    assert!((total.commands - 2.0).abs() < 0.00001);
    assert_eq!(total, direct);
    let scaled = summarize(&commands, 2.0)[0];
    assert_eq!(scaled.solid, total.solid * 4.0);
    assert_eq!(scaled.commands, total.commands);
}

#[test]
fn nested_translation_and_clip_constrain_estimated_work() {
    let commands = [Command::Clip {
        bounds: Bounds::new(0.0, 0.0, 25.0, 30.0),
        commands: Rc::new(vec![Command::Translate {
            offset: (20.0, 20.0),
            commands: Rc::new(vec![fill(Bounds::new(0.0, 0.0, 10.0, 10.0))]),
        }]),
    }];
    let work = summarize(&commands, 1.0)[0];
    assert_eq!(work.solid, 50.0);
    assert!((work.commands - 0.5).abs() < 0.00001);
}

#[test]
fn shadow_complexity_follows_outline_bands_and_not_a_fixed_weight() {
    let shadow = |blur| Command::InsetShadow {
        bounds: Bounds::new(0.0, 0.0, 120.0, 80.0),
        radius: 8.0,
        blur,
        spread: 1.0,
        color: Color::rgb(0x30ff40),
    };
    let narrow = leaf(&shadow(4.0), 1.0);
    let wide = leaf(&shadow(8.0), 1.0);
    assert_eq!(narrow.commands, 7.0);
    assert_eq!(narrow.edges, 56.0);
    assert_eq!(narrow.blended, (120 * 80 - 106 * 66) as f64);
    assert_eq!(wide.commands, 13.0);
    assert!(wide.blended > narrow.blended);
}

#[test]
fn execution_prices_work_without_mutating_it_and_shared_work_cancels() {
    let required = Work {
        solid: 1000.0,
        glyphs: 50.0,
        ..Work::default()
    };
    let remaining = Work {
        solid: 1000.0,
        ..Work::default()
    };
    let avoided = required.difference(remaining);
    assert_eq!(
        avoided,
        Work {
            glyphs: 50.0,
            ..Work::default()
        }
    );
    assert_eq!(
        Execution::SERIAL.replay(avoided),
        Execution::SERIAL.replay(required) - Execution::SERIAL.replay(remaining)
    );
    let queued = Execution {
        threads: 4,
        copy: CopyExecution::Queued,
    };
    assert!(queued.transfer(4096, 4096, 1) < Execution::SERIAL.transfer(4096, 4096, 1));
    assert_eq!(avoided, required.difference(remaining));
}
