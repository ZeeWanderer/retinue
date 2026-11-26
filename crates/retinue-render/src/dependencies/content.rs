use crate::{
    Key, PaintFrame, PaintInstance, PaintStats, ProgramId, ResourceRead,
    frame::Changes,
    text::preparation::{FontKey, FontRequest, PreparedFonts},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

// Membership follows recorded content, not painter order or screen position.
#[derive(Default)]
pub(crate) struct ContentIndex {
    members: FxHashMap<Key, Member>,
    reads: FxHashMap<ResourceRead, FxHashSet<Key>>,
    fonts: FxHashMap<FontKey, (usize, FontRequest)>,
}

struct Member {
    program: ProgramId,
    reads: Box<[ResourceRead]>,
    fonts: Box<[FontKey]>,
}

impl ContentIndex {
    pub fn consumers(&self, read: ResourceRead) -> impl Iterator<Item = Key> + '_ {
        self.reads
            .get(&read)
            .into_iter()
            .flat_map(|v| v.iter().copied())
    }

    fn remove(&mut self, key: Key, dirty: &mut bool) {
        let Some(p) = self.members.remove(&key) else {
            return;
        };
        for r in &p.reads {
            let set = self.reads.get_mut(r).unwrap();
            set.remove(&key);
            if set.is_empty() {
                self.reads.remove(r);
            }
        }
        for f in &p.fonts {
            let count = &mut self.fonts.get_mut(f).unwrap().0;
            *count -= 1;
            if *count == 0 {
                self.fonts.remove(f);
                *dirty = true;
            }
        }
    }

    fn write(&mut self, c: &PaintInstance, dirty: &mut bool) {
        if self
            .members
            .get(&c.key)
            .is_some_and(|p| p.program == c.program.id())
        {
            return;
        }
        self.remove(c.key, dirty);
        if c.program.fonts().is_empty() && c.program.analysis().resources.is_empty() {
            return;
        }
        for r in &c.program.analysis().resources {
            self.reads.entry(*r).or_default().insert(c.key);
        }
        for f in c.program.fonts() {
            self.fonts
                .entry(f.key())
                .or_insert_with(|| {
                    *dirty = true;
                    (0, f.clone())
                })
                .0 += 1;
        }
        self.members.insert(
            c.key,
            Member {
                program: c.program.id(),
                reads: c.program.analysis().resources.as_slice().into(),
                fonts: c.program.fonts().iter().map(FontRequest::key).collect(),
            },
        );
    }

    pub fn update(
        &mut self,
        frame: &PaintFrame,
        changes: Option<&Changes>,
        fonts: &mut Arc<PreparedFonts>,
        stats: &mut PaintStats,
    ) {
        let mut dirty = fonts.has_failures();
        if let Some(edits) = changes.and_then(|c| c.content.as_ref()) {
            for &key in &edits.removed {
                self.remove(key, &mut dirty);
            }
            for &i in &edits.written {
                self.write(&frame.chunks[i], &mut dirty);
            }
            stats.content_visits = edits.removed.len() + edits.written.len();
        } else if let Some(changes) = changes.filter(|c| !c.topology) {
            for &i in &changes.instances {
                self.write(&frame.chunks[i], &mut dirty);
            }
            stats.content_visits = changes.instances.len();
        } else {
            self.members.clear();
            self.fonts.clear();
            self.reads.clear();
            for c in frame.instances() {
                self.write(c, &mut dirty);
            }
            stats.content_visits = frame.chunks.len();
            dirty = true;
        }
        if dirty {
            let (next, prepared) = fonts.update_requests(self.fonts.values().map(|(_, f)| f));
            *fonts = Arc::new(next);
            stats.prepared_fonts = prepared;
        }
    }
}
