//! Inherited, independently observed providers for UI values and model handles.

mod key;
mod scope;

pub use key::{EnvironmentKey, MissingModel};
pub use scope::Environment;

#[cfg(test)]
mod tests;
