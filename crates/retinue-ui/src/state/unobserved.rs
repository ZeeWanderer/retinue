use std::{
    cell::{Ref, RefCell},
    fmt,
    rc::Rc,
};

/// Shared bookkeeping with no registration or notifications of its own.
pub struct Unobserved<T>(Rc<RefCell<T>>);

impl<T: Default> Default for Unobserved<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Unobserved<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }

    pub fn read(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.read().clone()
    }

    pub fn replace(&self, value: T) -> T {
        self.0.replace(value)
    }

    pub fn set(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        let old = {
            let mut current = self.0.borrow_mut();
            if *current == value {
                return false;
            }
            std::mem::replace(&mut *current, value)
        };
        drop(old);
        true
    }

    pub fn update<R>(&self, update: impl FnOnce(&mut T) -> R) -> R {
        update(&mut self.0.borrow_mut())
    }
}

impl<T> Clone for Unobserved<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> PartialEq for Unobserved<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for Unobserved<T> {}

impl<T: fmt::Debug> fmt::Debug for Unobserved<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Unobserved").field(&self.0.borrow()).finish()
    }
}

#[cfg(test)]
mod tests;
