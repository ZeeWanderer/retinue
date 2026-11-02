use super::*;
use crate::{Bounds, Color, Command, PaintInstance};

fn program(x: f32, color: u32) -> PaintProgram {
    PaintProgram::new(
        1,
        (40.0, 30.0),
        1.0,
        Rc::new(vec![
            Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 40.0, 30.0),
                radius: 0.0,
                color: Color::rgb(0x102030),
            },
            Command::Translate {
                offset: (0.4, 0.4),
                commands: Rc::new(vec![Command::Translate {
                    offset: (0.4, 0.4),
                    commands: Rc::new(vec![Command::Fill {
                        bounds: Bounds::new(x, 2.0, 3.0, 3.0),
                        radius: 0.0,
                        color: Color::rgb(color),
                    }]),
                }]),
            },
        ]),
    )
    .unwrap()
}

#[test]
fn repair_uses_physical_analysis_not_unsnapped_nested_bounds() {
    let before = Recipe::item(&program(2.0, 0x123456));
    let after = Recipe::item(&program(2.0, 0xabcdef));
    let repair = after.repair(Some(&before));
    assert_eq!(
        repair.regions,
        vec![Rect {
            x: 2,
            y: 2,
            width: 3,
            height: 3
        }]
    );
    assert!(repair.copy);
}

#[test]
fn unchanged_operations_do_not_dirty_the_whole_parent() {
    let clip = Rect {
        width: 100,
        height: 80,
        ..Rect::default()
    };
    let mut frame = PaintFrame::default();
    let common = program(6.0, 0x123456);
    let group = |frame: &mut PaintFrame, first| {
        frame
            .replace(
                (100, 80),
                1.0,
                vec![
                    PaintInstance::new(1.into(), first, (5, 5), clip),
                    PaintInstance::new(2.into(), common.clone(), (45, 5), clip),
                ],
                Vec::new(),
            )
            .unwrap();
        Recipe::region(frame, 0..2, (0, 0), (clip.width, clip.height)).unwrap()
    };
    let before = group(&mut frame, program(2.0, 0x123456));
    let after = group(&mut frame, program(2.0, 0xabcdef));
    let repair = after.repair(Some(&before));
    assert_eq!(
        repair.regions,
        vec![Rect {
            x: 7,
            y: 7,
            width: 3,
            height: 3
        }]
    );
}

#[test]
fn reordering_and_removal_damage_old_and_new_contributions() {
    let mut before = Recipe::item(&program(2.0, 0x123456));
    let mut second = before.fragments[0].clone();
    second.key = 2.into();
    second.origin = (10, 0);
    before.fragments = Rc::new(vec![before.fragments[0].clone(), second]);
    let mut after = before.clone();
    after.fragments = Rc::new(before.fragments.iter().rev().cloned().collect());
    assert_eq!(after.repair(Some(&before)).regions, vec![before.canvas()]);
    after.fragments = Rc::new(Vec::new());
    assert_eq!(after.repair(Some(&before)).regions, vec![before.canvas()]);
}
