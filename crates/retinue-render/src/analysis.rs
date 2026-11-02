use super::{Command, paint::Pixels};
use crate::resources::ResourceId;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceRead {
    Image(ResourceId),
    Font(ResourceId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub writes: Pixels,
    pub reads: Pixels,
    pub barrier: bool,
}

impl Effects {
    pub fn independent(self, other: Self) -> bool {
        !self.barrier
            && !other.barrier
            && self.writes.intersection(other.writes).is_empty()
            && self.reads.intersection(other.writes).is_empty()
            && other.reads.intersection(self.writes).is_empty()
    }

    fn union(self, other: Self) -> Self {
        Self {
            writes: self.writes.union(other.writes),
            reads: self.reads.union(other.reads),
            barrier: self.barrier || other.barrier,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub effects: Effects,
    pub resources: Vec<ResourceRead>,
    pub operations: usize,
    pub retained_bytes: usize,
    pub(crate) operations_effects: Vec<Effects>,
}

impl Analysis {
    pub(super) fn new(commands: &[Command], scale: f32) -> Self {
        let mut resources = Vec::new();
        let mut operations = 0;
        let operations_effects: Vec<_> = commands
            .iter()
            .map(|c| analyze(c, scale, &mut resources, &mut operations))
            .collect();
        let effects = operations_effects
            .iter()
            .copied()
            .fold(Effects::default(), Effects::union);
        resources.sort_unstable();
        resources.dedup();
        Self {
            effects,
            resources,
            operations,
            retained_bytes: commands.iter().map(Command::retained_bytes).sum(),
            operations_effects,
        }
    }
}

pub(super) fn changed_writes(
    before: Option<&Command>,
    after: Option<&Command>,
    scale: f32,
) -> Pixels {
    let children = |before: &[Command], after: &[Command]| {
        (0..before.len().max(after.len())).fold(Pixels::default(), |damage, index| {
            damage.union(changed_writes(before.get(index), after.get(index), scale))
        })
    };
    match (before, after) {
        (Some(a), Some(b)) if a.same_paint(b) => Pixels::default(),
        (
            Some(Command::Clip {
                bounds: a,
                commands: before,
            }),
            Some(Command::Clip {
                bounds: b,
                commands: after,
            }),
        ) if a == b => children(before, after).intersection(a.pixels(scale)),
        (
            Some(Command::Translate {
                offset: a,
                commands: before,
            }),
            Some(Command::Translate {
                offset: b,
                commands: after,
            }),
        ) if a == b => children(before, after)
            .translated(((a.0 * scale).round() as i32, (a.1 * scale).round() as i32)),
        (
            Some(Command::Fragment { program: a, .. }),
            Some(Command::Fragment { program: b, .. }),
        ) if a.scale() == b.scale() && a.scale() == scale => children(a.commands(), b.commands()),
        _ => [before, after]
            .into_iter()
            .flatten()
            .fold(Pixels::default(), |damage, command| {
                damage.union(analyze(command, scale, &mut Vec::new(), &mut 0).writes)
            }),
    }
}

fn analyze(
    command: &Command,
    scale: f32,
    resources: &mut Vec<ResourceRead>,
    operations: &mut usize,
) -> Effects {
    *operations += 1;
    match command {
        Command::Fragment { program, .. } => {
            let analysis = program.analysis();
            resources.extend(&analysis.resources);
            *operations += analysis.operations;
            analysis.effects
        }
        Command::Translate { offset, commands } => {
            let effects = commands
                .iter()
                .map(|c| analyze(c, scale, resources, operations))
                .fold(Effects::default(), Effects::union);
            let delta = (
                (offset.0 * scale).round() as i32,
                (offset.1 * scale).round() as i32,
            );
            Effects {
                writes: effects.writes.translated(delta),
                reads: effects.reads.translated(delta),
                ..effects
            }
        }
        Command::Clip { bounds, commands } => {
            let effects = commands
                .iter()
                .map(|c| analyze(c, scale, resources, operations))
                .fold(Effects::default(), Effects::union);
            let clip = bounds.pixels(scale);
            Effects {
                writes: effects.writes.intersection(clip),
                reads: effects.reads.intersection(clip),
                ..effects
            }
        }
        _ => {
            match command {
                Command::Image { image, .. } => resources.push(ResourceRead::Image(image.id())),
                Command::Text { font, .. } => resources.push(ResourceRead::Font(font.id())),
                _ => {}
            }
            let writes = command.bounds().pixels(scale);
            let opaque = matches!(command, Command::Fill { radius, color, .. } if *radius == 0.0 && color.bgra()[3] == 255);
            Effects {
                writes,
                reads: if opaque { Pixels::default() } else { writes },
                barrier: matches!(command, Command::External { .. }),
            }
        }
    }
}
