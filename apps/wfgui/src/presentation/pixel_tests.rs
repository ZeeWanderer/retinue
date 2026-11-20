use retinue_render::{Command, PaintFrame, Rect};

pub fn text_ink(frame: &PaintFrame, origin: (u32, u32), clip: Rect) -> Vec<Rect> {
    let mut ink = Vec::new();
    for item in frame.instances() {
        let bounds = item.clip();
        let clip = Rect {
            x: bounds.x + origin.0,
            y: bounds.y + origin.1,
            ..bounds
        }
        .intersection(clip);
        collect_ink(
            item.program.commands(),
            item.program.scale(),
            (
                item.origin().0 + origin.0 as i32,
                item.origin().1 + origin.1 as i32,
            ),
            clip,
            &mut ink,
        );
    }
    ink
}

fn collect_ink(
    commands: &[Command],
    scale: f32,
    origin: (i32, i32),
    clip: Rect,
    ink: &mut Vec<Rect>,
) {
    for command in commands {
        match command {
            Command::GlyphRun { .. } => {
                ink.push(command.bounds().pixels(scale).visible(origin, clip))
            }
            Command::Fragment { program, .. } => {
                collect_ink(program.commands(), scale, origin, clip, ink)
            }
            Command::Clip { bounds, commands } => collect_ink(
                commands,
                scale,
                origin,
                bounds.pixels(scale).visible(origin, clip),
                ink,
            ),
            Command::Translate { offset, commands } => collect_ink(
                commands,
                scale,
                (
                    origin.0 + (offset.0 * scale).round() as i32,
                    origin.1 + (offset.1 * scale).round() as i32,
                ),
                clip,
                ink,
            ),
            _ => (),
        }
    }
}

pub fn mismatch(
    actual: &[u8],
    expected: &[u8],
    width: u32,
    check: Rect,
    ink: &[Rect],
) -> Option<(u32, u32)> {
    assert_eq!(actual.len(), expected.len());
    actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .enumerate()
        .find_map(|(index, (a, b))| {
            let (x, y) = (index as u32 % width, index as u32 / width);
            if a == b || !check.contains(x as f64, y as f64) {
                return None;
            }
            // Native curve clipping and the extra premultiplied blend of cached text
            // can round one channel step differently. Non-text pixels remain exact.
            let tolerance = u8::from(ink.iter().any(|rect| rect.contains(x as f64, y as f64)));
            a.iter()
                .zip(b)
                .any(|(a, b)| a.abs_diff(*b) > tolerance)
                .then_some((x, y))
        })
}

#[test]
fn only_native_ink_gets_one_channel_step_of_tolerance() {
    let bounds = Rect {
        x: 0,
        y: 0,
        width: 2,
        height: 1,
    };
    let ink = [Rect { width: 1, ..bounds }];
    let expected = [0; 8];
    assert_eq!(
        mismatch(&[1, 0, 0, 0, 0, 0, 0, 0], &expected, 2, bounds, &ink),
        None
    );
    assert_eq!(
        mismatch(&[2, 0, 0, 0, 0, 0, 0, 0], &expected, 2, bounds, &ink),
        Some((0, 0))
    );
    assert_eq!(
        mismatch(&[0, 0, 0, 0, 1, 0, 0, 0], &expected, 2, bounds, &ink),
        Some((1, 0))
    );
}
