use std::{any::type_name, fmt, marker::PhantomData};

pub trait EnvironmentKey: 'static {
    type Value: Clone + 'static;
    fn default_value() -> Self::Value;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingModel {
    pub name: &'static str,
}

impl MissingModel {
    pub(super) fn of<T>() -> Self {
        Self {
            name: type_name::<T>(),
        }
    }
}

impl fmt::Display for MissingModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no environment provider for {}", self.name)
    }
}

impl std::error::Error for MissingModel {}

pub(super) struct Model<T>(PhantomData<T>);

impl<T: Clone + 'static> EnvironmentKey for Model<T> {
    type Value = Option<T>;

    fn default_value() -> Self::Value {
        None
    }
}
