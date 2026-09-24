//! Interior-mutable value cell.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::cell)]
mod native {
    /// A single-threaded interior-mutable value cell.
    // The temporary empty slot lets `get` clone non-`Copy` values without borrowing them.
    #[rils_struct]
    pub struct Cell<T>(std::cell::Cell<std::option::Option<T>>);

    struct Taken<'a, T> {
        cell: &'a std::cell::Cell<std::option::Option<T>>,
        value: std::option::Option<T>,
    }

    impl<T> Drop for Taken<'_, T> {
        fn drop(&mut self) {
            // A reentrant write wins over the value temporarily taken by `get`.
            let current = self.cell.take();
            self.cell.set(current.or_else(|| self.value.take()));
        }
    }

    impl<T> Cell<T> {
        /// Creates a cell containing a value.
        #[export_rils]
        pub fn new(value: T) -> Self {
            Self(std::cell::Cell::new(Some(value)))
        }

        /// Clones the current value.
        #[export_rils]
        pub fn get(&self) -> T
        where
            T: Clone,
        {
            let taken = Taken {
                cell: &self.0,
                value: self.0.take(),
            };
            let result = taken
                .value
                .as_ref()
                .expect("Cell::get called while its value is temporarily unavailable")
                .clone();
            drop(taken);
            result
        }

        /// Replaces the current value.
        #[export_rils]
        pub fn set(&self, value: T) {
            self.0.set(Some(value));
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        pub fn replace(&self, value: T) -> T {
            self.0
                .replace(Some(value))
                .expect("Cell::replace called while its value is temporarily unavailable")
        }
    }
}

pub use native::Cell;
