use super::{Command, paint::Pixels};
use std::rc::Rc;

#[derive(Clone, Copy, Default)]
pub(super) struct Summary {
    pub removed: usize,
    pub work: usize,
    pub direct_work: usize,
    pub external: bool,
}

pub(super) fn leaf_work(command: &Command) -> usize {
    match command {
        Command::Text { text, .. } => text.chars().count().min(1024),
        Command::InsetShadow { .. } => 16,
        Command::Polygon { points, .. } => points.len(),
        _ => 1,
    }
}

pub(super) fn compile(commands: &Rc<Vec<Command>>, scale: f32) -> (Rc<Vec<Command>>, Summary) {
    let mut summary = Summary::default();
    let mut changes = Vec::new();
    let mut covers = Vec::<Pixels>::new();
    for (index, command) in commands.iter().enumerate().rev() {
        let removable = matches!(
            command,
            Command::Fill { .. } | Command::Text { .. } | Command::Image { .. }
        );
        let bounds = if removable {
            command.bounds().pixels(scale)
        } else {
            Pixels::default()
        };
        let transparent = match command {
            Command::Fill { color, .. }
            | Command::Text { color, .. }
            | Command::InsetShadow { color, .. } => color.bgra()[3] == 0,
            _ => false,
        };
        if transparent
            || (removable
                && (bounds.width == 0
                    || bounds.height == 0
                    || covers.iter().any(|cover| contains(*cover, bounds))))
        {
            summary.removed += 1;
            changes.push((index, Vec::new()));
            continue;
        }
        if let Command::Fill { radius, color, .. } = command
            && *radius == 0.0
            && color.bgra()[3] == 255
            && covers.len() < 16
        {
            covers.push(bounds);
        }
        match command {
            Command::Fragment { program, .. } => {
                let nested = program.summary();
                summary.work += nested.work;
                summary.external |= nested.external;
                if nested.external {
                    covers.clear();
                }
            }
            Command::Clip { commands, .. } | Command::Translate { commands, .. } => {
                let empty_clip = matches!(command, Command::Clip { bounds, .. }
                    if bounds.width <= 0.0 || bounds.height <= 0.0);
                if empty_clip {
                    summary.removed += 1;
                    changes.push((index, Vec::new()));
                    continue;
                }
                let (children, nested) = compile(commands, scale);
                summary.removed += nested.removed;
                summary.work += nested.work;
                summary.direct_work += nested.direct_work;
                summary.external |= nested.external;
                if nested.external {
                    covers.clear();
                }
                match command {
                    Command::Translate { offset, .. } if *offset == (0.0, 0.0) => {
                        changes.push((index, children.as_ref().clone()));
                        summary.removed += 1;
                    }
                    Command::Translate { offset, .. } if nested.removed != 0 => changes.push((
                        index,
                        vec![Command::Translate {
                            offset: *offset,
                            commands: children,
                        }],
                    )),
                    Command::Clip { bounds, .. } if nested.removed != 0 => changes.push((
                        index,
                        vec![Command::Clip {
                            bounds: *bounds,
                            commands: children,
                        }],
                    )),
                    _ => {}
                }
            }
            _ => {
                summary.external |= matches!(command, Command::External { .. });
                if matches!(command, Command::External { .. }) {
                    covers.clear();
                }
                summary.work += leaf_work(command);
                summary.direct_work += leaf_work(command);
            }
        }
    }
    let compiled = if changes.is_empty() {
        commands.clone()
    } else {
        let mut output = Vec::with_capacity(commands.len());
        for (index, command) in commands.iter().enumerate() {
            if changes.last().is_some_and(|(changed, _)| *changed == index) {
                output.extend(changes.pop().unwrap().1);
            } else {
                output.push(command.clone());
            }
        }
        Rc::new(output)
    };
    (compiled, summary)
}

