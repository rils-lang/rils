//! Map operations over the layout generated from standard-library declarations.

use std::rc::Rc;

use rils_builtins::{BuiltinId, builtin};
use rils_native::NativeKey;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicType, DynamicValue};

use crate::value::{
    DynamicObject, HashKey, OwnedIteratorValue, Value, record_codec::NativeRecordCodec,
};

use super::{NativeOwnedContext, import_receiver};

fn owned_insert_kind(symbol: &str) -> Option<MapKind> {
    [("HashMap", MapKind::Hash), ("BTreeMap", MapKind::BTree)]
        .into_iter()
        .find_map(|(name, kind)| {
            builtin(name)?
                .members
                .iter()
                .any(|member| member.name == "insert" && member.native_symbol == Some(symbol))
                .then_some(kind)
        })
}

fn owned_into_iter_kind(symbol: &str) -> Option<MapKind> {
    [("HashMap", MapKind::Hash), ("BTreeMap", MapKind::BTree)]
        .into_iter()
        .find_map(|(name, kind)| {
            builtin(name)?
                .members
                .iter()
                .any(|member| member.name == "into_iter" && member.native_symbol == Some(symbol))
                .then_some(kind)
        })
}

fn borrowed_iter_kind(symbol: &str) -> Option<MapKind> {
    [("HashMap", MapKind::Hash), ("BTreeMap", MapKind::BTree)]
        .into_iter()
        .find_map(|(name, kind)| {
            builtin(name)?
                .members
                .iter()
                .any(|member| member.name == "iter" && member.native_symbol == Some(symbol))
                .then_some(kind)
        })
}

pub(super) fn is_owned_symbol(symbol: &str) -> bool {
    owned_insert_kind(symbol).is_some()
        || owned_into_iter_kind(symbol).is_some()
        || borrowed_iter_kind(symbol).is_some()
}

