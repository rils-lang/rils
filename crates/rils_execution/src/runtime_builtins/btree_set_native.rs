//! B-tree set operations over declaration-derived native sequence storage.

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use rils_builtins::BuiltinId;
use rils_native::NativeKey;
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use crate::value::{
    DynamicObject, HashKey, OwnedIteratorValue, Value, record_codec::NativeRecordCodec,
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Option<Result<Value, String>> {
    if !matches!(
        id,
        BuiltinId::BtreeSetLen
            | BuiltinId::BtreeSetIsEmpty
            | BuiltinId::BtreeSetClear
            | BuiltinId::BtreeSetContains
            | BuiltinId::BtreeSetInsert
            | BuiltinId::BtreeSetRemove
            | BuiltinId::BtreeSetFirstCloned
            | BuiltinId::BtreeSetLastCloned
            | BuiltinId::BtreeSetIsSubset
            | BuiltinId::BtreeSetIsSuperset
            | BuiltinId::BtreeSetIsDisjoint
            | BuiltinId::BtreeSetUnion
            | BuiltinId::BtreeSetIntersection
            | BuiltinId::BtreeSetDifference
            | BuiltinId::BtreeSetSymmetricDifference
            | BuiltinId::BtreeSetIter
            | BuiltinId::BtreeSetIntoIter
    ) {
        return None;
    }
    let receiver = arguments.first()?;
    let Value::Dynamic(object) = super::import_receiver(receiver).ok()? else {
        return None;
    };
    if !crate::value::native_layouts::btree_set::matches(object.descriptor().layout().rils_type()) {
        return None;
    }
    Some(dispatch(id, arguments, receiver, &object))
}

fn dispatch(
    id: BuiltinId,
    arguments: &[Value],
    receiver: &Value,
    object: &DynamicObject,
) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("BTreeSet has no native item layout")?
        .clone();
    let mutating = matches!(
        id,
        BuiltinId::BtreeSetClear | BuiltinId::BtreeSetInsert | BuiltinId::BtreeSetRemove
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeSet mutation requires a mutable reference".into());
    }
    match id {
        BuiltinId::BtreeSetLen => Ok(crate::numeric::native_usize(
            object.with(DynamicValue::sequence_len)??,
        )),
        BuiltinId::BtreeSetIsEmpty => {
            Ok(Value::Bool(object.with(DynamicValue::sequence_len)?? == 0))
        }
        BuiltinId::BtreeSetClear => {
            object
                .with_mut(DynamicValue::clear_sequence)?
                .map_err(mutation_error)?;
            Ok(Value::Unit)
        }
        BuiltinId::BtreeSetContains | BuiltinId::BtreeSetInsert | BuiltinId::BtreeSetRemove => {
            let item = arguments.get(1).ok_or("missing BTreeSet element")?;
            let (key, native) = encode_key(item, item_layout)?;
            let index = object.with(|set| find(set, &key))??;
            match id {
                BuiltinId::BtreeSetContains => Ok(Value::Bool(index.is_some())),
                BuiltinId::BtreeSetInsert => {
                    if index.is_some() {
                        return Ok(Value::Bool(false));
                    }
                    object
                        .with_mut(|set| {
                            let position = insertion_index(set, &key)?;
                            set.insert_sequence_item(position, native)
                        })?
                        .map_err(mutation_error)?;
                    Ok(Value::Bool(true))
                }
                BuiltinId::BtreeSetRemove => {
                    if let Some(index) = index {
                        object
                            .with_mut(|set| set.take_sequence_item(index))?
                            .map_err(mutation_error)?;
                        Ok(Value::Bool(true))
                    } else {
                        Ok(Value::Bool(false))
                    }
                }
                _ => unreachable!(),
            }
        }
        BuiltinId::BtreeSetFirstCloned | BuiltinId::BtreeSetLastCloned => {
            let item = object.with(|set| {
                let length = set.sequence_len()?;
                if length == 0 {
                    return Ok(None);
                }
                let index = if id == BuiltinId::BtreeSetFirstCloned {
                    0
                } else {
                    length - 1
                };
                set.with_sequence_item(index, clone_item)?.map(Some)
            })??;
            native_option(item, item_layout)
        }
        BuiltinId::BtreeSetIsSubset
        | BuiltinId::BtreeSetIsSuperset
        | BuiltinId::BtreeSetIsDisjoint
        | BuiltinId::BtreeSetUnion
        | BuiltinId::BtreeSetIntersection
        | BuiltinId::BtreeSetDifference
        | BuiltinId::BtreeSetSymmetricDifference => {
            let other = other_set(arguments.get(1).ok_or("missing other BTreeSet")?)?;
            if !object
                .descriptor()
                .layout()
                .compatible_with(other.descriptor().layout())
            {
                return Err("BTreeSet element types do not match".into());
            }
            let left_keys = object.with(keys)??;
            let right_keys = other.with(keys)??;
            match id {
                BuiltinId::BtreeSetIsSubset => Ok(Value::Bool(left_keys.is_subset(&right_keys))),
                BuiltinId::BtreeSetIsSuperset => {
                    Ok(Value::Bool(left_keys.is_superset(&right_keys)))
                }
                BuiltinId::BtreeSetIsDisjoint => {
                    Ok(Value::Bool(left_keys.is_disjoint(&right_keys)))
                }
                _ => {
                    let left = object.with(snapshot)??;
                    let right = other.with(snapshot)??;
                    let keys: BTreeSet<_> = match id {
                        BuiltinId::BtreeSetUnion => left_keys.union(&right_keys).cloned().collect(),
                        BuiltinId::BtreeSetIntersection => {
                            left_keys.intersection(&right_keys).cloned().collect()
                        }
                        BuiltinId::BtreeSetDifference => {
                            left_keys.difference(&right_keys).cloned().collect()
                        }
                        BuiltinId::BtreeSetSymmetricDifference => left_keys
                            .symmetric_difference(&right_keys)
                            .cloned()
                            .collect(),
                        _ => unreachable!(),
                    };
                    let mut values = left.into_iter().chain(right).collect::<BTreeMap<_, _>>();
                    let items = keys
                        .into_iter()
                        .map(|key| values.remove(&key).expect("set operand owns its key"))
                        .collect();
                    native_set(object.descriptor().layout_handle(), items)
                }
            }
        }
        BuiltinId::BtreeSetIter => super::indexed_iter::borrow(arguments),
        BuiltinId::BtreeSetIntoIter => {
            let items = object
                .with_mut(DynamicValue::take_all_sequence_items)?
                .map_err(mutation_error)?;
            let values = items
                .into_iter()
                .map(crate::value::record_codec::from_native)
                .collect::<Result<_, _>>()?;
            Ok(Value::OwnedIterator(Rc::new(
                OwnedIteratorValue::from_items(values, item_layout.rils_type().clone()),
            )))
        }
        _ => unreachable!(),
    }
}

