//! Double-ended element movement for owned sequence backends.
use crate::stdlib::{collections::VecDequeIntoIter, option::Option as RilsOption};
use rils_value::{DynamicValue, DynamicValueMut};
use std::collections::VecDeque;

pub trait SequenceDeque<T> {
    type WriteOutput;
    type PopOutput;
    fn push_front(&mut self, value: T) -> Self::WriteOutput;
    fn push_back(&mut self, value: T) -> Self::WriteOutput;
    fn pop_front(&mut self) -> Self::PopOutput;
    fn pop_back(&mut self) -> Self::PopOutput;
}
impl<T> SequenceDeque<T> for VecDeque<T> {
    type WriteOutput = ();
    type PopOutput = RilsOption<T>;
    fn push_front(&mut self, value: T) {
        VecDeque::push_front(self, value);
    }
    fn push_back(&mut self, value: T) {
        VecDeque::push_back(self, value);
    }
    fn pop_front(&mut self) -> Self::PopOutput {
        VecDeque::pop_front(self).map_or(RilsOption::None, RilsOption::Some)
    }
    fn pop_back(&mut self) -> Self::PopOutput {
        VecDeque::pop_back(self).map_or(RilsOption::None, RilsOption::Some)
    }
}
impl SequenceDeque<DynamicValue> for DynamicValueMut<'_> {
    type WriteOutput = Result<(), String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push_front(&mut self, value: DynamicValue) -> Self::WriteOutput {
        self.push_sequence_front(value)
    }
    fn push_back(&mut self, value: DynamicValue) -> Self::WriteOutput {
        self.push_sequence_item(value)
    }
    fn pop_front(&mut self) -> Self::PopOutput {
        if self.sequence_len()? == 0 {
            Ok(None)
        } else {
            self.take_sequence_item(0).map(Some)
        }
    }
    fn pop_back(&mut self) -> Self::PopOutput {
        super::SequenceTransfer::pop(self)
    }
}
impl<T> super::SequenceRead for VecDeque<T> {
    fn len(&self) -> usize {
        VecDeque::len(self)
    }
}
impl<T> super::SequenceTruncate for VecDeque<T> {
    type TruncateOutput = ();
    fn truncate(&mut self, length: usize) {
        VecDeque::truncate(self, length);
    }
}
impl<T> super::SequenceIntoIter for VecDeque<T> {
    type Item = T;
    type IntoIter = VecDequeIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}
