use super::{State, untracked};
use std::{fmt, rc::Rc};

/// A writable path to existing state. Creating a path does not observe it.
pub struct Binding<T>(Access<T>);

enum Access<T> {
    State(State<T>),
    Projected(Rc<dyn Target<T>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unavailable;

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("binding target is unavailable")
    }
}

impl std::error::Error for Unavailable {}

impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Access::State(state) => Access::State(state.clone()),
            Access::Projected(target) => Access::Projected(target.clone()),
        })
    }
}

impl<T> fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding").finish_non_exhaustive()
    }
}

impl<T> From<State<T>> for Binding<T> {
    fn from(state: State<T>) -> Self {
        Self(Access::State(state))
    }
}

impl<T: 'static> Binding<T> {
    /// Resolve the current field on each access, including fields that disappear.
    pub fn resolve(resolve: impl Fn() -> Option<State<T>> + 'static) -> Self {
        Self(Access::Projected(Rc::new(Resolved(resolve))))
    }

    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> Result<R, Unavailable> {
        let mut read = Some(read);
        let mut result = None;
        self.read_into(&mut |value| result = Some(read.take().unwrap()(value)))?;
        Ok(result.unwrap())
    }

    pub fn get(&self) -> Result<T, Unavailable>
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }

    pub fn is_available(&self) -> bool {
        self.with(|_| ()).is_ok()
    }

    pub fn set(&self, value: T) -> Result<bool, Unavailable>
    where
        T: PartialEq,
    {
        let mut value = Some(value);
        let old = self.edit(|current| {
            if current == value.as_ref().unwrap() {
                (None, false)
            } else {
                (
                    Some(std::mem::replace(current, value.take().unwrap())),
                    true,
                )
            }
        })?;
        Ok(old.is_some())
    }

    pub fn replace(&self, value: T) -> Result<T, Unavailable> {
        self.edit(|current| (std::mem::replace(current, value), true))
    }

    pub fn update<R>(&self, update: impl FnOnce(&mut T) -> R) -> Result<R, Unavailable> {
        self.edit(|value| (update(value), true))
    }

    /// Project into a value; observation remains at the source field's granularity.
    pub fn map<U: 'static>(
        &self,
        read: impl Fn(&T) -> &U + 'static,
        write: impl Fn(&mut T) -> &mut U + 'static,
    ) -> Binding<U> {
        Binding(Access::Projected(Rc::new(Projection {
            source: self.clone(),
            read,
            write,
        })))
    }

    /// Follow a model's observable field rather than capturing its current instance.
    pub fn follow<U: 'static>(&self, field: impl Fn(&T) -> State<U> + 'static) -> Binding<U> {
        self.try_follow(move |value| Some(field(value)))
    }

    pub fn try_follow<U: 'static>(
        &self,
        field: impl Fn(&T) -> Option<State<U>> + 'static,
    ) -> Binding<U> {
        let source = self.clone();
        Binding::resolve(move || source.with(&field).ok().flatten())
    }

    fn edit<R>(&self, edit: impl FnOnce(&mut T) -> (R, bool)) -> Result<R, Unavailable> {
        untracked(|| {
            let mut edit = Some(edit);
            let mut result = None;
            self.change(&mut |value| {
                let (output, changed) = edit.take().unwrap()(value);
                result = Some(output);
                changed
            })?;
            Ok(result.unwrap())
        })
    }

    fn read_into(&self, read: &mut dyn FnMut(&T)) -> Result<(), Unavailable> {
        match &self.0 {
            Access::State(state) => {
                read(&state.read());
                Ok(())
            }
            Access::Projected(target) => target.read(read),
        }
    }

    fn change(&self, change: &mut dyn FnMut(&mut T) -> bool) -> Result<(), Unavailable> {
        match &self.0 {
            Access::State(state) => {
                state.update_if_changed(|value| ((), change(value)));
                Ok(())
            }
            Access::Projected(target) => target.change(change),
        }
    }
}

trait Target<T> {
    fn read(&self, read: &mut dyn FnMut(&T)) -> Result<(), Unavailable>;
    fn change(&self, change: &mut dyn FnMut(&mut T) -> bool) -> Result<(), Unavailable>;
}

struct Resolved<F>(F);

impl<T: 'static, F: Fn() -> Option<State<T>>> Target<T> for Resolved<F> {
    fn read(&self, read: &mut dyn FnMut(&T)) -> Result<(), Unavailable> {
        let state = (self.0)().ok_or(Unavailable)?;
        read(&state.read());
        Ok(())
    }

    fn change(&self, change: &mut dyn FnMut(&mut T) -> bool) -> Result<(), Unavailable> {
        let state = (self.0)().ok_or(Unavailable)?;
        state.update_if_changed(|value| ((), change(value)));
        Ok(())
    }
}

struct Projection<T, R, W> {
    source: Binding<T>,
    read: R,
    write: W,
}

impl<T: 'static, U, R, W> Target<U> for Projection<T, R, W>
where
    R: Fn(&T) -> &U,
    W: Fn(&mut T) -> &mut U,
{
    fn read(&self, read: &mut dyn FnMut(&U)) -> Result<(), Unavailable> {
        self.source.read_into(&mut |value| read((self.read)(value)))
    }

    fn change(&self, change: &mut dyn FnMut(&mut U) -> bool) -> Result<(), Unavailable> {
        self.source.change(&mut |value| change((self.write)(value)))
    }
}

#[cfg(test)]
mod tests;
