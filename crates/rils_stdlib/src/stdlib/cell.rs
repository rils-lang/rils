//! Interior-mutable value cell.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::cell)]
mod native {
    /// A single-threaded interior-mutable value cell.
    #[rils_struct]
    pub struct Cell<T>(std::cell::RefCell<T>);

    impl<T> std::ops::Deref for Cell<T> {
        type Target = std::cell::RefCell<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Cell<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> Cell<T> {
        /// Creates a cell containing a value.
        #[export_rils]
        pub fn new(value: T) -> Self {
            Self(std::cell::RefCell::new(value))
        }

        /// Clones the current value.
        #[export_rils]
        pub fn get(&self) -> T
        where
            T: Clone,
        {
            self.0.borrow().clone()
        }

        /// Replaces the current value.
        #[export_rils]
        pub fn set(&self, value: T) {
            *self.0.borrow_mut() = value;
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        pub fn replace(&self, value: T) -> T {
            self.0.replace(value)
        }
    }
}

pub use native::Cell;
