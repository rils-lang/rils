//! Implementation-only storage capabilities used by standard-library method bodies.
mod construction;
pub use construction::{ArrayInput, OwnedSequence, SequenceFromArray, SequenceNew};

mod access;
mod collections;
mod collections_native;
mod collections_rust;
mod keys;
pub use access::{EntrySchema, EntryShape, MutableAccess, NativeServices, SharedAccess};
pub use collections::*;
pub use keys::{KeyPolicy, KeyQuery};

/// Read-only sequence metadata. Element access and mutation are separate capabilities.
pub trait SequenceRead {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl<T> SequenceRead for std::vec::Vec<T> {
    fn len(&self) -> usize {
        std::vec::Vec::len(self)
    }
}
impl<T> SequenceRead for &[T] {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}
impl SequenceRead for rils_value::IndexedView<'_> {
    fn len(&self) -> usize {
        rils_value::IndexedView::len(self)
    }
}

/// Discard a sequence suffix without exposing its element representation.
/// Backends retain their own mutation failure model.
pub trait SequenceTruncate {
    type TruncateOutput;
    fn truncate(&mut self, length: usize) -> Self::TruncateOutput;
}
impl<T> SequenceTruncate for std::vec::Vec<T> {
    type TruncateOutput = ();
    fn truncate(&mut self, length: usize) {
        std::vec::Vec::truncate(self, length);
    }
}
impl SequenceTruncate for rils_value::DynamicValueMut<'_> {
    type TruncateOutput = Result<(), String>;
    fn truncate(&mut self, length: usize) -> Self::TruncateOutput {
        self.truncate_sequence(length)
    }
}

mod transfer;
pub use transfer::SequenceTransfer;

mod search;
pub use search::{ElementQuery, SequenceSearch};

mod extend;
pub use extend::SequenceExtend;

mod iteration;
pub use iteration::{SequenceIntoIter, SequenceIter};

mod deque;
pub use deque::SequenceDeque;
mod cloning;
pub use cloning::{CloningView, SequenceClone};

mod heap;
pub use heap::SequenceHeap;

/// Clear owned sequence storage without requiring a truncation operation.
pub trait SequenceClear {
    type ClearOutput;
    fn clear(&mut self) -> Self::ClearOutput;
}
impl SequenceClear for rils_value::DynamicValueMut<'_> {
    type ClearOutput = Result<(), String>;
    fn clear(&mut self) -> Self::ClearOutput {
        self.truncate_sequence(0)
    }
}
