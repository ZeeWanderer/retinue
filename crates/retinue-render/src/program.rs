use super::{
    Bounds, Command, Key, OptimizationContext, PaintKey, analysis::Analysis, optimize,
    paint::Pixels,
};
use crate::resources::ResourceId;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    sync::Arc,
};

mod discovery;
mod hash;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Compilation {
    Source,
    Authored,
    Discovered,
    Opaque,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct ProgramId(ResourceId);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct RasterSlot {
    pub owner: Key,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct Signature {
    pub content: PaintKey,
    pub slot: RasterSlot,
    pub program: ProgramId,
}

#[derive(Clone)]
pub struct PaintProgram {
    key: PaintKey,
    data: Rc<Program>,
}

struct Program {
    id: ProgramId,
    compilation: Compilation,
    fingerprint: u64,
    size: (f32, f32),
    scale: f32,
    commands: Rc<Vec<Command>>,
    optimized: Rc<Vec<Command>>,
    analysis: Analysis,
    summary: optimize::Summary,
    cost: [crate::cost::Density; 2],
    calls: Vec<FragmentCall>,
    logical_bounds: Bounds,
    acceleration: RefCell<Acceleration>,
}

#[derive(Default)]
struct Acceleration {
    lifetime: Option<Arc<()>>,
    index: Option<Rc<super::replay_index::ReplayIndex>>,
}

pub(super) struct AnalysisTarget(Weak<Program>);

impl AnalysisTarget {
    pub fn publish(&self, id: ProgramId, index: super::replay_index::ReplayIndex) -> bool {
        let Some(program) = self.0.upgrade().filter(|p| p.id == id) else {
            return false;
        };
        let mut acceleration = program.acceleration.borrow_mut();
        if acceleration.index.is_some() {
            return false;
        }
        acceleration.index = Some(Rc::new(index));
        acceleration.lifetime = None;
        true
    }
}

#[derive(Clone)]
pub(super) struct FragmentCall {
    pub program: PaintProgram,
    pub context: OptimizationContext,
    pub origin: (i32, i32),
    pub clip: Option<Pixels>,
}

#[derive(Default)]
pub struct ProgramStore {
    entries: HashMap<(u64, Compilation), Vec<Weak<Program>>>,
    prune_at: usize,
    pub compiled: u64,
    pub reused: u64,
}

impl ProgramStore {
    pub fn record(
        &mut self,
        key: impl Into<PaintKey>,
        size: (f32, f32),
        scale: f32,
        commands: Rc<Vec<Command>>,
    ) -> Result<PaintProgram, &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("paint scale must be positive and finite");
        }
        if !size.0.is_finite() || !size.1.is_finite() || size.0 < 0.0 || size.1 < 0.0 {
            return Err("paint size must be finite and nonnegative");
        }
        Ok(self.compile(key.into(), size, scale, commands))
    }

    /// Compile validated geometry; `record` also validates arbitrary input dimensions.
    pub fn compile(
        &mut self,
        key: PaintKey,
        size: (f32, f32),
        scale: f32,
        commands: Rc<Vec<Command>>,
    ) -> PaintProgram {
        self.intern(key, size, scale, commands, Compilation::Source)
    }

