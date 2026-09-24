//! Iterator adapters used by collection wrappers.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::iter)]
mod native {
    /// An iterator borrowing elements from a sequence.
    #[rils_struct]
    pub struct Iter<T>(std::vec::IntoIter<T>);

    impl<T> From<std::vec::Vec<T>> for Iter<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self(values.into_iter())
        }
    }

    impl<T> std::iter::Iterator for Iter<T> {
        type Item = T;

        fn next(&mut self) -> std::option::Option<T> {
            self.0.next()
        }
    }

    impl<T> Iter<T> {
        /// Advances the iterator and borrows its next item.
        #[export_rils]
        #[rils_legacy_id(core::sequence_iter::next)]
        #[rils_return(Option<T>)]
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> std::option::Option<T> {
            self.0.next()
        }
    }
}

pub use native::Iter;
