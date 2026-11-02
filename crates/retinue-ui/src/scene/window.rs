use super::Scene;
use crate::{
    Key,
    semantics::{WindowAction, WindowCapabilities},
};

impl Scene {
    pub fn window_action(
        &self,
        point: (f64, f64),
        capabilities: WindowCapabilities,
    ) -> Option<(Key, WindowAction)> {
        let (_, geometry) = self
            .geometry
            .iter()
            .enumerate()
            .rev()
            .find(|(index, node)| {
                self.projection[*index].hit_enabled
                    && node.hit.contains(point.0, point.1)
                    && (node.interactive || node.window_action.is_some())
            })?;
        geometry
            .window_action
            .filter(|action| capabilities.supports(*action))
            .map(|action| (geometry.key, action))
    }
}
