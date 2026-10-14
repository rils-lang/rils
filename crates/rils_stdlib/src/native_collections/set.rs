//! Native keyed collection storage operations.
use rils_native::NativeKey;
use rils_value::{DynamicValue, DynamicValueMut, DynamicValueRef};
use std::collections::{BTreeMap, BTreeSet};

pub fn insert(
    mut set: DynamicValueMut<'_>,
    item: DynamicValue,
    key: &NativeKey,
    ordered: bool,
) -> Result<bool, String> {
    if find(&set.view(), key)?.is_some() {
        return Ok(false);
    }
    let position = if ordered {
        insertion_index(&set.view(), key)?
    } else {
        set.sequence_len()?
    };
    set.insert_sequence_item(position, item)?;
    Ok(true)
}

pub fn remove(mut set: DynamicValueMut<'_>, key: &NativeKey) -> Result<bool, String> {
    if let Some(index) = find(&set.view(), key)? {
        set.take_sequence_item(index)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn boundary(
    set: &DynamicValueRef<'_>,
    last: bool,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Option<DynamicValue>, String> {
    let len = set.sequence_len()?;
    if len == 0 {
        return Ok(None);
    }
    set.with_sequence_item(if last { len - 1 } else { 0 }, |item| {
        clone_value(item.view())
    })?
    .map(Some)
}

pub fn relation(
    left: &DynamicValueRef<'_>,
    right: &DynamicValueRef<'_>,
    method: &str,
) -> Result<bool, String> {
    let left = keys(left)?;
    let right = keys(right)?;
    match method {
        "is_subset" => Ok(left.is_subset(&right)),
        "is_superset" => Ok(left.is_superset(&right)),
        "is_disjoint" => Ok(left.is_disjoint(&right)),
        _ => Err(format!("unsupported set relation `{method}`")),
    }
}

/// Select keys before cloning so only values present in the result are copied.
pub fn algebra(
    left: &DynamicValueRef<'_>,
    right: &DynamicValueRef<'_>,
    method: &str,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Vec<DynamicValue>, String> {
    let left_indices = indices(left)?;
    let right_indices = indices(right)?;
    let left_keys: BTreeSet<_> = left_indices.keys().cloned().collect();
    let right_keys: BTreeSet<_> = right_indices.keys().cloned().collect();
    let selected: BTreeSet<_> = match method {
        "union" => left_keys.union(&right_keys).cloned().collect(),
        "intersection" => left_keys.intersection(&right_keys).cloned().collect(),
        "difference" => left_keys.difference(&right_keys).cloned().collect(),
        "symmetric_difference" => left_keys
            .symmetric_difference(&right_keys)
            .cloned()
            .collect(),
        _ => return Err(format!("unsupported set operation `{method}`")),
    };
    selected
        .into_iter()
        .map(|key| {
            // Preserve the previous right-operand preference for equal keys.
            let (view, index) = if let Some(index) = right_indices.get(&key) {
                (right, *index)
            } else {
                (left, left_indices[&key])
            };
            view.with_sequence_item(index, |item| clone_value(item.view()))?
        })
        .collect()
}

fn indices(set: &DynamicValueRef<'_>) -> Result<BTreeMap<NativeKey, usize>, String> {
    (0..set.sequence_len()?)
        .map(|index| {
            let key =
                set.with_sequence_item(index, |item| crate::native::registry().key(item.view()))??;
            Ok((key, index))
        })
        .collect()
}
pub fn find(set: &DynamicValueRef<'_>, key: &NativeKey) -> Result<Option<usize>, String> {
    for index in 0..set.sequence_len()? {
        let known =
            set.with_sequence_item(index, |item| crate::native::registry().key(item.view()))??;
        if known == *key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn insertion_index(set: &DynamicValueRef<'_>, key: &NativeKey) -> Result<usize, String> {
    for index in 0..set.sequence_len()? {
        let known =
            set.with_sequence_item(index, |item| crate::native::registry().key(item.view()))??;
        if known > *key {
            return Ok(index);
        }
    }
    set.sequence_len()
}

fn keys(set: &DynamicValueRef<'_>) -> Result<BTreeSet<NativeKey>, String> {
    (0..set.sequence_len()?)
        .map(|index| {
            set.with_sequence_item(index, |item| crate::native::registry().key(item.view()))?
        })
        .collect()
}
