use super::{PaintFrame, PaintStats, ProgramId, text::preparation::PreparedFonts};
use std::{collections::BTreeSet, sync::Arc};
mod content;

#[derive(Default)]
pub(super) struct Dependencies {
    frame: Option<(crate::frame::Identity, u64)>,
    programs: Vec<ProgramId>,
    pub content: content::ContentIndex,
    pub changed: Vec<usize>,
    pub groups: Vec<usize>,
    pub fragment_roots: BTreeSet<usize>,
    pub visited: usize,
    pub paints: usize,
    pub full: bool,
}

impl Dependencies {
    pub fn has_program(&self, index: usize, program: ProgramId) -> bool {
        self.programs
            .get(index)
            .is_some_and(|entry| *entry == program)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn update(
        &mut self,
        frame: &PaintFrame,
        fonts: &mut Arc<PreparedFonts>,
        stats: &mut PaintStats,
        retry_fonts: bool,
    ) {
        self.changed.clear();
        self.groups.clear();
        self.visited = 0;
        self.paints = 0;
        self.full = false;
        let previous = self
            .frame
            .as_ref()
            .filter(|(id, _)| *id == frame.identity)
            .map(|(_, revision)| *revision);
        if previous == Some(frame.generation) {
            if retry_fonts && fonts.has_failures() {
                self.content
                    .update(frame, Some(&Default::default()), fonts, stats);
            }
            return;
        }
        let changes = previous.and_then(|generation| frame.changes_since(generation));
        let full = changes.as_ref().is_none_or(|changes| changes.topology)
            || self.programs.len() != frame.chunks.len();
        self.full = full;
        self.content.update(frame, changes.as_deref(), fonts, stats);
        if full {
            self.programs.clear();
            self.fragment_roots.clear();
            for (index, instance) in frame.chunks.iter().enumerate() {
                if !instance.program.calls().is_empty() {
                    self.fragment_roots.insert(index);
                }
                self.programs.push(instance.program.id());
                self.changed.push(index);
            }
            self.paints = changes
                .as_deref()
                .and_then(|c| c.content.as_ref())
                .map_or(frame.chunks.len(), |e| e.written.len());
        } else {
            let changes = changes.as_ref().unwrap();
            self.groups.extend(&changes.groups);
            self.changed.extend(&changes.instances);
            self.changed.sort_unstable();
            self.changed.dedup();
            for n in 0..self.changed.len() {
                let index = self.changed[n];
                let instance = &frame.chunks[index];
                if self.programs[index] != instance.program.id() {
                    if instance.program.calls().is_empty() {
                        self.fragment_roots.remove(&index);
                    } else {
                        self.fragment_roots.insert(index);
                    }
                    self.programs[index] = instance.program.id();
                    self.paints += 1;
                }
            }
        }
        self.visited = self.changed.len();
        self.frame = Some((frame.identity, frame.generation));
    }
}
