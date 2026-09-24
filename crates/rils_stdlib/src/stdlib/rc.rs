//! Shared ownership handles.

use rils_builtins_macros::decl_rils;

use super::prelude::Option;

#[decl_rils(core::rc)]
mod native {
    use super::Option;

    /// A single-threaded reference-counted owning handle.
    #[rils_struct]
    pub struct Rc<T>(std::rc::Rc<T>);

    #[rils_impl]
    impl<T> Clone for Rc<T> {
        fn clone(&self) -> Self {
            Self(std::rc::Rc::clone(&self.0))
        }
    }

    impl<T> std::ops::Deref for Rc<T> {
        type Target = std::rc::Rc<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Rc<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::rc::Rc<T>> for Rc<T> {
        fn from(value: std::rc::Rc<T>) -> Self {
            Self(value)
        }
    }

    impl<T> From<Rc<T>> for std::rc::Rc<T> {
        fn from(value: Rc<T>) -> Self {
            value.0
        }
    }

    impl<T> Rc<T> {
        /// Creates a new reference-counted handle.
        #[export_rils]
        pub fn new(value: T) -> Self {
            Self(std::rc::Rc::new(value))
        }

        /// Returns the number of strong handles to the shared value.
        #[export_rils]
        pub fn strong_count(&self) -> usize {
            std::rc::Rc::strong_count(&self.0)
        }

        /// Creates a non-owning weak handle to this allocation.
        #[export_rils]
        pub fn downgrade(&self) -> Weak<T> {
            Weak(std::rc::Rc::downgrade(&self.0))
        }
    }

    /// A non-owning reference to an [`Rc<T>`] allocation.
    #[rils_struct]
    pub struct Weak<T>(std::rc::Weak<T>);

    impl<T> std::ops::Deref for Weak<T> {
        type Target = std::rc::Weak<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Weak<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::rc::Weak<T>> for Weak<T> {
        fn from(value: std::rc::Weak<T>) -> Self {
            Self(value)
        }
    }

    impl<T> From<Weak<T>> for std::rc::Weak<T> {
        fn from(value: Weak<T>) -> Self {
            value.0
        }
    }

    impl<T> Weak<T> {
        /// Attempts to upgrade this handle while the allocation is still alive.
        #[export_rils]
        pub fn upgrade(&self) -> Option<Rc<T>> {
            match self.0.upgrade() {
                Some(value) => Option::Some(Rc(value)),
                None => Option::None,
            }
        }

        /// Returns the number of strong owners.
        #[export_rils]
        pub fn strong_count(&self) -> usize {
            self.0.strong_count()
        }

        /// Returns the number of weak handles.
        #[export_rils]
        pub fn weak_count(&self) -> usize {
            self.0.weak_count()
        }
    }
}

pub use native::{Rc, Weak};