fn other_set(value: &Value) -> Result<DynamicObject, String> {
    let Value::Dynamic(object) = super::import_receiver(value)? else {
        return Err("expected BTreeSet receiver".into());
    };
    if !crate::value::native_layouts::btree_set::matches(object.descriptor().layout().rils_type()) {
        return Err("expected BTreeSet receiver".into());
    }
    Ok(object)
}

fn encode_key(
    value: &Value,
    layout: Rc<DynamicLayout>,
) -> Result<(NativeKey, DynamicValue), String> {
    let key = HashKey::from_ordered_value(value)
        .map_err(|_| "BTreeSet elements must be bool, integer, char, or string".to_owned())?;
    let native = NativeRecordCodec::new().into_native(key.to_value(), layout)?;
    let identity = rils_stdlib::native::registry().key(native.view())?;
    Ok((identity, native))
}

fn find(set: &DynamicValue, key: &NativeKey) -> Result<Option<usize>, String> {
    for index in 0..set.sequence_len()? {
        let known = set.with_sequence_item(index, |item| {
            rils_stdlib::native::registry().key(item.view())
        })??;
        if known == *key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn insertion_index(set: &DynamicValue, key: &NativeKey) -> Result<usize, String> {
    for index in 0..set.sequence_len()? {
        let known = set.with_sequence_item(index, |item| {
            rils_stdlib::native::registry().key(item.view())
        })??;
        if known > *key {
            return Ok(index);
        }
    }
    set.sequence_len()
}

fn clone_item(item: &DynamicValue) -> Result<DynamicValue, String> {
    rils_stdlib::native::registry().clone_borrowed_view(item.view())
}

fn snapshot(set: &DynamicValue) -> Result<Vec<(NativeKey, DynamicValue)>, String> {
    (0..set.sequence_len()?)
        .map(|index| {
            set.with_sequence_item(index, |item| {
                Ok((
                    rils_stdlib::native::registry().key(item.view())?,
                    clone_item(item)?,
                ))
            })?
        })
        .collect()
}

fn keys(set: &DynamicValue) -> Result<BTreeSet<NativeKey>, String> {
    (0..set.sequence_len()?)
        .map(|index| {
            set.with_sequence_item(index, |item| {
                rils_stdlib::native::registry().key(item.view())
            })?
        })
        .collect()
}

fn native_value(layout: Rc<DynamicLayout>, value: DynamicValue) -> Result<Value, String> {
    let descriptor = Rc::new(DynamicType::new(layout));
    DynamicObject::new(descriptor, value).map(Value::Dynamic)
}

fn native_set(layout: Rc<DynamicLayout>, items: Vec<DynamicValue>) -> Result<Value, String> {
    let value = DynamicValue::sequence(layout.clone(), items)?;
    native_value(layout, value)
}

fn native_option(
    item: Option<DynamicValue>,
    item_layout: Rc<DynamicLayout>,
) -> Result<Value, String> {
    let layout = DynamicLayout::option(item_layout)?;
    let value = match item {
        Some(item) => DynamicValue::some(layout.clone(), item)?,
        None => DynamicValue::none(layout.clone())?,
    };
    native_value(layout, value)
}

fn mutation_error(message: String) -> String {
    if message.contains("referenced") || message.contains("iterator") {
        format!("cannot mutate BTreeSet while it is borrowed: {message}")
    } else {
        message
    }
}
