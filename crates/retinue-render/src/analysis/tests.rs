use super::*;
use crate::{Bounds, Color};
use std::rc::Rc;

fn fill(bounds: Bounds, color: u32) -> Command {
    Command::Fill {
        bounds,
        radius: 0.0,
        color: Color::rgb(color),
    }
}

fn group(color: u32, offset: (f32, f32), clip: Bounds) -> Command {
    Command::Translate {
        offset,
        commands: Rc::new(vec![Command::Clip {
            bounds: clip,
            commands: Rc::new(vec![
                fill(Bounds::new(0.0, 0.0, 200.0, 100.0), 0x202020),
                fill(Bounds::new(12.0, 18.0, 20.0, 16.0), color),
            ]),
        }]),
    }
}

#[test]
fn unchanged_wrappers_preserve_child_damage_at_each_scale() {
    let clip = Bounds::new(0.0, 0.0, 28.0, 100.0);
    let a = group(0xff0000, (8.0, 4.0), clip);
    let b = group(0x00ff00, (8.0, 4.0), clip);
    for scale in [1.0, 1.25, 2.0] {
        assert_eq!(changed_writes(Some(&a), Some(&a), scale), Pixels::default());
        assert_eq!(
            changed_writes(Some(&a), Some(&b), scale),
            Bounds::new(12.0, 18.0, 20.0, 16.0)
                .pixels(scale)
                .intersection(clip.pixels(scale))
                .translated(((8.0 * scale).round() as i32, (4.0 * scale).round() as i32)),
        );
    }
}

#[test]
fn wrapper_changes_and_removed_content_include_old_and_new_pixels() {
    let a = group(0xff0000, (8.0, 4.0), Bounds::new(0.0, 0.0, 28.0, 100.0));
    let b = group(0xff0000, (18.0, 9.0), Bounds::new(0.0, 0.0, 80.0, 100.0));
    for scale in [1.0, 1.25, 2.0] {
        let before = Analysis::new(std::slice::from_ref(&a), scale)
            .effects
            .writes;
        let after = Analysis::new(std::slice::from_ref(&b), scale)
            .effects
            .writes;
        assert_eq!(
            changed_writes(Some(&a), Some(&b), scale),
            before.union(after)
        );
        assert_eq!(changed_writes(Some(&a), None, scale), before);
        assert_eq!(changed_writes(None, Some(&b), scale), after);
    }
}

#[test]
fn normalized_fragments_preserve_child_damage() {
    let clip = Bounds::new(0.0, 0.0, 200.0, 100.0);
    for scale in [1.0, 1.25, 2.0] {
        let mut store = crate::program::ProgramStore::default();
        let command = |store: &mut crate::program::ProgramStore, color| Command::Fragment {
            program: store
                .record(
                    1u64,
                    (220.0, 120.0),
                    scale,
                    Rc::new(vec![group(color, (8.0, 4.0), clip)]),
                )
                .unwrap(),
            context: Default::default(),
        };
        let a = command(&mut store, 0xff0000);
        let b = command(&mut store, 0x00ff00);
        assert_eq!(
            changed_writes(Some(&a), Some(&b), scale),
            Bounds::new(20.0, 22.0, 20.0, 16.0).pixels(scale)
        );
    }
}
