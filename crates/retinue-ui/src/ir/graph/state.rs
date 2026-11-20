use super::*;
use std::any::Any;

struct Lease<'a> {
    graph: &'a mut InstanceGraph,
    handle: Handle,
    state: Option<Box<dyn Any>>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            self.graph
                .banks
                .get_mut(&self.handle.bank)
                .unwrap()
                .restore_state(self.handle, state);
        }
    }
}

impl InstanceGraph {
    /// The allocation stays with the instance; nested compilation only borrows it.
    pub(crate) fn with_state<T: Default + 'static, R>(
        &mut self,
        handle: Handle,
        action: impl FnOnce(&mut Self, &mut T) -> Result<R>,
    ) -> Result<R> {
        let state = self
            .banks
            .get_mut(&handle.bank)
            .ok_or("instance belongs to another graph")?
            .take_state(handle)?
            .unwrap_or_else(|| Box::<T>::default());
        let mut lease = Lease {
            graph: self,
            handle,
            state: Some(state),
        };
        let state = lease
            .state
            .as_mut()
            .unwrap()
            .downcast_mut::<T>()
            .ok_or("owner state type changed")?;
        action(lease.graph, state)
    }
}
