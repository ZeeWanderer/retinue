use crate::environment::EnvironmentKey;

mod color;
mod keyframes;
mod playback;
#[cfg(any(test, feature = "reference-frontend"))]
pub(crate) mod presentation;
#[cfg(any(test, feature = "reference-frontend"))]
mod style;
#[cfg(any(test, feature = "reference-frontend"))]
pub(crate) mod switch;

pub use crate::transition::{Easing, Interpolate};
pub use keyframes::{Keyframes, MotionProgram, Parallel};
pub use playback::{Completion, Playback, PlaybackOwner, PlaybackStatus, Run};
#[cfg(any(test, feature = "reference-frontend"))]
pub use switch::TranslationTransition;

pub trait Animatable: PartialEq + 'static {
    type Data: Interpolate;
    fn animatable_data(&self) -> Self::Data;
}

pub struct ReduceMotion;
impl EnvironmentKey for ReduceMotion {
    type Value = bool;
    fn default_value() -> bool {
        false
    }
}

#[cfg(any(test, feature = "reference-frontend"))]
mod scope;
#[cfg(any(test, feature = "reference-frontend"))]
pub(crate) use scope::AnimationScope;
