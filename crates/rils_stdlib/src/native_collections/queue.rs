//! Native deque and heap operations; no execution-value conversion.

use rils_value::{DynamicValue, DynamicValueMut, DynamicValueRef};
use std::cmp::Ordering;

pub fn pop(mut value: DynamicValueMut<'_>, back: bool) -> Result<Option<DynamicValue>, String> {
    let len = value.sequence_len()?;
    if len == 0 {
        return Ok(None);
    }
    value
        .take_sequence_item(if back { len - 1 } else { 0 })
        .map(Some)
}

pub fn boundary(
    value: DynamicValueRef<'_>,
    back: bool,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Option<DynamicValue>, String> {
    let len = value.sequence_len()?;
    if len == 0 {
        return Ok(None);
    }
    clone_value(value.sequence_item(if back { len - 1 } else { 0 })?).map(Some)
}

pub fn heap_push(mut value: DynamicValueMut<'_>, item: DynamicValue) -> Result<(), String> {
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

pub fn heap_pop(mut value: DynamicValueMut<'_>) -> Result<Option<DynamicValue>, String> {
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
