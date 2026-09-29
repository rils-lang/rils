//! Iterator adapters used by collection wrappers.

use rils_stdlib_macros::decl_rils;

#[decl_rils(core::iter)]
mod native {
    /// An iterator over owned or borrowed sequence elements.
    #[rils_struct]
    pub struct Iter<T>(IterStorage<T>);

    enum IterStorage<T> {
        Owned(std::vec::IntoIter<T>),
        Generated(std::boxed::Box<dyn std::iter::Iterator<Item = T>>),
    }

    impl<T> From<std::vec::Vec<T>> for Iter<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self(IterStorage::Owned(values.into_iter()))
        }
    }

    impl<T> Iter<T> {
        pub fn into_inner(self) -> std::option::Option<std::vec::IntoIter<T>> {
            match self.0 {
                IterStorage::Owned(values) => Some(values),
                _ => None,
            }
        }
    }

    impl<T: 'static> Iter<T> {
        pub fn from_generator(items: impl std::iter::Iterator<Item = T> + 'static) -> Self {
            Self(IterStorage::Generated(std::boxed::Box::new(items)))
        }
    }

    #[rils_impl]
    impl<T> std::iter::Iterator for Iter<T> {
        type Item = T;

        /// Advances the iterator and borrows its next item.
        fn next(&mut self) -> std::option::Option<T> {
            match &mut self.0 {
                IterStorage::Owned(items) => items.next(),
                IterStorage::Generated(items) => items.next(),
            }
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
        fn count(self) -> usize {
            let mut iterator = self;
            let mut count = 0usize;
            while iterator.next().is_some() {
                count = count + 1usize;
            }
            count
        }
        /// Consumes the iterator and returns its final item.
        fn last(self) -> Option<<Self as Iterator>::Item> {
            let mut values = self.collect_vec();
            values.pop()
        }
        /// Advances to and returns the nth remaining item.
        fn nth(&mut self, index: usize) -> Option<<Self as Iterator>::Item> {
            let mut remaining = index;
            while remaining > 0usize {
                if self.next().is_none() {
                    return None;
                }
                remaining = remaining - 1usize;
            }
            self.next()
        }
        /// Consumes the iterator and collects its items into a Vec.
        fn collect_vec(self) -> Vec<<Self as Iterator>::Item> {
            let mut iterator = self;
            let mut values = Vec::new();
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                values.push(item.unwrap());
            }
            values
        }
        /// Returns an iterator over at most the first n remaining items.
        fn take(self, count: usize) -> Iterator<<Self as Iterator>::Item> {
            let mut iterator = self;
            let mut values = Vec::new();
            let mut remaining = count;
            while remaining > 0usize {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                values.push(item.unwrap());
                remaining = remaining - 1usize;
            }
            values.into_iter()
        }
        /// Returns an iterator after discarding the first n remaining items.
        fn skip(self, count: usize) -> Iterator<<Self as Iterator>::Item> {
            let mut iterator = self;
            let mut skipped = 0usize;
            let mut values = Vec::new();
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                if skipped >= count {
                    values.push(item.unwrap());
                }
                skipped = skipped + 1usize;
            }
            values.into_iter()
        }
        /// Reverses the remaining items of this double-ended built-in iterator.
        fn rev(self) -> Iterator<<Self as Iterator>::Item> {
            let mut values = self.collect_vec();
            let mut reversed = Vec::new();
            loop {
                let item = values.pop();
                if item.is_none() {
                    break;
                }
                reversed.push(item.unwrap());
            }
            reversed.into_iter()
        }
        /// Transforms every remaining item with the supplied function.
        fn map<U>(self, transform: fn(<Self as Iterator>::Item) -> U) -> Iterator<U> {
            let mut iterator = self;
            let mut values = Vec::new();
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                values.push(transform(item.unwrap()));
            }
            values.into_iter()
        }
        /// Keeps items for which the predicate returns true.
        fn filter(
            self,
            predicate: fn(&<Self as Iterator>::Item) -> bool,
        ) -> Iterator<<Self as Iterator>::Item> {
            let mut iterator = self;
            let mut values = Vec::new();
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                let value = item.unwrap();
                if predicate(&value) {
                    values.push(value);
                }
            }
            values.into_iter()
        }
        /// Transforms and keeps items for which the function returns Some.
        fn filter_map<U>(
            self,
            transform: fn(<Self as Iterator>::Item) -> Option<U>,
        ) -> Iterator<U> {
            let mut iterator = self;
            let mut values = Vec::new();
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                let mapped = transform(item.unwrap());
                if mapped.is_some() {
                    values.push(mapped.unwrap());
                }
            }
            values.into_iter()
        }
        /// Accumulates all remaining items from an initial value.
        fn fold<U>(self, initial: U, accumulate: fn(U, <Self as Iterator>::Item) -> U) -> U {
            let mut iterator = self;
            let mut result = initial;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                result = accumulate(result, item.unwrap());
            }
            result
        }
        /// Calls a function for every remaining item.
        fn for_each(self, operation: fn(<Self as Iterator>::Item) -> ()) {
            let mut iterator = self;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                operation(item.unwrap());
            }
        }
        /// Returns true when any item satisfies the predicate.
        fn any(self, predicate: fn(<Self as Iterator>::Item) -> bool) -> bool {
            let mut iterator = self;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    return false;
                }
                if predicate(item.unwrap()) {
                    return true;
                }
            }
        }
        /// Returns true when every item satisfies the predicate.
        fn all(self, predicate: fn(<Self as Iterator>::Item) -> bool) -> bool {
            let mut iterator = self;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    return true;
                }
                if !predicate(item.unwrap()) {
                    return false;
                }
            }
        }
        /// Returns the first item satisfying the predicate.
        fn find(
            self,
            predicate: fn(&<Self as Iterator>::Item) -> bool,
        ) -> Option<<Self as Iterator>::Item> {
            let mut iterator = self;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    return None;
                }
                let value = item.unwrap();
                if predicate(&value) {
                    return Some(value);
                }
            }
        }
        /// Returns the index of the first item satisfying the predicate.
        fn position(self, predicate: fn(<Self as Iterator>::Item) -> bool) -> Option<usize> {
            let mut iterator = self;
            let mut index = 0usize;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    return None;
                }
                if predicate(item.unwrap()) {
                    return Some(index);
                }
                index = index + 1usize;
            }
        }
        /// Yields each remaining item together with its zero-based index.
        fn enumerate(self) -> Iterator<(usize, <Self as Iterator>::Item)> {
            let mut iterator = self;
            let mut values = Vec::new();
            let mut index = 0usize;
            loop {
                let item = iterator.next();
                if item.is_none() {
                    break;
                }
                values.push((index, item.unwrap()));
                index = index + 1usize;
            }
            values.into_iter()
        }
    }

    /// Conversion into an iterator.
    #[rils_trait]
    pub trait IntoIterator: ::std::iter::IntoIterator {
        /// The item yielded by the resulting iterator.
        type Item;

        /// The concrete iterator produced by this conversion.
        type IntoIter;

        /// Consumes a value and creates an iterator.
        fn into_iter(self) -> <Self as IntoIterator>::IntoIter;
    }
}

pub use native::{IntoIterator, Iter, Iterator};

/// A blanket relationship supplied by the Rust standard library for every
/// iterator. Rils uses the same relationship when checking trait bounds.
pub const BLANKET_TRAIT_IMPLS: &[(&str, &str)] = &[("Iterator", "IntoIterator")];
