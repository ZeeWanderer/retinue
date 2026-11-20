use super::*;

impl ProgramStore {
    pub(super) fn discover(
        &mut self,
        commands: &Arc<Vec<Command>>,
        scale: f32,
        wrap: bool,
    ) -> Arc<Vec<Command>> {
        let mut changed = None;
        for (index, command) in commands.iter().enumerate() {
            let replacement = match command {
                Command::Translate { offset, commands } => {
                    let children = self.discover(commands, scale, true);
                    (!Arc::ptr_eq(commands, &children)).then_some(Command::Translate {
                        offset: *offset,
                        commands: children,
                    })
                }
                Command::Clip { bounds, commands } => {
                    let children = self.discover(commands, scale, true);
                    (!Arc::ptr_eq(commands, &children)).then_some(Command::Clip {
                        bounds: *bounds,
                        commands: children,
                    })
                }
                Command::Fragment { .. } | Command::External { .. } => None,
                _ if commands.len() > 1 && optimize::leaf_work(command) >= 8 => {
                    self.candidate(Arc::new(vec![command.clone()]), scale)
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                changed.get_or_insert_with(|| commands.as_ref().clone())[index] = replacement;
            }
        }
        let commands = changed.map(Arc::new).unwrap_or_else(|| commands.clone());
        if wrap {
            self.candidate(commands.clone(), scale)
                .map(|fragment| Arc::new(vec![fragment]))
                .unwrap_or(commands)
        } else {
            commands
        }
    }

    fn candidate(&mut self, commands: Arc<Vec<Command>>, scale: f32) -> Option<Command> {
        let cost = estimate(&commands);
        let bounds = commands.iter().map(Command::bounds).reduce(Bounds::union)?;
        let area = f64::from(bounds.width) * f64::from(bounds.height);
        // Blitting a large sparse rectangle can cost more than replaying its few marks.
        if cost.barrier
            || cost.direct < 8
            || !area.is_finite()
            || area <= 0.0
            || cost.pixels < area * 4.0
        {
            return None;
        }
        let program = self.intern(
            0.into(),
            (bounds.width, bounds.height),
            scale,
            commands,
            Compilation::Discovered,
        );
        Some(Command::Fragment {
            program,
            context: OptimizationContext::default(),
        })
    }
}

#[derive(Default)]
struct Estimate {
    direct: usize,
    pixels: f64,
    barrier: bool,
}

fn estimate(commands: &[Command]) -> Estimate {
    let mut cost = Estimate::default();
    for command in commands {
        match command {
            Command::Fragment { program, .. } => cost.barrier |= program.summary().external,
            Command::External { .. } => cost.barrier = true,
            Command::Translate { commands, .. } | Command::Clip { commands, .. } => {
                let nested = estimate(commands);
                cost.direct = cost.direct.saturating_add(nested.direct);
                cost.pixels += nested.pixels;
                cost.barrier |= nested.barrier;
            }
            _ => {
                let work = optimize::leaf_work(command);
                let bounds = command.bounds();
                cost.direct = cost.direct.saturating_add(work);
                cost.pixels += work as f64 * f64::from(bounds.width) * f64::from(bounds.height);
            }
        }
    }
    cost
}
