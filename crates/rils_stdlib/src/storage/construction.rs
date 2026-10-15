//! Owned construction backends. The call boundary supplies the concrete result layout.
use rils_value::{DynamicValue, OwnedElements};

pub trait SequenceNew {
    fn new() -> Self;
}

pub trait SequenceFromArray<T>: Sized {
    type Array<const N: usize>: ArrayInput<Storage = Self>;
}

pub trait ArrayInput {
    type Storage;
    type Output<R>;
    fn build<R>(self, wrap: impl FnOnce(Self::Storage) -> R) -> Self::Output<R>;
}

impl<T> SequenceNew for Vec<T> {
    fn new() -> Self {
        Vec::new()
    }
}
impl<T> SequenceNew for std::collections::VecDeque<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T> SequenceFromArray<T> for Vec<T> {
    type Array<const N: usize> = [T; N];
}
impl<T, const N: usize> ArrayInput for [T; N] {
    type Storage = Vec<T>;
    type Output<R> = R;
    fn build<R>(self, wrap: impl FnOnce(Self::Storage) -> R) -> R {
        wrap(Vec::from(self))
    }
}

/// A newly constructed sequence before attaching its nominal layout and codec.
pub struct OwnedSequence(Vec<DynamicValue>);
impl OwnedSequence {
    pub fn from_elements(elements: Vec<DynamicValue>) -> Self {
        Self(elements)
    }

    pub fn into_elements(self) -> Vec<DynamicValue> {
        self.0
    }
}
impl SequenceNew for OwnedSequence {
    fn new() -> Self {
        Self(Vec::new())
    }
}
impl SequenceFromArray<DynamicValue> for OwnedSequence {
    // N belongs to the language signature. The native cursor retains its actual length.
    type Array<const N: usize> = OwnedElements;
}
impl ArrayInput for OwnedElements {
    type Storage = OwnedSequence;
    type Output<R> = Result<R, String>;
    fn build<R>(self, wrap: impl FnOnce(Self::Storage) -> R) -> Result<R, String> {
        self.collect::<Result<Vec<_>, _>>()
            .map(|items| wrap(OwnedSequence(items)))
    }
}