fn contains(outer: Pixels, inner: Pixels) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && i64::from(inner.x) + i64::from(inner.width)
            <= i64::from(outer.x) + i64::from(outer.width)
        && i64::from(inner.y) + i64::from(inner.height)
            <= i64::from(outer.y) + i64::from(outer.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rect, painter::Painter, *};

    fn fill(radius: f32, alpha: u8) -> Command {
        Command::Fill {
            bounds: Bounds::new(1.25, 2.25, 36.5, 32.5),
            radius,
            color: Color::rgba(60, 80, 90, alpha),
        }
    }

    #[test]
    fn coverage_never_assumes_rounded_or_translucent_bounds_are_opaque() {
        for top in [fill(4.0, 255), fill(0.0, 200)] {
            let commands = Rc::new(vec![fill(0.0, 255), top]);
            let (compiled, summary) = compile(&commands, 1.25);
            assert_eq!(summary.removed, 0);
            assert!(Rc::ptr_eq(&commands, &compiled));
        }
        let (_, summary) = compile(&Rc::new(vec![fill(4.0, 255), fill(0.0, 255)]), 1.25);
        assert_eq!(summary.removed, 1);
    }

    #[test]
    fn external_callbacks_are_an_observation_barrier() {
        let external = Command::External {
            bounds: Bounds::new(0.0, 0.0, 40.0, 40.0),
            key: 1.into(),
        };
        for external in [
            external.clone(),
            Command::Clip {
                bounds: Bounds::new(0.0, 0.0, 40.0, 40.0),
                commands: Rc::new(vec![external]),
            },
        ] {
            let commands = Rc::new(vec![fill(0.0, 255), external, fill(0.0, 255)]);
            let (compiled, summary) = compile(&commands, 1.0);
            assert!(summary.external);
            assert_eq!(summary.removed, 0);
            assert!(Rc::ptr_eq(&commands, &compiled));
        }
    }

    #[test]
    fn compiled_coverage_and_identity_scopes_match_raw_pixels() {
        let commands = Rc::new(vec![
            fill(4.0, 255),
            fill(0.0, 255),
            Command::Clip {
                bounds: Bounds::new(0.0, 0.0, 0.0, 40.0),
                commands: Rc::new(vec![fill(0.0, 255)]),
            },
            Command::Translate {
                offset: (0.0, 0.0),
                commands: Rc::new(vec![
                    fill(4.0, 0),
                    Command::Translate {
                        offset: (0.35, 0.35),
                        commands: Rc::new(vec![Command::Translate {
                            offset: (0.35, 0.35),
                            commands: Rc::new(vec![fill(4.0, 128)]),
                        }]),
                    },
                ]),
            },
        ]);
        for scale in [1.0, 1.2, 1.25, 2.0] {
            let clip = Rect {
                width: 128,
                height: 128,
                ..Rect::default()
            };
            let program = PaintProgram::new(0, (60.0, 60.0), scale, commands.clone()).unwrap();
            let mut frame = PaintFrame::default();
            frame
                .replace(
                    (128, 128),
                    scale,
                    vec![PaintInstance::new(0.into(), program, (0, 0), clip)],
                    vec![],
                )
                .unwrap();
            let mut outputs = Vec::new();
            for optimized in [false, true] {
                let mut r = Renderer::new(crate::ResidencyPolicy::desktop(0));
                r.set_optimizations(optimized);
                r.prepare(&frame, RasterMode::Replay);
                let mut pixels = vec![0; 128 * 128 * 4];
                let mut painter = Painter::new(&mut pixels, 128, 128).unwrap();
                r.paint(
                    &frame,
                    &mut painter,
                    Rect {
                        width: 128,
                        height: 128,
                        ..Default::default()
                    },
                    Color::rgb(0x112233),
                );
                painter.finish().unwrap();
                if optimized {
                    assert!(r.stats.simplified_commands >= 4);
                }
                outputs.push(pixels);
            }
            assert_eq!(outputs[0], outputs[1], "scale={scale}");
        }
    }
}
