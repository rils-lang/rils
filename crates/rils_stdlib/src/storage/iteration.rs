//! Borrowed iteration supplied by a sequence storage backend.
use crate::stdlib::iterator::Iter;

pub trait SequenceIter {
    type IterOutput<'a>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_>;
}
impl<T> SequenceIter for Vec<T> {
    type IterOutput<'a>
        = Iter<&'a T>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        Iter::from(self.as_slice().iter().collect::<Vec<_>>())
    }
}

/// Owning cursor construction, preserving each backend's iterator representation.
pub trait SequenceIntoIter {
    type Item;
    type IntoIter: Iterator<Item = Self::Item>;
    fn into_iter(self) -> Self::IntoIter;
}
impl<T> SequenceIntoIter for Vec<T> {
    type Item = T;
    type IntoIter = Iter<T>;
    fn into_iter(self) -> Self::IntoIter {
        Iter::from(self)
    }
}
impl SequenceIntoIter for rils_value::OwnedElements {
    type Item = Result<rils_value::DynamicValue, String>;
    type IntoIter = Self;
    fn into_iter(self) -> Self {
        self
    }
}
