//! Hash and B-tree set operations over declaration-derived native storage.

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use rils_builtins::builtin;
use rils_native::NativeKey;
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use crate::value::{DynamicObject, HashKey, Value, record_codec::NativeRecordCodec};

use super::{NativeOwnedContext, import_receiver};

fn owned_insert_kind(symbol: &str) -> Option<SetKind> {
    [("HashSet", SetKind::Hash), ("BTreeSet", SetKind::BTree)]
        .into_iter()
        .find_map(|(name, kind)| {
            builtin(name)?
                .members
                .iter()
                .any(|member| member.name == "insert" && member.native_symbol == Some(symbol))
                .then_some(kind)
        })
}

fn owned_into_iter_kind(symbol: &str) -> Option<SetKind> {
    [("HashSet", SetKind::Hash), ("BTreeSet", SetKind::BTree)]
        .into_iter()
        .find_map(|(name, kind)| {
            builtin(name)?
                .members
                .iter()
                .any(|member| member.name == "into_iter" && member.native_symbol == Some(symbol))
                .then_some(kind)
        })
}

fn borrowed_iter_kind(symbol: &str) -> Option<SetKind> {
    [("HashSet", SetKind::Hash), ("BTreeSet", SetKind::BTree)]
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
            Some(super::indexed_iter::borrow_with_context(
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
                Value::HashSet(set) if kind == SetKind::Hash => {
                    crate::hash_collections::into_iter_set(set)
                }
                Value::BTreeSet(set) if kind == SetKind::BTree => super::btree_set::into_iter(set),
                _ => Err(format!(
                    "{}::into_iter received the wrong collection",
                    kind.name()
                )),
            }
        })());
    }
    let kind = owned_insert_kind(symbol)?;
    Some((|| {
        if arguments.len() != 2 {
            return Err(format!("{}::insert expects one element", kind.name()));
        }
        if !matches!(&arguments[0], Value::Reference(reference) if reference.mutable) {
            return Err(format!("{}::insert requires `&mut self`", kind.name()));
        }
        let receiver = import_receiver(&arguments[0])?;
        let Value::Dynamic(object) = receiver else {
            return match kind {
                SetKind::Hash => crate::hash_collections::call_set("insert", &arguments),
                SetKind::BTree => super::btree_set::call("insert", &arguments),
            };
        };
        if !kind.matches(&object) {
            return Err(format!(
                "{}::insert received the wrong collection",
                kind.name()
            ));
        }
        let item_layout = object
            .descriptor()
            .layout()
            .sequence_item()
            .ok_or("set has no native item layout")?
            .clone();
        let mut values = arguments.into_iter();
        values.next();
        let value = values.next().expect("arity checked");
        if kind == SetKind::BTree {
            HashKey::from_ordered_value(&value)
                .map_err(|_| "BTreeSet elements must be bool, integer, char, or string")?;
        }
        let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
        let native = codec.into_native(value, item_layout)?;
        let key = rils_stdlib::native::registry().key(native.view())?;
        let index = object.with(|set| find(set, &key))??;
        if index.is_some() {
            return Ok(Value::Bool(false));
        }
        object
            .with_mut(|set| {
                let position = if kind == SetKind::BTree {
                    insertion_index(set, &key)?
                } else {
                    set.sequence_len()?
                };
                set.insert_sequence_item(position, native)
            })?
            .map_err(|error| mutation_error(kind, error))?;
        Ok(Value::Bool(true))
    })())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetKind {
    Hash,
    BTree,
}

impl SetKind {
    fn name(self) -> &'static str {
        match self {
            Self::Hash => "HashSet",
            Self::BTree => "BTreeSet",
        }
    }

    fn matches(self, object: &DynamicObject) -> bool {
        let ty = object.descriptor().layout().rils_type();
        match self {
            Self::Hash => crate::value::native_layouts::hash_set::matches(ty),
            Self::BTree => crate::value::native_layouts::btree_set::matches(ty),
        }
    }
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let (kind, member) = [("HashSet", SetKind::Hash), ("BTreeSet", SetKind::BTree)]
        .into_iter()
        .find_map(|(owner, kind)| {
            builtin(owner)?
                .members
                .iter()
                .find(|member| member.native_symbol == Some(symbol))
                .map(|member| (kind, member))
        })?;
    let method = member.name;
    if matches!(method, "new" | "insert" | "iter" | "into_iter") {
        return None;
    }
    Some((|| {
        let expected = member
            .signature
            .map_or(0, |signature| signature.parameters.len())
            + 1;
        if arguments.len() != expected {
            return Err(format!(
                "{}::{method} expects {expected} arguments, found {}",
                kind.name(),
                arguments.len()
            ));
        }
        let receiver = &arguments[0];
        match import_receiver(receiver)? {
            Value::Dynamic(object) if kind.matches(&object) => {
                dispatch(kind, method, arguments, receiver, &object)
            }
            Value::HashSet(_) if kind == SetKind::Hash => {
                crate::hash_collections::call_set(method, arguments)
            }
            Value::BTreeSet(_) if kind == SetKind::BTree => {
                super::btree_set::call(method, arguments)
            }
            _ => Err(format!(
                "{}::{method} received the wrong collection",
                kind.name()
            )),
        }
    })())
}

