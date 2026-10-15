//! Owned element transfers for sequence storage backends.
use crate::stdlib::option::Option as RilsOption;
use rils_value::{DynamicValue, DynamicValueMut};

pub trait SequenceTransfer<T> {
    type WriteOutput;
    type TakeOutput;
    type PopOutput;
    fn push(&mut self, value: T) -> Self::WriteOutput;
    fn insert(&mut self, index: usize, value: T) -> Self::WriteOutput;
    fn pop(&mut self) -> Self::PopOutput;
    fn remove(&mut self, index: usize) -> Self::TakeOutput;
    fn swap_remove(&mut self, index: usize) -> Self::TakeOutput;
}
impl<T> SequenceTransfer<T> for Vec<T> {
    type WriteOutput = ();
    type TakeOutput = T;
    type PopOutput = RilsOption<T>;
    fn push(&mut self, value: T) {
        Vec::push(self, value);
    }
    fn insert(&mut self, index: usize, value: T) {
        Vec::insert(self, index, value);
    }
    fn pop(&mut self) -> Self::PopOutput {
        Vec::pop(self).map_or(RilsOption::None, RilsOption::Some)
    }
    fn remove(&mut self, index: usize) -> T {
        Vec::remove(self, index)
    }
    fn swap_remove(&mut self, index: usize) -> T {
        Vec::swap_remove(self, index)
    }
}
impl SequenceTransfer<DynamicValue> for DynamicValueMut<'_> {
    type WriteOutput = Result<(), String>;
    type TakeOutput = Result<DynamicValue, String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push(&mut self, value: DynamicValue) -> Self::WriteOutput {
        self.push_sequence_item(value)
    }
    fn insert(&mut self, index: usize, value: DynamicValue) -> Self::WriteOutput {
        self.insert_sequence_item(index, value)
    }
    fn pop(&mut self) -> Self::PopOutput {
        let len = self.sequence_len()?;
        if len == 0 {
            return Ok(None);
        }
        self.take_sequence_item(len - 1).map(Some)
    }
    fn remove(&mut self, index: usize) -> Self::TakeOutput {
        self.take_sequence_item(index)
    }
    fn swap_remove(&mut self, index: usize) -> Self::TakeOutput {
        self.swap_remove_sequence_item(index)
    }
}
