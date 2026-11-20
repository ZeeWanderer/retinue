use super::*;

impl<P> Template<P> {
    fn compact_sources(&mut self) {
        if self.sources.iter().all(|s| !s.bindings.is_empty()) {
            return;
        }
        if !self.groups.is_empty() {
            let mut offsets = Vec::with_capacity(self.sources.len() + 1);
            let mut count = 0;
            for source in &self.sources {
                offsets.push(count);
                count += usize::from(!source.bindings.is_empty());
            }
            offsets.push(count);
            let mut groups = std::mem::take(&mut self.groups).into_vec();
            groups.retain_mut(|group| {
                group.range = offsets[group.range.start]..offsets[group.range.end];
                group.range.len() >= 2
            });
            self.groups = groups.into_boxed_slice();
        }
        let mut sources = std::mem::take(&mut self.sources).into_vec();
        sources.retain(|s| !s.bindings.is_empty());
        self.sources = sources.into_boxed_slice();
    }

    pub(crate) fn with_source_groups(mut self, groups: Vec<SourceGroup<P>>) -> Result<Self> {
        let mut end = 0;
        for group in &groups {
            if group.range.start < end
                || group.range.is_empty()
                || group.range.end > self.sources.len()
            {
                return Err("invalid comparison group");
            }
            end = group.range.end;
        }
        self.groups = groups.into_boxed_slice();
        self.compact_sources();
        Ok(self)
    }
}