fn dispatch(
    kind: SetKind,
    method: &str,
    arguments: &[Value],
    receiver: &Value,
    object: &DynamicObject,
) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("set has no native item layout")?
        .clone();
    let mutating = matches!(method, "clear" | "insert" | "remove");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err(format!(
            "{} mutation requires a mutable reference",
            kind.name()
        ));
    }
    match method {
        "len" => Ok(crate::numeric::native_usize(
            object.with(DynamicValue::sequence_len)??,
        )),
        "is_empty" => Ok(Value::Bool(object.with(DynamicValue::sequence_len)?? == 0)),
        "clear" => {
            object
                .with_mut(DynamicValue::clear_sequence)?
                .map_err(|error| mutation_error(kind, error))?;
            Ok(Value::Unit)
        }
        "contains" | "insert" | "remove" => {
            let item = arguments.get(1).ok_or("missing set element")?;
            let (key, native) = encode_key(kind, item, item_layout)?;
            let index = object.with(|set| find(set, &key))??;
            match method {
                "contains" => Ok(Value::Bool(index.is_some())),
                "insert" => {
                    if index.is_some() {
                        return Ok(Value::Bool(false));
                    }
                    object
                        .with_mut(|set| {
                            let position = if kind == SetKind::BTree {
                                insertion_index(set, &key)?
                            } else {
                                set.sequence_len()?
                            };
                            set.insert_sequence_item(position, native)
                        })?
                        .map_err(|error| mutation_error(kind, error))?;
                    Ok(Value::Bool(true))
                }
                "remove" => {
                    if let Some(index) = index {
                        object
                            .with_mut(|set| set.take_sequence_item(index))?
                            .map_err(|error| mutation_error(kind, error))?;
                        Ok(Value::Bool(true))
                    } else {
                        Ok(Value::Bool(false))
                    }
                }
                _ => unreachable!(),
            }
        }
        "first_cloned" | "last_cloned" => {
            let item = object.with(|set| {
                let length = set.sequence_len()?;
                if length == 0 {
                    return Ok(None);
                }
                let index = if method == "first_cloned" {
                    0
                } else {
                    length - 1
                };
                set.with_sequence_item(index, clone_item)?.map(Some)
            })??;
            native_option(item, item_layout)
        }
        "is_subset"
        | "is_superset"
        | "is_disjoint"
        | "union"
        | "intersection"
        | "difference"
        | "symmetric_difference" => {
            let other = other_set(kind, arguments.get(1).ok_or("missing other set")?)?;
            if !object
                .descriptor()
                .layout()
                .compatible_with(other.descriptor().layout())
            {
                return Err(format!("{} element types do not match", kind.name()));
            }
            let left_keys = object.with(keys)??;
            let right_keys = other.with(keys)??;
            match method {
                "is_subset" => Ok(Value::Bool(left_keys.is_subset(&right_keys))),
                "is_superset" => Ok(Value::Bool(left_keys.is_superset(&right_keys))),
                "is_disjoint" => Ok(Value::Bool(left_keys.is_disjoint(&right_keys))),
                _ => {
                    let left = object.with(snapshot)??;
                    let right = other.with(snapshot)??;
                    let keys: BTreeSet<_> = match method {
                        "union" => left_keys.union(&right_keys).cloned().collect(),
                        "intersection" => left_keys.intersection(&right_keys).cloned().collect(),
                        "difference" => left_keys.difference(&right_keys).cloned().collect(),
                        "symmetric_difference" => left_keys
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
        _ => unreachable!(),
    }
}

fn other_set(kind: SetKind, value: &Value) -> Result<DynamicObject, String> {
    let Value::Dynamic(object) = super::import_receiver(value)? else {
        return Err(format!("expected {} receiver", kind.name()));
    };
    if !kind.matches(&object) {
        return Err(format!("expected {} receiver", kind.name()));
    }
    Ok(object)
}

fn encode_key(
    kind: SetKind,
    value: &Value,
    layout: Rc<DynamicLayout>,
) -> Result<(NativeKey, DynamicValue), String> {
    let key = match kind {
        SetKind::Hash => HashKey::from_value(value)?,
        SetKind::BTree => HashKey::from_ordered_value(value)
            .map_err(|_| "BTreeSet elements must be bool, integer, char, or string".to_owned())?,
    };
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

fn mutation_error(kind: SetKind, message: String) -> String {
    if message.contains("referenced") || message.contains("iterator") {
        format!(
            "cannot mutate {} while it is borrowed: {message}",
            kind.name()
        )
    } else {
        message
    }
}
