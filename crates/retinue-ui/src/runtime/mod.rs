//! Shared view identities, update counters and paint surfaces.

#[cfg(any(test, feature = "reference-frontend"))]
pub(crate) mod instance;
pub(crate) mod profile;
#[cfg(any(test, feature = "reference-frontend"))]
pub mod reference;
mod surface;

pub use crate::scene::LayoutExecution;
#[cfg(feature = "profiling")]
pub use profile::UpdateProfile;
#[cfg(any(test, feature = "reference-frontend"))]
pub use reference::{AmbiguousKey, ViewInfo, ViewTree};
pub use surface::ViewSurface;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(pub u64);

impl ViewId {
    pub fn key(self) -> crate::Key {
        crate::Key::new("retinue-ui-view", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateStats {
    pub bodies: usize,
    pub mounted: usize,
    pub unmounted: usize,
    pub projected: usize,
    pub invalid_deadlines: usize,
    pub motion_samples: usize,
    pub input_updates: usize,
}
