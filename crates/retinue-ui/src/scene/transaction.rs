use super::{
    Change, LayoutExecution, Scene, execution,
    layout::{self, Subtree},
};

pub(crate) struct Admitted {
    pub completion: execution::Update,
}

impl Change {
    pub(crate) fn key(&self) -> crate::Key {
        match self {
            Self::Offset(key, _)
            | Self::Paint(key, _)
            | Self::Layout(key, _, _)
            | Self::LayoutValues(key, _)
            | Self::Placement(key, _)
            | Self::LayoutKey(key, _)
            | Self::Clip(key, _)
            | Self::Interactive(key, _)
            | Self::HitTesting(key, _)
            | Self::Role(key, _)
            | Self::WindowAction(key, _)
            | Self::Reuse(key, _) => *key,
        }
    }
}

impl Scene {
    pub(crate) fn transact(
        &mut self,
        roots: Vec<Subtree>,
        changes: &mut Vec<Change>,
        size: (u32, u32),
        scale: f32,
        execution: Option<&LayoutExecution>,
    ) -> Result<Admitted, &'static str> {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Reconcile);
        self.ensure_idle()?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("composition scale must be positive and finite");
        }
        self.root.ok_or("composition has no root")?;
        let replacement = if roots.is_empty() {
            None
        } else {
            Some(self.tree.prepare_replacement(roots)?)
        };
        for change in changes.iter() {
            let exists = replacement.as_ref().map_or_else(
                || self.tree.slot(change.key()).is_some(),
                |replacement| replacement.contains(&self.tree, change.key()),
            );
            if !exists {
                return Err("unknown composition key");
            }
            if matches!(change, Change::Layout(_, style, _) if !layout::Tree::supports_style(style))
            {
                return Err("unsupported layout calc handle");
            }
            if matches!(change, Change::Offset(_, (x, y)) if !x.is_finite() || !y.is_finite()) {
                return Err("composition offset must be finite");
            }
        }
        let structural = replacement.is_some();
        if let Some(replacement) = replacement {
            self.tree.apply_replacement(replacement);
        } else {
            self.tree.state.changed.clear();
            self.tree.topology_changed = false;
        }
        self.tree.state.computed = 0;
        let relayout = self.apply_changes(changes.drain(..))
            | structural
            | self.layout_unpublished
            | (self.frame.size() != size)
            | (self.frame.scale() != scale);
        // Inputs are accepted here. Failure below belongs to execution, not admission.
        Ok(Admitted {
            completion: self.finish_patch(relayout, size, scale, execution),
        })
    }
}
