//! Heap operations over native and dynamic sequence storage.
use crate::stdlib::{
    collections::{BinaryHeapIntoIter, HeapElement},
    option::Option as RilsOption,
};
use rils_value::{DynamicValue, DynamicValueMut, DynamicValueRef};
use std::{cmp::Ordering, collections::BinaryHeap};

pub trait SequenceHeap<T> {
    type WriteOutput;
    type PopOutput;
    fn push(&mut self, value: T) -> Self::WriteOutput;
    fn pop(&mut self) -> Self::PopOutput;
}
impl<T: HeapElement> SequenceHeap<T> for BinaryHeap<T> {
    type WriteOutput = ();
    type PopOutput = RilsOption<T>;
    fn push(&mut self, value: T) {
        BinaryHeap::push(self, value);
    }
    fn pop(&mut self) -> Self::PopOutput {
        BinaryHeap::pop(self).map_or(RilsOption::None, RilsOption::Some)
    }
}
impl SequenceHeap<DynamicValue> for DynamicValueMut<'_> {
    type WriteOutput = Result<(), String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push(&mut self, value: DynamicValue) -> Self::WriteOutput {
        push(self, value)
    }
    fn pop(&mut self) -> Self::PopOutput {
        pop(self)
    }
}
impl<T> super::SequenceRead for BinaryHeap<T> {
    fn len(&self) -> usize {
        BinaryHeap::len(self)
    }
}
impl<T> super::SequenceClear for BinaryHeap<T> {
    type ClearOutput = ();
    fn clear(&mut self) {
        BinaryHeap::clear(self);
    }
}
impl<T: HeapElement> super::SequenceClone for BinaryHeap<T> {
    type CloneOutput = RilsOption<T>;
    fn front_cloned(&self) -> Self::CloneOutput {
        self.peek()
            .cloned()
            .map_or(RilsOption::None, RilsOption::Some)
    }
    fn back_cloned(&self) -> Self::CloneOutput {
        self.as_slice()
            .last()
            .cloned()
            .map_or(RilsOption::None, RilsOption::Some)
    }
}
impl<T> super::SequenceIntoIter for BinaryHeap<T> {
    type Item = T;
    type IntoIter = BinaryHeapIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}

fn push(value: &mut DynamicValueMut<'_>, item: DynamicValue) -> Result<(), String> {
    crate::stdlib::collections::native_heap_key(item.view())?;
    value.push_sequence_item(item)?;
    let mut index = value.sequence_len()? - 1;
    while index > 0 {
        let parent = (index - 1) / 2;
        if compare(&value.view(), index, parent)? != Ordering::Greater {
            break;
        }
        value.swap_sequence_items(index, parent)?;
        index = parent;
    }
    Ok(())
}

fn pop(value: &mut DynamicValueMut<'_>) -> Result<Option<DynamicValue>, String> {
    let len = value.sequence_len()?;
    if len == 0 {
        return Ok(None);
    }
    // Check before swapping: failed structural mutation must preserve the heap.
    value.sequence_borrows()?.check_structural_mutation()?;
    value.swap_sequence_items(0, len - 1)?;
    let item = value.take_sequence_item(len - 1)?;
    let mut index = 0;
    while index * 2 + 1 < value.sequence_len()? {
        let left = index * 2 + 1;
        let right = left + 1;
        let child = if right < value.sequence_len()?
            && compare(&value.view(), right, left)? == Ordering::Greater
        {
            right
        } else {
            left
        };
        if compare(&value.view(), child, index)? != Ordering::Greater {
            break;
        }
        value.swap_sequence_items(index, child)?;
        index = child;
    }
    Ok(Some(item))
}

fn compare(value: &DynamicValueRef<'_>, left: usize, right: usize) -> Result<Ordering, String> {
    let left = crate::stdlib::collections::native_heap_key(value.sequence_item(left)?)?;
    let right = crate::stdlib::collections::native_heap_key(value.sequence_item(right)?)?;
    Ok(left.cmp(&right))
}

impl<T: HeapElement> super::SequenceNew for BinaryHeap<T> {
    fn new() -> Self {
        BinaryHeap::new()
    }
}