pub(super) fn call_owned_symbol(
    symbol: &str,
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if borrowed_iter_kind(symbol).is_some() {
        return super::collection_iter::call_symbol(symbol, &arguments).or_else(|| {
            Some(super::indexed_iter::borrow_map_with_context(
                &arguments, context,
            ))
        });
    }
    if let Some(kind) = owned_into_iter_kind(symbol) {
        return Some((|| {
            if arguments.len() != 1 {
                return Err(format!("{}::into_iter expects one receiver", kind.name()));
            }
            let receiver = arguments.into_iter().next().expect("arity checked");
            match receiver {
                Value::Dynamic(object) if kind.matches(&object) => {
                    crate::iteration::native_sequence_into_iterator(object, context)
                }
                Value::HashMap(map) if kind == MapKind::Hash => {
                    crate::hash_collections::into_iter_map(map)
                }
                Value::BTreeMap(map) if kind == MapKind::BTree => super::btree_map::into_iter(map),
                _ => Err(format!(
                    "{}::into_iter received the wrong collection",
                    kind.name()
                )),
            }
        })());
    }
    let kind = owned_insert_kind(symbol)?;
    Some((|| {
        if arguments.len() != 3 {
            return Err(format!("{}::insert expects a key and value", kind.name()));
        }
        if !matches!(&arguments[0], Value::Reference(reference) if reference.mutable) {
            return Err(format!("{}::insert requires `&mut self`", kind.name()));
        }
        let receiver = import_receiver(&arguments[0])?;
        let Value::Dynamic(object) = receiver else {
            let id = match kind {
                MapKind::Hash => BuiltinId::HashMapInsert,
                MapKind::BTree => BuiltinId::BtreeMapInsert,
            };
            return super::call(id, &arguments);
        };
        if !kind.matches(&object) {
            return Err(format!(
                "{}::insert received the wrong collection",
                kind.name()
            ));
        }
        let pair_layout = object
            .descriptor()
            .layout()
            .sequence_item()
            .ok_or("map has no entry layout")?
            .clone();
        let fields = pair_layout
            .record_fields()
            .ok_or("map entry is not a pair")?;
        if fields.len() != 2 {
            return Err("map entry is not a pair".into());
        }
        let key_layout = fields[0].layout_handle();
        let value_layout = fields[1].layout_handle();
        let mut values = arguments.into_iter();
        values.next();
        let key = values.next().expect("arity checked");
        let value = values.next().expect("arity checked");
        if kind == MapKind::BTree {
            HashKey::from_ordered_value(&key)
                .map_err(|_| "BTreeMap key must be bool, integer, char, or string")?;
        }
        let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
        let native_key = codec.into_native(key, key_layout)?;
        let identity = rils_stdlib::native::registry().key(native_key.view())?;
        let native_value = codec.into_native(value, value_layout.clone())?;
        let pair = DynamicValue::record(pair_layout, vec![native_key, native_value])?;
        let index = object.with(|map| find(map, &identity))??;
        let previous = object
            .with_mut(|map| -> Result<Option<DynamicValue>, String> {
                map.sequence_borrows()?.check_structural_mutation()?;
                if let Some(index) = index {
                    let mut previous = map.replace_sequence_item(index, pair)?;
                    Ok(Some(
                        previous.take_path_field(&[DynamicPathStep::Field(1)])?,
                    ))
                } else {
                    let position = if kind == MapKind::BTree {
                        insertion_index(map, &identity)?
                    } else {
                        map.sequence_len()?
                    };
                    map.insert_sequence_item(position, pair)?;
                    Ok(None)
                }
            })?
            .map_err(|error| mutation_error(kind, error))?;
        native_option(previous, value_layout)
    })())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapKind {
    Hash,
    BTree,
}

impl MapKind {
    fn name(self) -> &'static str {
        match self {
            Self::Hash => "HashMap",
            Self::BTree => "BTreeMap",
        }
    }
    fn matches(self, object: &DynamicObject) -> bool {
        let ty = object.descriptor().layout().rils_type();
        match self {
            Self::Hash => crate::value::native_layouts::hash_map::matches(ty),
            Self::BTree => crate::value::native_layouts::btree_map::matches(ty),
        }
    }
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let (kind, method) = [("HashMap", MapKind::Hash), ("BTreeMap", MapKind::BTree)]
        .into_iter()
        .find_map(|(owner, kind)| {
            ["len", "is_empty"].into_iter().find_map(|method| {
                (builtin(owner)?.member(method)?.native_symbol == Some(symbol))
                    .then_some((kind, method))
            })
        })?;
    Some((|| {
        let [receiver] = arguments else {
            return Err(format!("{}::{method} expects one receiver", kind.name()));
        };
        let length = match import_receiver(receiver)? {
            Value::Dynamic(object) if kind.matches(&object) => {
                object.with(DynamicValue::sequence_len)??
            }
            Value::HashMap(map) if kind == MapKind::Hash => map.entries.borrow().len(),
            Value::BTreeMap(map) if kind == MapKind::BTree => map.entries.borrow().len(),
            _ => {
                return Err(format!(
                    "{}::{method} received the wrong collection",
                    kind.name()
                ));
            }
        };
        Ok(match method {
            "len" => crate::numeric::native_usize(length),
            "is_empty" => Value::Bool(length == 0),
            _ => unreachable!(),
        })
    })())
}

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Option<Result<Value, String>> {
    let kind = if matches!(
        id,
        BuiltinId::HashMapClear
            | BuiltinId::HashMapContainsKey
            | BuiltinId::HashMapInsert
            | BuiltinId::HashMapGetCloned
            | BuiltinId::HashMapRemove
            | BuiltinId::HashMapKeysCloned
            | BuiltinId::HashMapValuesCloned
    ) {
        MapKind::Hash
    } else if matches!(
        id,
        BuiltinId::BtreeMapClear
            | BuiltinId::BtreeMapContainsKey
            | BuiltinId::BtreeMapInsert
            | BuiltinId::BtreeMapGetCloned
            | BuiltinId::BtreeMapRemove
            | BuiltinId::BtreeMapFirstKeyCloned
            | BuiltinId::BtreeMapLastKeyCloned
    ) {
        MapKind::BTree
    } else {
        return None;
    };
    let receiver = arguments.first()?;
    let Value::Dynamic(object) = super::import_receiver(receiver).ok()? else {
        return None;
    };
    if !kind.matches(&object) {
        return None;
    }
    Some(dispatch(kind, id, arguments, receiver, &object))
}

