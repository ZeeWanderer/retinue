use super::*;
use crate::Command;

pub(crate) fn leaf(command: &Command, scale: f32) -> Work {
    let bounds = command.bounds().pixels(scale);
    let pixels = area(bounds) as f64;
    match command {
        Command::Fill { radius, color, .. } => Work {
            solid: if color.bgra()[3] == 255 { pixels } else { 0.0 },
            blended: if color.bgra()[3] == 255 { 0.0 } else { pixels },
            commands: 1.0,
            edges: if *radius == 0.0 { 0.0 } else { 8.0 },
            ..Work::default()
        },
        Command::Image { .. } => Work::image(area(bounds)),
        Command::Text { text, size, .. } => {
            let glyphs = text.chars().count() as f64;
            let size = f64::from(size * scale);
            let coverage = pixels.min(glyphs * size * size * 0.6);
            Work {
                blended: coverage,
                sampled: coverage / 4.0,
                glyphs,
                commands: 1.0,
                ..Work::default()
            }
        }
        Command::Outline { thickness, .. } | Command::Ring { thickness, .. } => Work {
            blended: pixels.min(
                2.0 * (f64::from(bounds.width) + f64::from(bounds.height))
                    * f64::from(thickness * scale),
            ),
            commands: 1.0,
            edges: 8.0,
            ..Work::default()
        },
        Command::InsetShadow { blur, spread, .. } => {
            let bands = f64::from((spread.max(0.0) + 1.5 * blur.max(0.0)) * scale)
                .ceil()
                .min(f64::from(bounds.width.min(bounds.height) / 2));
            let interior =
                (f64::from(bounds.width) - 2.0 * bands) * (f64::from(bounds.height) - 2.0 * bands);
            Work {
                blended: pixels - interior,
                commands: bands,
                edges: bands * 8.0,
                ..Work::default()
            }
        }
        Command::Line { .. } => Work {
            blended: pixels,
            commands: 1.0,
            edges: 4.0,
            ..Work::default()
        },
        Command::Polygon { points, .. } => Work {
            blended: pixels,
            commands: 1.0,
            edges: points.len() as f64,
            ..Work::default()
        },
        _ => Work {
            blended: pixels,
            commands: 1.0,
            ..Work::default()
        },
    }
}

pub(crate) fn summarize(commands: &[Command], scale: f32) -> [Work; 2] {
    fn walk(
        commands: &[Command],
        scale: f32,
        origin: (i32, i32),
        clip: Option<Pixels>,
    ) -> [Work; 2] {
        let mut work = [Work::default(); 2];
        for command in commands {
            let nested = match command {
                Command::Clip { bounds, commands } => {
                    let bounds = bounds.pixels(scale).translated(origin);
                    let clip = clip.map_or(bounds, |clip| clip.intersection(bounds));
                    walk(commands, scale, origin, Some(clip))
                }
                Command::Translate { offset, commands } => {
                    let origin = (
                        origin.0.saturating_add((offset.0 * scale).round() as i32),
                        origin.1.saturating_add((offset.1 * scale).round() as i32),
                    );
                    walk(commands, scale, origin, clip)
                }
                _ => {
                    let bounds = command.bounds().pixels(scale).translated(origin);
                    let visible = area(clip.map_or(bounds, |clip| bounds.intersection(clip)));
                    match command {
                        Command::Fragment { program, .. } => {
                            [program.cost(false).work(visible), Work::default()]
                        }
                        _ => [leaf(command, scale).density(area(bounds)).work(visible); 2],
                    }
                }
            };
            work[0].add(nested[0]);
            work[1].add(nested[1]);
        }
        work
    }
    walk(commands, scale, (0, 0), None)
}