    fn intern(
        &mut self,
        key: PaintKey,
        size: (f32, f32),
        scale: f32,
        commands: Rc<Vec<Command>>,
        compilation: Compilation,
    ) -> PaintProgram {
        let commands = self.canonicalize(&commands, scale);
        if self.entries.len() >= self.prune_at.max(64) {
            self.entries.retain(|_, versions| {
                versions.retain(|p| p.strong_count() != 0);
                !versions.is_empty()
            });
            self.prune_at = self.entries.len().saturating_mul(2).max(64);
        }
        let hash = hash::fingerprint(&commands, size, scale);
        let versions = self.entries.entry((hash, compilation)).or_default();
        versions.retain(|p| p.strong_count() != 0);
        for data in versions.iter().filter_map(Weak::upgrade) {
            if data.size == size
                && data.scale == scale
                && (Rc::ptr_eq(&data.commands, &commands) || data.commands == commands)
            {
                self.reused += 1;
                return PaintProgram { key, data };
            }
        }
        let (optimized, mut summary) = optimize::compile(&commands, scale);
        let optimized = if matches!(compilation, Compilation::Source | Compilation::Authored) {
            let discovered = self.discover(&optimized, scale, compilation == Compilation::Source);
            if !Rc::ptr_eq(&discovered, &optimized) {
                let (_, discovered_summary) = optimize::compile(&discovered, scale);
                summary.work = discovered_summary.work;
                summary.direct_work = discovered_summary.direct_work;
            }
            discovered
        } else {
            optimized
        };
        let analysis = Analysis::new(&commands, scale);
        let area = crate::cost::area(analysis.effects.writes);
        let cost = crate::cost::summarize(&optimized, scale).map(|work| work.density(area));
        let mut calls = Vec::new();
        collect_calls(&optimized, scale, (0, 0), None, &mut calls);
        let logical_bounds = commands
            .iter()
            .map(Command::bounds)
            .reduce(Bounds::union)
            .unwrap_or_default();
        let data = Rc::new(Program {
            id: ProgramId(ResourceId::next()),
            compilation,
            fingerprint: hash,
            size,
            scale,
            commands,
            optimized,
            analysis,
            summary,
            cost,
            calls,
            logical_bounds,
            acceleration: RefCell::default(),
        });
        self.entries
            .entry((hash, compilation))
            .or_default()
            .push(Rc::downgrade(&data));
        self.compiled += 1;
        PaintProgram { key, data }
    }

    fn canonicalize(&mut self, commands: &Rc<Vec<Command>>, scale: f32) -> Rc<Vec<Command>> {
        let mut changed = None;
        for (index, command) in commands.iter().enumerate() {
            let replacement = match command {
                Command::Fragment { program, context } => {
                    let interned = self.intern(
                        program.key,
                        program.data.size,
                        scale,
                        program.commands().clone(),
                        if context.get::<super::CacheHint>() == Some(&super::CacheHint::Dynamic) {
                            Compilation::Opaque
                        } else if program.discovered() {
                            Compilation::Discovered
                        } else {
                            Compilation::Authored
                        },
                    );
                    (interned.id() != program.id()).then(|| Command::Fragment {
                        program: interned,
                        context: context.clone(),
                    })
                }
                Command::Translate { offset, commands } => {
                    let children = self.canonicalize(commands, scale);
                    (!Rc::ptr_eq(commands, &children)).then_some(Command::Translate {
                        offset: *offset,
                        commands: children,
                    })
                }
                Command::Clip { bounds, commands } => {
                    let children = self.canonicalize(commands, scale);
                    (!Rc::ptr_eq(commands, &children)).then_some(Command::Clip {
                        bounds: *bounds,
                        commands: children,
                    })
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                changed.get_or_insert_with(|| commands.as_ref().clone())[index] = replacement;
            }
        }
        changed.map(Rc::new).unwrap_or_else(|| commands.clone())
    }
}

impl PaintProgram {
    pub(super) fn analysis_target(&self) -> (AnalysisTarget, std::sync::Weak<()>) {
        let mut acceleration = self.data.acceleration.borrow_mut();
        (
            AnalysisTarget(Rc::downgrade(&self.data)),
            Arc::downgrade(acceleration.lifetime.get_or_insert_with(|| Arc::new(()))),
        )
    }

    pub(super) fn has_replay_index(&self) -> bool {
        self.data.acceleration.borrow().index.is_some()
    }