fn dispatch(
    kind: MapKind,
    id: BuiltinId,
    arguments: &[Value],
    receiver: &Value,
    object: &DynamicObject,
) -> Result<Value, String> {
    let pair_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("map has no entry layout")?
        .clone();
    let fields = pair_layout
        .record_fields()
        .ok_or("map entry is not a pair")?;
    if fields.len() != 2 {
        return Err("map entry is not a pair".into());
    }
    let key_layout = fields[0].layout_handle();
    let value_layout = fields[1].layout_handle();
    let mutating = matches!(
        id,
        BuiltinId::HashMapClear
            | BuiltinId::HashMapInsert
            | BuiltinId::HashMapRemove
            | BuiltinId::BtreeMapClear
            | BuiltinId::BtreeMapInsert
            | BuiltinId::BtreeMapRemove
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err(format!(
            "{} mutation requires a mutable reference",
            kind.name()
        ));
    }
    match id {
        BuiltinId::HashMapClear | BuiltinId::BtreeMapClear => {
            object
                .with_mut(DynamicValue::clear_sequence)?
                .map_err(|error| mutation_error(kind, error))?;
            Ok(Value::Unit)
        }
        BuiltinId::HashMapContainsKey
        | BuiltinId::BtreeMapContainsKey
        | BuiltinId::HashMapInsert
        | BuiltinId::BtreeMapInsert
        | BuiltinId::HashMapGetCloned
        | BuiltinId::BtreeMapGetCloned
        | BuiltinId::HashMapRemove
        | BuiltinId::BtreeMapRemove => {
            let (key, native_key) = encode_key(
                kind,
                arguments.get(1).ok_or("missing map key")?,
                key_layout.clone(),
            )?;
            let index = object.with(|map| find(map, &key))??;
            match id {
                BuiltinId::HashMapContainsKey | BuiltinId::BtreeMapContainsKey => {
                    Ok(Value::Bool(index.is_some()))
                }
                BuiltinId::HashMapGetCloned | BuiltinId::BtreeMapGetCloned => {
                    let value = index
                        .map(|index| object.with(|map| clone_field(map, index, 1)))
                        .transpose()?
                        .transpose()?;
                    native_option(value, value_layout)
                }
                BuiltinId::HashMapRemove | BuiltinId::BtreeMapRemove => {
                    let value = if let Some(index) = index {
                        Some(
                            object
                                .with_mut(|map| {
                                    let mut entry = map.take_sequence_item(index)?;
                                    entry.take_path_field(&[DynamicPathStep::Field(1)])
                                })?
                                .map_err(|error| mutation_error(kind, error))?,
                        )
                    } else {
                        None
                    };
                    native_option(value, value_layout)
                }
                BuiltinId::HashMapInsert | BuiltinId::BtreeMapInsert => {
                    let value = arguments.get(2).ok_or("missing map value")?.clone();
                    let native_value =
                        NativeRecordCodec::new().into_native(value, value_layout.clone())?;
                    let pair =
                        DynamicValue::record(pair_layout.clone(), vec![native_key, native_value])?;
                    let previous = object
                        .with_mut(|map| -> Result<Option<DynamicValue>, String> {
                            map.sequence_borrows()?.check_structural_mutation()?;
                            if let Some(index) = index {
                                let mut previous = map.replace_sequence_item(index, pair)?;
                                Ok(Some(
                                    previous.take_path_field(&[DynamicPathStep::Field(1)])?,
                                ))
                            } else {
                                let position = if kind == MapKind::BTree {
                                    insertion_index(map, &key)?
                                } else {
                                    map.sequence_len()?
                                };
                                map.insert_sequence_item(position, pair)?;
                                Ok(None)
                            }
                        })?
                        .map_err(|error| mutation_error(kind, error))?;
                    native_option(previous, value_layout)
                }
                _ => unreachable!(),
            }
        }
        BuiltinId::BtreeMapFirstKeyCloned | BuiltinId::BtreeMapLastKeyCloned => {
            let key = object.with(|map| {
                let len = map.sequence_len()?;
                if len == 0 {
                    return Ok(None);
                }
                let index = if id == BuiltinId::BtreeMapFirstKeyCloned {
                    0
                } else {
                    len - 1
                };
                clone_field(map, index, 0).map(Some)
            })??;
            native_option(key, key_layout)
        }
        BuiltinId::HashMapKeysCloned | BuiltinId::HashMapValuesCloned => {
            let field = usize::from(id == BuiltinId::HashMapValuesCloned);
            let layout = if field == 0 { key_layout } else { value_layout };
            let values = object.with(|map| {
                (0..map.sequence_len()?)
                    .map(|index| {
                        clone_field(map, index, field)
                            .and_then(crate::value::record_codec::from_native)
                    })
                    .collect::<Result<Vec<_>, String>>()
            })??;
            Ok(Value::OwnedIterator(Rc::new(
                OwnedIteratorValue::from_items(values.into(), layout.rils_type().clone()),
            )))
        }
        _ => unreachable!(),
    }
}

