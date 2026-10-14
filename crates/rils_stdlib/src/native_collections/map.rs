//! Native keyed collection storage operations.
use rils_native::NativeKey;
use rils_value::{DynamicPathStep, DynamicValue, DynamicValueMut, DynamicValueRef};

pub fn insert(
    mut map: DynamicValueMut<'_>,
    pair: DynamicValue,
    key: &NativeKey,
    ordered: bool,
) -> Result<Option<DynamicValue>, String> {
    map.sequence_borrows()?.check_structural_mutation()?;
    if let Some(index) = find(&map.view(), key)? {
        let mut previous = map.replace_sequence_item(index, pair)?;
        Ok(Some(
            previous.take_path_field(&[DynamicPathStep::Field(1)])?,
        ))
    } else {
        let position = if ordered {
            insertion_index(&map.view(), key)?
        } else {
            map.sequence_len()?
        };
        map.insert_sequence_item(position, pair)?;
        Ok(None)
    }
}

pub fn remove(
    mut map: DynamicValueMut<'_>,
    key: &NativeKey,
) -> Result<Option<DynamicValue>, String> {
    find(&map.view(), key)?
        .map(|index| {
            let mut entry = map.take_sequence_item(index)?;
            entry.take_path_field(&[DynamicPathStep::Field(1)])
        })
        .transpose()
}

pub fn get_cloned(
    map: &DynamicValueRef<'_>,
    key: &NativeKey,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Option<DynamicValue>, String> {
    find(map, key)?
        .map(|index| clone_field(map, index, 1, clone_value))
        .transpose()
}

pub fn boundary_key(
    map: &DynamicValueRef<'_>,
    last: bool,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Option<DynamicValue>, String> {
    let len = map.sequence_len()?;
    if len == 0 {
        return Ok(None);
    }
    clone_field(map, if last { len - 1 } else { 0 }, 0, clone_value).map(Some)
}

pub fn cloned_fields(
    map: &DynamicValueRef<'_>,
    field: usize,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<Vec<DynamicValue>, String> {
    (0..map.sequence_len()?)
        .map(|index| clone_field(map, index, field, clone_value))
        .collect()
}
fn entry_key(entry: &DynamicValue) -> Result<NativeKey, String> {
    crate::native::registry().key(entry.view().field(0)?)
}

pub fn find(map: &DynamicValueRef<'_>, key: &NativeKey) -> Result<Option<usize>, String> {
    for index in 0..map.sequence_len()? {
        if map.with_sequence_item(index, entry_key)?? == *key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn insertion_index(map: &DynamicValueRef<'_>, key: &NativeKey) -> Result<usize, String> {
    for index in 0..map.sequence_len()? {
        if map.with_sequence_item(index, entry_key)?? > *key {
            return Ok(index);
        }
    }
    map.sequence_len()
}

fn clone_field(
    map: &DynamicValueRef<'_>,
    index: usize,
    field: usize,
    clone_value: &mut super::CloneValue<'_>,
) -> Result<DynamicValue, String> {
    map.with_sequence_item(index, |entry| clone_value(entry.view().field(field)?))?
}