    pub(super) fn replay(
        &self,
        painter: &mut crate::painter::Painter<'_>,
        origin: (i32, i32),
        external: &mut impl FnMut(Key, &mut crate::painter::Painter<'_>),
        compiled: bool,
        cull: bool,
        stats: &mut super::PaintStats,
        fragments: &mut impl FnMut(
            &PaintProgram,
            &OptimizationContext,
        ) -> Option<Rc<crate::painter::RasterImage>>,
    ) {
        let commands = if compiled {
            self.optimized()
        } else {
            self.commands()
        };
        let clip = painter.visible_bounds();
        let mut replay = |range: std::ops::Range<usize>| {
            for command in &commands[range] {
                command.paint(
                    painter,
                    self.scale(),
                    origin,
                    external,
                    compiled,
                    cull,
                    stats,
                    fragments,
                );
            }
        };
        let index = self.data.acceleration.borrow().index.clone();
        if let Some(index) = index.as_ref().filter(|_| compiled && cull) {
            let (nodes, skipped) = index.visit(origin, clip, &mut replay);
            stats.indexed_nodes += nodes;
            stats.indexed_commands_skipped += skipped;
            stats.culled_commands += skipped;
        } else {
            replay(0..commands.len());
        }
    }

    /// Commands use local logical coordinates. Change the key's revision when
    /// a retained frontend's content changes. Immutable resource versions are
    /// captured by the program, never inferred from allocator addresses.
    pub fn new(
        key: impl Into<PaintKey>,
        size: (f32, f32),
        scale: f32,
        commands: Rc<Vec<Command>>,
    ) -> Result<Self, &'static str> {
        ProgramStore::default().record(key, size, scale, commands)
    }

    pub fn key(&self) -> PaintKey {
        self.key
    }
    pub fn id(&self) -> ProgramId {
        self.data.id
    }
    pub(super) fn fingerprint(&self) -> u64 {
        self.data.fingerprint
    }
    pub(super) fn source_eq(&self, other: &Self) -> bool {
        self.id() == other.id()
            || (self.data.fingerprint == other.data.fingerprint
                && self.data.size == other.data.size
                && self.scale() == other.scale()
                && self.commands() == other.commands())
    }
    pub fn commands(&self) -> &Rc<Vec<Command>> {
        &self.data.commands
    }
    pub fn scale(&self) -> f32 {
        self.data.scale
    }
    pub fn size(&self) -> (f32, f32) {
        self.data.size
    }
    pub fn analysis(&self) -> &Analysis {
        &self.data.analysis
    }
    pub(super) fn logical_bounds(&self) -> Bounds {
        self.data.logical_bounds
    }
    pub(super) fn calls(&self) -> &[FragmentCall] {
        &self.data.calls
    }

    pub fn matches(&self, key: PaintKey, size: (f32, f32), scale: f32) -> bool {
        self.key == key && self.data.size == size && self.data.scale == scale
    }

    pub(super) fn signature(&self) -> Signature {
        Signature {
            content: self.key,
            slot: RasterSlot {
                owner: self.key.owner,
                width: self.data.size.0.to_bits(),
                height: self.data.size.1.to_bits(),
                scale: self.data.scale.to_bits(),
            },
            program: self.data.id,
        }
    }
    pub fn bounds(&self) -> Pixels {
        self.data.analysis.effects.writes
    }
    pub(super) fn optimized(&self) -> &Rc<Vec<Command>> {
        &self.data.optimized
    }
    pub(super) fn summary(&self) -> optimize::Summary {
        self.data.summary
    }
    pub(super) fn cost(&self, direct: bool) -> crate::cost::Density {
        self.data.cost[usize::from(direct)]
    }
    pub(super) fn discovered(&self) -> bool {
        self.data.compilation == Compilation::Discovered
    }
}

fn collect_calls(
    commands: &[Command],
    scale: f32,
    origin: (i32, i32),
    clip: Option<Pixels>,
    calls: &mut Vec<FragmentCall>,
) {
    for command in commands {
        match command {
            Command::Fragment { program, context } => calls.push(FragmentCall {
                program: program.clone(),
                context: context.clone(),
                origin,
                clip,
            }),
            Command::Translate { offset, commands } => collect_calls(
                commands,
                scale,
                (
                    origin.0.saturating_add((offset.0 * scale).round() as i32),
                    origin.1.saturating_add((offset.1 * scale).round() as i32),
                ),
                clip,
                calls,
            ),
            Command::Clip { bounds, commands } => {
                let bounds = bounds.pixels(scale).translated(origin);
                collect_calls(
                    commands,
                    scale,
                    origin,
                    Some(clip.map_or(bounds, |c| c.intersection(bounds))),
                    calls,
                );
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests;
