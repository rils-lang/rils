//! Interior-mutable value cell.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::cell)]
mod native {
    /// A single-threaded interior-mutable value cell.
    #[rils_struct]
    pub struct Cell<T>(std::cell::Cell<T>);

    impl<T> std::ops::Deref for Cell<T> {
        type Target = std::cell::Cell<T>;

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
            Self(std::cell::Cell::new(value))
        }

        /// Copies the current value.
        #[export_rils]
        pub fn get(&self) -> T
        where
            T: Copy,
        {
            self.0.get()
        }

        /// Replaces the current value.
        #[export_rils]
        pub fn set(&self, value: T) {
            self.0.set(value);
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        pub fn replace(&self, value: T) -> T {
            self.0.replace(value)
        }
    }

    /// A dynamically borrow-checked interior-mutable value.
    #[rils_struct]
    pub struct RefCell<T>(std::cell::RefCell<T>);

    impl<T> std::ops::Deref for RefCell<T> {
        type Target = std::cell::RefCell<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for RefCell<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> RefCell<T> {
        /// Creates a dynamically checked cell.
        #[export_rils]
        #[rils_legacy_id(core::ref_cell::new)]
        pub fn new(value: T) -> Self {
            Self(std::cell::RefCell::new(value))
        }

        /// Borrows the contained value for reading.
        #[export_rils]
        #[rils_legacy_id(core::ref_cell::borrow)]
        #[rils_return(&T)]
        pub fn borrow(&self) -> std::cell::Ref<'_, T> {
            self.0.borrow()
        }

        /// Borrows the contained value for writing.
        #[export_rils]
        #[rils_legacy_id(core::ref_cell::borrow_mut)]
        #[rils_return(&mut T)]
        pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
            self.0.borrow_mut()
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        #[rils_legacy_id(core::ref_cell::replace)]
        pub fn replace(&self, value: T) -> T {
            self.0.replace(value)
        }

        pub fn get(&mut self) -> &mut T {
            self.0.get_mut()
        }
    }
}

pub use native::{Cell, RefCell};
