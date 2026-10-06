//! Owned bridge for collection values with a declaration-derived sequence layout.

use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicPathStep, DynamicType, DynamicValue};

use crate::Type;

use super::{
    DynamicObject, EnumType, StructType, Value, record_codec, record_layout::RecordLayoutResolver,
};

/// An unshared collection can acquire its concrete layout when a binding gives
/// an otherwise untyped constructor its type argument.
pub fn promote_empty(value: Value, expected: &Type) -> Value {
    promote_empty_with_definitions(value, expected, &[], &[])
}

/// Construct an empty collection from its concrete declared type.
pub fn empty_with_definitions(
    ty: &Type,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Value, String> {
    let mut resolver = RecordLayoutResolver::with_enums(structs, enums);
    let layout = resolver.resolve(ty)?;
    empty_with_layout(layout)
}

pub(crate) fn empty_with_layout(layout: Rc<DynamicLayout>) -> Result<Value, String> {
    if layout.sequence_item().is_none() {
        return Err(format!(
            "{} has no native collection layout",
            layout.rils_type()
        ));
    }
    let payload = DynamicValue::sequence(layout.clone(), Vec::new())?;
    Ok(Value::Dynamic(DynamicObject::new(
        Rc::new(DynamicType::new(layout)),
        payload,
    )?))
}

pub fn promote_empty_with_definitions(
    value: Value,
    expected: &Type,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Value {
    promote_empty_with_context(
        value,
        expected,
        &super::storage::TypedStorageContext::new(structs, enums),
    )
}

pub(crate) fn promote_empty_with_context(
    value: Value,
    expected: &Type,
    context: &super::storage::TypedStorageContext<'_>,
) -> Value {
    let supported = match (&value, expected) {
        (Value::Vec(_), Type::Named { name, arguments }) => name == "Vec" && arguments.len() == 1,
        (Value::VecDeque(_), Type::Named { name, arguments }) => {
            name == "VecDeque" && arguments.len() == 1
        }
        (Value::BinaryHeap(_), Type::Named { name, arguments }) => {
            name == "BinaryHeap" && arguments.len() == 1
        }
        (Value::BTreeSet(_), Type::Named { name, arguments }) => {
            name == "BTreeSet" && arguments.len() == 1
        }
        (Value::HashSet(_), Type::Named { name, arguments }) => {
            name == "HashSet" && arguments.len() == 1
        }
        (Value::HashMap(_), Type::Named { name, arguments }) => {
            name == "HashMap" && arguments.len() == 2
        }
        (Value::BTreeMap(_), Type::Named { name, arguments }) => {
            name == "BTreeMap" && arguments.len() == 2
        }
        _ => false,
    };
    if !supported {
        return value;
    }
    let empty_and_unique = match &value {
        Value::Vec(sequence) => {
            Rc::strong_count(sequence) == 1
                && sequence.active_iterators.get() == 0
                && sequence.elements.borrow().is_empty()
        }
        Value::VecDeque(queue) => {
            Rc::strong_count(queue) == 1 && queue.elements.borrow().is_empty()
        }
        Value::BinaryHeap(heap) => Rc::strong_count(heap) == 1 && heap.elements.borrow().is_empty(),
        Value::BTreeSet(set) => {
            Rc::strong_count(set) == 1 && set.borrowed.get() == 0 && set.entries.borrow().is_empty()
        }
        Value::HashSet(set) => {
            Rc::strong_count(set) == 1 && set.borrowed.get() == 0 && set.entries.borrow().is_empty()
        }
        Value::HashMap(map) => {
            Rc::strong_count(map) == 1 && map.borrowed.get() == 0 && map.entries.borrow().is_empty()
        }
        Value::BTreeMap(map) => {
            Rc::strong_count(map) == 1 && map.borrowed.get() == 0 && map.entries.borrow().is_empty()
        }
        _ => false,
    };
    if !empty_and_unique {
        return value;
    }
    context
        .layout(expected)
        .and_then(empty_with_layout)
        .unwrap_or(value)
}

pub fn copy_items(object: &DynamicObject) -> Result<Vec<Value>, String> {
    let items = object.with(|payload| {
        (0..payload.sequence_len()?)
            .map(|index| payload.with_sequence_item(index, clone_item)?)
            .collect::<Result<Vec<_>, _>>()
    })??;
    items.into_iter().map(record_codec::from_native).collect()
}

pub fn view_vec(value: &Value) -> Option<Result<Vec<Value>, String>> {
    match value {
        Value::Vec(sequence) => Some(
            sequence
                .elements
                .borrow()
                .iter()
                .map(|slot| {
                    slot.value
                        .clone()
                        .ok_or("Vec contains a moved element".into())
                })
                .collect(),
        ),
        Value::Dynamic(object)
            if super::native_layouts::vec::matches(object.descriptor().layout().rils_type()) =>
        {
            Some(copy_items(object))
        }
        _ => None,
    }
}

pub fn copy_item(object: &DynamicObject, index: usize) -> Result<Value, String> {
    let item = object.with(|payload| payload.copy_path(&[DynamicPathStep::Index(index)]))??;
    record_codec::from_native(item)
}

/// Read through a Rils reference. Strings are cloned as values for existing
/// Value-based call sites; the sequence retains the original element.
pub fn borrowed_item(object: &DynamicObject, index: usize) -> Result<Value, String> {
    let item = object.with(|payload| payload.with_sequence_item(index, clone_item))???;
    record_codec::from_native(item)
}

pub fn borrowed_item_with_codec(
    object: &DynamicObject,
    index: usize,
    codec: &record_codec::NativeRecordCodec,
) -> Result<Value, String> {
    let item = object.with(|payload| payload.with_sequence_item(index, clone_item))???;
    codec.from_native(item)
}

fn clone_item(item: &DynamicValue) -> Result<DynamicValue, String> {
    crate::value::runtime_layouts::clone_borrowed_element(item)
}

pub fn replace_item(object: &DynamicObject, index: usize, value: Value) -> Result<(), String> {
    let layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("native Vec has no item layout")?
        .clone();
    if !layout.rils_type().accepts(&value) {
        return Err(format!(
            "value is incompatible with element type {}",
            layout.rils_type()
        ));
    }
    let item = record_codec::into_native(value, layout)?;
    object.with_mut(|payload| {
        payload.sequence_borrows()?.check_element_replace(index)?;
        payload.replace_sequence_item(index, item).map(|_| ())
    })??;
    Ok(())
}
