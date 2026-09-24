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

    /// A stateful sequence producer.
    #[rils_trait]
    pub trait Iterator: ::std::iter::Iterator {
        /// The yielded item type.
        type Item;

        /// Advances the iterator.
        #[rils_legacy_id(core::iterator::next)]
        fn next(&mut self) -> Option<<Self as Iterator>::Item>;
        /// Consumes the iterator and returns the remaining item count.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::count)]
        fn count(self) -> usize;
        /// Consumes the iterator and returns its final item.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::last)]
        fn last(self) -> Option<T>;
        /// Advances to and returns the nth remaining item.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::nth)]
        fn nth(&mut self, index: usize) -> Option<T>;
        /// Consumes the iterator and collects its items into a Vec.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::collect_vec)]
        fn collect_vec(self) -> Vec<T>;
        /// Returns an iterator over at most the first n remaining items.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::take)]
        fn take(self, count: usize) -> Self;
        /// Returns an iterator after discarding the first n remaining items.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::skip)]
        fn skip(self, count: usize) -> Self;
        /// Reverses the remaining items of this double-ended built-in iterator.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::rev)]
        fn rev(self) -> Self;
        /// Transforms every remaining item with the supplied function.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::map)]
        fn map<U>(self, transform: fn(T) -> U) -> Iterator<U>;
        /// Keeps items for which the predicate returns true.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::filter)]
        fn filter(self, predicate: fn(&T) -> bool) -> Iterator<T>;
        /// Transforms and keeps items for which the function returns Some.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::filter_map)]
        fn filter_map<U>(self, transform: fn(T) -> Option<U>) -> Iterator<U>;
        /// Accumulates all remaining items from an initial value.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::fold)]
        fn fold<U>(self, initial: U, accumulate: fn(U, T) -> U) -> U;
        /// Calls a function for every remaining item.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::for_each)]
        fn for_each(self, operation: fn(T) -> ());
        /// Returns true when any item satisfies the predicate.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::any)]
        fn any(self, predicate: fn(T) -> bool) -> bool;
        /// Returns true when every item satisfies the predicate.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::all)]
        fn all(self, predicate: fn(T) -> bool) -> bool;
        /// Returns the first item satisfying the predicate.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::find)]
        fn find(self, predicate: fn(&T) -> bool) -> Option<T>;
        /// Returns the index of the first item satisfying the predicate.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::position)]
        fn position(self, predicate: fn(T) -> bool) -> Option<usize>;
        /// Yields each remaining item together with its zero-based index.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::enumerate)]
        fn enumerate(self) -> Iterator<(usize, T)>;
        /// Returns this iterator unchanged.
        #[rils_provided]
        #[rils_legacy_id(core::iterator::into_iter)]
        fn into_iter(self) -> Self;
    }

    /// Conversion into an iterator.
    #[rils_trait]
    pub trait IntoIterator: ::std::iter::IntoIterator {
        /// The concrete iterator produced by this conversion.
        type IntoIter;

        /// Consumes a value and creates an iterator.
        #[rils_legacy_id(core::sequence::into_iter)]
        fn into_iter(self) -> <Self as IntoIterator>::IntoIter;
    }
}

pub use native::{IntoIterator, Iter, Iterator};