fn encode_key(
    kind: MapKind,
    value: &Value,
    layout: Rc<DynamicLayout>,
) -> Result<(NativeKey, DynamicValue), String> {
    let key = match kind {
        MapKind::Hash => HashKey::from_value(value)?,
        MapKind::BTree => HashKey::from_ordered_value(value)
            .map_err(|_| "BTreeMap key must be bool, integer, char, or string".to_owned())?,
    };
    let native = NativeRecordCodec::new().into_native(key.to_value(), layout)?;
    let identity = rils_stdlib::native::registry().key(native.view())?;
    Ok((identity, native))
}

fn entry_key(entry: &DynamicValue) -> Result<NativeKey, String> {
    rils_stdlib::native::registry().key(entry.view().field(0)?)
}

fn find(map: &DynamicValue, key: &NativeKey) -> Result<Option<usize>, String> {
    for index in 0..map.sequence_len()? {
        if map.with_sequence_item(index, entry_key)?? == *key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn insertion_index(map: &DynamicValue, key: &NativeKey) -> Result<usize, String> {
    for index in 0..map.sequence_len()? {
        if map.with_sequence_item(index, entry_key)?? > *key {
            return Ok(index);
        }
    }
    map.sequence_len()
}

fn clone_field(map: &DynamicValue, index: usize, field: usize) -> Result<DynamicValue, String> {
    map.with_sequence_item(index, |entry| {
        rils_stdlib::native::registry().clone_borrowed_view(entry.view().field(field)?)
    })?
}

fn native_option(
    value: Option<DynamicValue>,
    item_layout: Rc<DynamicLayout>,
) -> Result<Value, String> {
    let layout = DynamicLayout::option(item_layout)?;
    let payload = match value {
        Some(value) => DynamicValue::some(layout.clone(), value)?,
        None => DynamicValue::none(layout.clone())?,
    };
    DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).map(Value::Dynamic)
}

fn mutation_error(kind: MapKind, message: String) -> String {
    if message.contains("referenced") || message.contains("iterator") {
        format!(
            "cannot mutate {} while it is borrowed: {message}",
            kind.name()
        )
    } else {
        message
    }
}
